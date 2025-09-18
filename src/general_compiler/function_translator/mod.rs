use crate::aot::STORE_FUNCTIONS;
use crate::frontend::parser::ast::expr::Expr;
use crate::general_compiler::call_malloc;
use crate::general_compiler::trap::CompilerTrapCode;
use anyhow::anyhow;
use base64ct::{Base64, Encoding};
use cranelift::codegen::Context;
use cranelift::codegen::ir::{BlockArg, BlockCall, JumpTable, ValueListPool};
use cranelift::frontend::{FunctionBuilder, Variable};
use cranelift::module::{FuncId, Linkage, Module};
use cranelift::prelude::{
    AbiParam, Block, FunctionBuilderContext, InstBuilder, IntCC, JumpTableData, MemFlags, TrapCode,
    Value, types,
};
use std::collections::HashMap;
use std::ops::DerefMut;
use whirlpool::Digest;

pub struct FunctionTranslator<'a> {
    int: types::Type,
    variables: HashMap<String, usize>,
    module: &'a mut dyn Module,
    ctx: &'a mut Context,
    builder_ctx: &'a mut FunctionBuilderContext,
}

impl<'a> FunctionTranslator<'a> {
    pub fn new(
        int: types::Type,
        variables: HashMap<String, usize>,
        module: &'a mut dyn Module,
        ctx: &'a mut Context,
        builder_ctx: &'a mut FunctionBuilderContext,
    ) -> Self {
        Self {
            int,
            variables,
            module,
            ctx,
            builder_ctx,
        }
    }
}

struct TranslatePack<'a>(
    types::Type,
    &'a mut HashMap<String, usize>,
    &'a mut dyn Module,
);

pub fn translate(ft: FunctionTranslator, function: Expr) -> anyhow::Result<()> {
    let FunctionTranslator {
        int,
        mut variables,
        module,
        ctx,
        builder_ctx,
    } = ft;
    let mut builder = FunctionBuilder::new(&mut ctx.func, builder_ctx);
    match function {
        Expr::Function {
            name,
            function_ty,
            body,
        } => match *function_ty {
            Expr::FunctionType { params, .. } => {
                let mut sig = module.make_signature();

                let entry_block = builder.create_block();

                sig.params.push(AbiParam::new(int));
                sig.params.push(AbiParam::new(int));
                builder.append_block_param(entry_block, int);
                builder.append_block_param(entry_block, int);

                let block_index: Value = *builder.block_params(entry_block).first().unwrap();

                builder.append_block_params_for_function_params(entry_block);
                sig.returns.push(AbiParam::new(int));

                let mut func_blocks = vec![];

                for expr in body {
                    let (val, blocks) = translate_expr(
                        expr,
                        &mut builder,
                        TranslatePack(int, &mut variables, module),
                    );
                    for block in blocks {
                        func_blocks.push(BlockCall::new(block, [], &mut ValueListPool::new()));
                    }
                }

                let trap_block = builder.create_block();
                builder.switch_to_block(trap_block);
                builder
                    .ins()
                    .trap(TrapCode::from(CompilerTrapCode::EndOfBlocks));

                builder.switch_to_block(entry_block);
                builder.seal_block(entry_block);

                let mut value_list_pool = ValueListPool::new();

                let jt = builder.create_jump_table(JumpTableData::new(
                    BlockCall::new(trap_block, [], &mut value_list_pool),
                    &func_blocks,
                ));

                builder.ins().br_table(block_index, jt);

                let mut whirlpool = whirlpool::Whirlpool::default();
                let name = b"main";
                Digest::update(&mut whirlpool, name);
                let hash = whirlpool.finalize();
                let hash = Base64::encode_string(hash.as_ref());

                let id =
                    module.declare_function(&hash, Linkage::Export, &builder.func.signature)?;

                STORE_FUNCTIONS
                    .write()
                    .unwrap()
                    .insert(id, builder.func.signature.clone());
                builder.finalize();
                module.define_function(id, ctx)?;
                module.clear_context(ctx);
                return Ok(());
            }
            _ => return Err(anyhow!("Not a function type")),
        },
        _ => return Err(anyhow!("Not a function")),
    }
    Err(anyhow!("Compile function error"))
}

fn translate_expr(
    expr: Expr,
    builder: &mut FunctionBuilder,
    tp: TranslatePack,
) -> (Value, Vec<Block>) {
    let TranslatePack(int, variables, module) = tp;
    match expr {
        Expr::Ident(name) => {
            let b = builder.create_block();
            builder.append_block_param(b, int);
            builder.switch_to_block(b);
            let val = *variables.get(&name).expect("Variable not define");
            let ctx_ptr: Value = *builder.block_params(b).get(0).unwrap();
            let vars_ptr = builder.ins().load(int, MemFlags::new(), ctx_ptr, 0);
            let index = builder.ins().iconst(int, val as i64);
            let offset = builder.ins().imul_imm(index, 8);
            let vars_ptr = builder.ins().iadd(vars_ptr, offset);
            let val = builder.ins().load(int, MemFlags::new(), vars_ptr, 0);
            builder.ins().store(MemFlags::new(), val, ctx_ptr, 16);
            builder.ins().return_(&[]);
            (ctx_ptr, vec![b])
        }
        Expr::Call { ident, args } => match *ident {
            Expr::Ident(name) => {
                let b = builder.create_block();
                builder.switch_to_block(b);
                let mut sig = module.make_signature();

                for _arg in &args {
                    sig.params.push(AbiParam::new(int))
                }

                sig.returns.push(AbiParam::new(int));

                let callee = module
                    .declare_function(&name, Linkage::Import, &sig)
                    .expect("Problem declaration function");

                let local_callee = module.declare_func_in_func(callee, builder.func);

                let mut arg_values = vec![];

                for arg in args {
                    let (v, b) =
                        translate_expr(arg, builder, TranslatePack(int, variables, module));
                    arg_values.push(v)
                }

                let call = builder.ins().call(local_callee, &arg_values);
                (*builder.inst_results(call).get(0).unwrap(), vec![b])
            }
            _ => todo!(),
        },
        Expr::Lit(lit) => {
            let b = builder.create_block();
            builder.append_block_param(b, int);
            builder.switch_to_block(b);

            let ctx_ptr: Value = *builder.block_params(b).get(0).unwrap();

            let imm: i64 = lit.parse().unwrap();
            let imm_val = builder.ins().iconst(int, imm);
            builder.ins().store(MemFlags::new(), imm_val, ctx_ptr, 16);
            builder.ins().return_(&[]);
            (ctx_ptr, vec![b])
        }
        Expr::Function {
            name,
            function_ty,
            body,
        } => todo!(),
        Expr::FunctionType { params, ret_ty } => todo!(),
        Expr::Assign((name, _), expr) => match *name {
            Expr::Ident(name) => {
                let (val, block) =
                    translate_expr(*expr, builder, TranslatePack(int, variables, module));
                let b = builder.create_block();
                builder.append_block_param(b, int);
                builder.switch_to_block(b);

                let ctx_ptr: Value = *builder.block_params(b).get(0).unwrap();

                let val = builder.ins().load(int, MemFlags::new(), ctx_ptr, 16);
                let vars_ptr_len = builder.ins().load(int, MemFlags::new(), ctx_ptr, 8);
                let new_vars_len = builder.ins().iadd_imm(vars_ptr_len, 1);
                builder
                    .ins()
                    .store(MemFlags::new(), new_vars_len, ctx_ptr, 8);
                let len_var = builder.declare_var(int);
                builder.def_var(len_var, new_vars_len);

                let buf_size = builder.ins().imul_imm(new_vars_len, 8);
                let b_2 = builder.create_block();
                builder.append_block_param(b_2, int);
                builder.append_block_param(b_2, int);
                call_malloc(
                    module,
                    builder,
                    buf_size,
                    b_2,
                    &[BlockArg::Value(ctx_ptr), BlockArg::Value(val)],
                );

                let ctx_ptr: Value = *builder.block_params(b).get(0).unwrap();
                let val: Value = *builder.block_params(b).get(0).unwrap();
                let new_vars_ptr: Value = *builder.block_params(b).get(0).unwrap();

                let ctx_var = builder.declare_var(int);
                let val_var = builder.declare_var(int);
                let new_vars_ptr_var = builder.declare_var(int);
                builder.def_var(ctx_var, ctx_ptr);
                builder.def_var(val_var, val);
                builder.def_var(new_vars_ptr_var, new_vars_ptr);

                let entry_block = builder.create_block();
                let counter_block = builder.create_block();
                let condition_block = builder.create_block();
                let action_block = builder.create_block();
                let exit_block = builder.create_block();

                builder.ins().jump(entry_block, &[]);

                builder.switch_to_block(entry_block);
                builder.seal_block(entry_block);

                let counter = builder.declare_var(int);
                let zero = builder.ins().iconst(int, 0);
                builder.def_var(counter, zero);
                builder.ins().jump(condition_block, &[]);

                builder.switch_to_block(condition_block);

                let vars_len: Value = builder.use_var(len_var);
                let current_counter = builder.use_var(counter);
                let cond_val = builder
                    .ins()
                    .icmp(IntCC::SignedLessThan, current_counter, vars_len);
                builder
                    .ins()
                    .brif(cond_val, action_block, &[], exit_block, &[]);

                builder.switch_to_block(action_block);
                builder.seal_block(action_block);

                let ctx_ptr = builder.use_var(ctx_var);
                let new_vars_ptr: Value = builder.use_var(new_vars_ptr_var);
                let old_vars_ptr = builder.ins().load(int, MemFlags::new(), ctx_ptr, 0);

                let current_counter = builder.use_var(counter);
                let offset = builder.ins().imul_imm(current_counter, 8);
                let old_vars_ptr = builder.ins().iadd(old_vars_ptr, offset);
                let new_vars_ptr = builder.ins().iadd(new_vars_ptr, offset);
                let val = builder.ins().load(int, MemFlags::new(), old_vars_ptr, 0);
                builder.ins().store(MemFlags::new(), val, new_vars_ptr, 0);
                builder.ins().jump(counter_block, &[]);

                builder.switch_to_block(counter_block);
                builder.seal_block(counter_block);

                let current_counter = builder.use_var(counter);
                let new_counter = builder.ins().iadd_imm(current_counter, 1);
                builder.def_var(counter, new_counter);
                builder.ins().jump(condition_block, &[]);
                builder.seal_block(condition_block);

                builder.switch_to_block(exit_block);
                builder.seal_block(exit_block);

                let new_vars_ptr = builder.use_var(new_vars_ptr_var);
                let ctx_ptr = builder.use_var(ctx_var);

                builder
                    .ins()
                    .store(MemFlags::new(), new_vars_ptr, ctx_ptr, 0);

                builder.ins().return_(&[]);

                variables.insert(name, vars_len.as_u32() as usize);

                (ctx_ptr, [vec![b], block].concat())
            }
            _ => todo!(),
        },
        Expr::GlobalDataAddr(expr) => todo!(),
    }
}
fn declare_variables(
    int: types::Type,
    builder: &mut FunctionBuilder,
    params: &[String],
    the_return: &str,
    stmt: &[Expr],
    entry_block: Block,
) -> HashMap<String, Variable> {
    let mut vars = HashMap::new();

    for (i, name) in params.iter().enumerate() {
        let val = *builder.block_params(entry_block).get(i).as_deref().unwrap();
        let var = builder.declare_var(int);
        vars.insert(name.into(), var);
        builder.def_var(var, val);
    }

    let zero = builder.ins().iconst(int, 0);
    let return_var = {
        let var = builder.declare_var(int);
        vars.insert(the_return.into(), var);
        var
    };
    builder.def_var(return_var, zero);

    vars
}
