use crate::{
    frontend::parser::{self, ast::expr::Expr},
    general_compiler::GeneralCompiler,
};
use anyhow::*;
use cranelift::{
    codegen::Context,
    module::{DataDescription, Linkage, Module, default_libcall_names},
    native,
    object::{ObjectBuilder, ObjectModule},
    prelude::{Configurable, FunctionBuilderContext, settings},
};
use std::{
    fs::write,
    path::{Path, PathBuf},
};

pub struct Aot {
    builder_ctx: FunctionBuilderContext,
    ctx: Context,
    data_description: DataDescription,
    module: ObjectModule,
}

impl Default for Aot {
    fn default() -> Self {
        let mut flag_builder = settings::builder();
        flag_builder.set("use_colocated_libcalls", "false").unwrap();
        flag_builder.set("is_pic", "false").unwrap();
        let isa_builder =
            native::builder().unwrap_or_else(|msg| panic!("Host machine not supported: {msg}"));
        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .unwrap();
        let builder = ObjectBuilder::new(isa, "test", default_libcall_names()).unwrap();
        let module = ObjectModule::new(builder);
        Self {
            builder_ctx: FunctionBuilderContext::new(),
            ctx: module.make_context(),
            data_description: DataDescription::new(),
            module,
        }
    }
}

impl Aot {
    pub fn compile<P: AsRef<Path>>(self, input: &str, path: P) -> Result<()> {
        let exprs = parser::exprs(input)?;
        let aot = self.translate(exprs)?;

        let obj = aot.module.finish();
        let obj_bytes = obj.emit()?;

        write(path.as_ref().join("obj.o"), obj_bytes)?;
        Ok(())
    }
}

impl GeneralCompiler<ObjectModule> for Aot {
    fn from_general_compiler(
        builder_ctx: FunctionBuilderContext,
        ctx: Context,
        data_description: DataDescription,
        module: ObjectModule,
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

    fn unwrap(
        self,
    ) -> (
        FunctionBuilderContext,
        Context,
        DataDescription,
        ObjectModule,
    ) {
        (
            self.builder_ctx,
            self.ctx,
            self.data_description,
            self.module,
        )
    }
}
