use crate::frontend::parser::ast::expr::Expr;
use crate::general_compiler::runtime::init_runtime;
use anyhow::*;
use base64ct::{Base64, Encoding};
use cranelift::codegen::ir::BlockArg;
use cranelift::module::Linkage;
use cranelift::prelude::{IntCC, MemFlags, TrapCode};
use cranelift::{
    codegen::Context,
    module::{DataDescription, Module},
    prelude::{
        AbiParam, Block, FunctionBuilder, FunctionBuilderContext, InstBuilder, Value,
    },
};
use whirlpool::Digest;
use crate::aot::STORE_FUNCTIONS;
use crate::general_compiler::runtime::virtual_process::create_process;

mod function_translator;
mod runtime;
mod type_def;
const REDUCTIONS_LIMIT: i64 = 2;

pub trait GeneralCompiler<T: Module> {
    fn from_general_compiler(
        builder_ctx: FunctionBuilderContext,
        ctx: Context,
        data_description: DataDescription,
        module: T,
    ) -> Self
    where
        Self: Sized;
    fn unwrap(self) -> (FunctionBuilderContext, Context, DataDescription, T);
    fn translate(self, _exprs: Vec<Expr>) -> Result<Self>
    where
        Self: Sized,
    {
        let (mut builder_ctx, mut ctx, data_description, mut module) = self.unwrap();

        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);
        let target_type = module.target_config().pointer_type();

        builder
            .func
            .signature
            .params
            .push(AbiParam::new(target_type));

        builder
            .func
            .signature
            .returns
            .push(AbiParam::new(target_type));

        let entry_block = builder.create_block();
        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);
        builder.append_block_param(entry_block, target_type);
        let v = *builder.block_params(entry_block).get(0).unwrap();
        call_stdprint(&mut module, &mut builder, v);
        let zero = builder.ins().iconst(target_type,0);
        builder.ins().return_(&[zero]);

        let mut whirlpool = whirlpool::Whirlpool::default();
        let name = b"main";
        Digest::update(&mut whirlpool, name);
        let hash = whirlpool.finalize();
        let hash = Base64::encode_string(hash.as_ref());

        let id = module
            .declare_function(
                &hash,
                Linkage::Export,
                &builder.func.signature
            )?;

        STORE_FUNCTIONS.write().unwrap()
            .insert(id, builder.func.signature.clone());
        builder.finalize();
        module.define_function(id, &mut ctx)?;
        module.clear_context(&mut ctx);

        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);
        let target_type = module.target_config().pointer_type();

        builder
            .func
            .signature
            .returns
            .push(AbiParam::new(target_type));

        let entry_block = builder.create_block();
        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);

        let after_build_runtime_block = builder.create_block();
        let runtime = init_runtime(&mut module, &mut builder, after_build_runtime_block);
        builder.switch_to_block(after_build_runtime_block);
        builder.seal_block(after_build_runtime_block);

        let processes_ptr = builder.use_var(runtime.processes_ptr);

        let new_process = create_process(&mut module, &mut builder);
        let ptr = builder.use_var(new_process);
        builder.ins().store(MemFlags::new(), ptr, processes_ptr, 0);

        let process_ptr = builder
            .ins().load(target_type, MemFlags::new(), processes_ptr, 0);
        let process_ctx_ptr = builder
            .ins().load(target_type, MemFlags::new(), process_ptr, 0);
        let func_addr = builder
            .ins().load(target_type, MemFlags::new(), process_ctx_ptr, 0);

        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(target_type));
        sig.returns.push(AbiParam::new(target_type));
        let sig_ref= builder.import_signature(sig);
        let v = builder.ins().iconst(target_type, 20);

        builder.ins()
            .call_indirect(sig_ref, func_addr, &[v]);

        call_free(&mut module, &mut builder, process_ctx_ptr);
        call_free(&mut module, &mut builder, process_ptr);
        call_free(&mut module, &mut builder, processes_ptr);

        let zero = builder.ins().iconst(target_type, 0);
        builder.ins().return_(&[zero]);


        let id = module.declare_function("main", Linkage::Export, &mut builder.func.signature)?;
        builder.finalize();
        module.define_function(id, &mut ctx)?;
        module.clear_context(&mut ctx);

        Ok(Self::from_general_compiler(
            builder_ctx,
            ctx,
            data_description,
            module,
        ))
    }
}

pub fn call_malloc(
    module: &mut dyn Module,
    builder: &mut FunctionBuilder,
    buffer_size: Value,
    block_after_call: Block,
    block_args: &[BlockArg]
) {
    let ty = module.target_config().pointer_type();
    let mut malloc_sig = module.make_signature();
    malloc_sig.params.push(AbiParam::new(ty));
    malloc_sig.returns.push(AbiParam::new(ty));

    let callee_malloc = module
        .declare_function("malloc", Linkage::Import, &malloc_sig)
        .unwrap();
    let local_callee_malloc = module.declare_func_in_func(callee_malloc, builder.func);

    let call = builder.ins().call(local_callee_malloc, &[buffer_size]);
    let ptr: Value = *builder.inst_results(call).get(0).unwrap();

    let cond_block = builder.create_block();
    for _ in block_args {
        builder.append_block_param(cond_block,ty);
    }
    builder.append_block_param(cond_block, ty);

    let trap_block = builder.create_block();

    builder.ins().jump(
        cond_block,
        &[
            block_args,
            &[BlockArg::Value(ptr)]
        ].concat()
    );

    builder.switch_to_block(cond_block);
    builder.seal_block(cond_block);

    let len = builder.block_params(cond_block).len();
    let ptr = *builder.block_params(cond_block).last().unwrap();
    let block_args: Vec<BlockArg> =
        (&builder.block_params(cond_block)[..len-1])
            .iter()
            .map(|x| BlockArg::Value(*x))
            .collect();

    let is_null = builder.ins().icmp_imm(IntCC::Equal, ptr, 0);
    builder.ins().brif(
        is_null,
        trap_block,
        &[],
        block_after_call,
        &[&block_args[..], &[BlockArg::Value(ptr)]].concat(),
    );

    builder.switch_to_block(trap_block);
    builder.seal_block(trap_block);
    builder.ins().trap(TrapCode::HEAP_OUT_OF_BOUNDS);
}

pub fn call_free(module: &mut dyn Module, builder: &mut FunctionBuilder, ptr: Value) -> Value {
    let ty = module.target_config().pointer_type();
    let mut free_sig = module.make_signature();
    free_sig.params.push(AbiParam::new(ty));
    free_sig.returns.push(AbiParam::new(ty));

    let callee_free = module
        .declare_function("free", Linkage::Import, &free_sig)
        .unwrap();
    let local_callee_free = module.declare_func_in_func(callee_free, builder.func);

    let call = builder.ins().call(local_callee_free, &[ptr]);
    *builder.inst_results(call).get(0).unwrap()
}

pub fn call_stdprint(module: &mut dyn Module, builder: &mut FunctionBuilder, value: Value) {
    let ty = module.target_config().pointer_type();
    let mut print_sig = module.make_signature();
    print_sig.params.push(AbiParam::new(ty));
    print_sig.returns.push(AbiParam::new(ty));

    let callee_print = module
        .declare_function("stdprint", Linkage::Import, &print_sig)
        .unwrap();
    let local_callee_print = module.declare_func_in_func(callee_print, builder.func);

    let call = builder.ins().call(local_callee_print, &[value]);
}