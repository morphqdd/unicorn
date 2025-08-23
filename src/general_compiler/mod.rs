use crate::frontend::parser::ast::expr::Expr;
use anyhow::*;
use cranelift::module::{FuncOrDataId, Linkage};
use cranelift::{
    codegen::Context,
    module::{DataDescription, Module},
    prelude::{
        AbiParam, Block, FunctionBuilder, FunctionBuilderContext, InstBuilder, Value, Variable,
        types,
    },
};
use std::{collections::HashMap, ops::DerefMut};
use cranelift::codegen::ir::BlockArg;
use cranelift::prelude::{Imm64, IntCC, MemFlags};
use crate::general_compiler::runtime::init_runtime;
use crate::general_compiler::type_def::{Field, TypeDef};

mod type_def;
mod function_translator;
mod runtime;
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

        let entry_block = builder.create_block();
        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);

        let after_build_runtime_block = builder.create_block();
        let runtime = init_runtime(&mut module, &mut builder, after_build_runtime_block);
        builder.switch_to_block(after_build_runtime_block);
        builder.seal_block(after_build_runtime_block);

        let id = module
            .declare_function("main", Linkage::Export, &mut builder.func.signature)?;
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

pub fn call_malloc(module: &mut dyn Module, builder: &mut FunctionBuilder, buffer_size: Value) -> Value {
    let ty = module.target_config().pointer_type();
    let mut malloc_sig = module.make_signature();
    malloc_sig.params.push(AbiParam::new(ty));
    malloc_sig.returns.push(AbiParam::new(ty));

    let callee_malloc = module
        .declare_function("malloc", Linkage::Import, &malloc_sig)
        .unwrap();
    let local_callee_malloc= module
        .declare_func_in_func(callee_malloc, builder.func);

    let call = builder.ins().call(local_callee_malloc, &[buffer_size]);
    *builder.inst_results(call).get(0).unwrap()
}