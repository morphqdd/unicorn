use anyhow::*;
use cranelift::{
    codegen::Context,
    jit::{JITBuilder, JITModule},
    module::{DataDescription, FuncId, Linkage, Module, default_libcall_names},
    native,
    prelude::{
        AbiParam, Block, Configurable, EntityRef, FunctionBuilder, FunctionBuilderContext,
        InstBuilder, Value, Variable, settings, types,
    },
};
use std::{collections::HashMap, fs::File, panic};
use cranelift::module::FuncOrDataId;
use crate::{
    frontend::parser::{self, ast::expr::Expr},
    general_compiler::GeneralCompiler,
};

pub struct Jit {
    builder_ctx: FunctionBuilderContext,
    ctx: Context,
    data_description: DataDescription,
    module: JITModule,
}

impl Default for Jit {
    fn default() -> Self {
        let mut flag_builder = settings::builder();
        flag_builder.set("use_colocated_libcalls", "false").unwrap();
        flag_builder.set("is_pic", "false").unwrap();
        let isa_builder =
            native::builder().unwrap_or_else(|msg| panic!("Host machine not supported: {msg}"));
        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .unwrap();
        let builder = JITBuilder::with_isa(isa, default_libcall_names());
        let module = JITModule::new(builder);
        Self {
            builder_ctx: FunctionBuilderContext::new(),
            ctx: module.make_context(),
            data_description: DataDescription::new(),
            module,
        }
    }
}

impl Jit {
    pub fn compile(self, input: &str) -> Result<*const u8> {
        let exprs = parser::exprs(input)?;
        let mut jit: Jit = self.translate(exprs)?;

        jit.module.finalize_definitions().unwrap();

        let FuncOrDataId::Func(id) =  jit.module.get_name("main")
            .expect("Main not found")
            else { panic!("Not a func") };

        let code = jit.module.get_finalized_function(id);
        Ok(code)
    }
}

impl GeneralCompiler<JITModule> for Jit {
    fn from_general_compiler(
        builder_ctx: FunctionBuilderContext,
        ctx: Context,
        data_description: DataDescription,
        module: JITModule,
    ) -> Self
    where
        Self: Sized,
    {
        Self {
            builder_ctx,
            ctx,
            data_description,
            module,
        }
    }

    fn unwrap(self) -> (FunctionBuilderContext, Context, DataDescription, JITModule) {
        (
            self.builder_ctx,
            self.ctx,
            self.data_description,
            self.module,
        )
    }
}
