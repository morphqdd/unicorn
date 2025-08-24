use crate::frontend::parser::ast::expr::Expr;
use crate::general_compiler::runtime::init_runtime;
use crate::general_compiler::type_def::{Field, TypeDef};
use anyhow::*;
use cranelift::codegen::ir::BlockArg;
use cranelift::module::{FuncOrDataId, Linkage};
use cranelift::prelude::{Imm64, IntCC, MemFlags, TrapCode};
use cranelift::{
    codegen::Context,
    module::{DataDescription, Module},
    prelude::{
        AbiParam, Block, FunctionBuilder, FunctionBuilderContext, InstBuilder, Value, Variable,
        types,
    },
};
use std::{collections::HashMap, ops::DerefMut};

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
    fn translate(self, exprs: Vec<Expr>) -> Result<Self>
    where
        Self: Sized,
    {
        let (mut builder_ctx, mut ctx, data_description, mut module) = self.unwrap();

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

        let process_ptr = builder.use_var(runtime.processes_ptr);
        call_free(&mut module, &mut builder, process_ptr);

        let zero = builder.ins().iconst(target_type, 0);
        builder.ins().return_(&[zero]);

        let id = module.declare_function("main", Linkage::Export, &mut builder.func.signature)?;
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
    let trap_block = builder.create_block();

    builder.ins().jump(cond_block, &[BlockArg::Value(ptr)]);

    builder.switch_to_block(cond_block);
    builder.seal_block(cond_block);
    builder.append_block_param(cond_block, ty);

    let ptr = *builder.block_params(cond_block).get(0).unwrap();

    let is_null = builder.ins().icmp_imm(IntCC::Equal, ptr, 0);
    builder.ins().brif(
        is_null,
        trap_block,
        &[],
        block_after_call,
        &[BlockArg::Value(ptr)],
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
