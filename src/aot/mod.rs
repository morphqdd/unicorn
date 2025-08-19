use std::{fs::write, path::{Path, PathBuf}};
use anyhow::*;
use cranelift::{codegen::Context, module::{default_libcall_names, DataDescription, Linkage, Module}, native, object::{ObjectBuilder, ObjectModule}, prelude::{settings, Configurable, FunctionBuilderContext}};

use crate::{frontend::parser::{self, ast::expr::Expr}, general_compiler::GeneralCompiler};

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
        let isa_builder = native::builder().unwrap_or_else(|msg| {
            panic!("Host machine not supported: {msg}")
        });
        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .unwrap();
        let builder = ObjectBuilder::new(isa, "test",default_libcall_names()).unwrap();
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
        let function = parser::function(input)?;
        let mut aot = self.translate(function.clone())?;

        let Expr::Function { name, function_ty, body } = function else { panic!("Not a funtion") };
        let Expr::Ident(name) = *name else { panic!("Not a name!") };

        let id = aot
            .module
            .declare_function(&name, Linkage::Export, &aot.ctx.func.signature)?;

        aot.module
            .define_function(id, &mut aot.ctx)?;

        aot.module.clear_context(&mut aot.ctx);

        let obj = aot.module.finish();
        let obj_bytes = obj.emit()?;

        write(path.as_ref().join("obj.o"), obj_bytes)?;
        Ok(())
    }

}

impl GeneralCompiler<ObjectModule> for Aot {

    fn unwrap(self) -> (FunctionBuilderContext, Context, DataDescription, ObjectModule) {
        (self.builder_ctx, self.ctx, self.data_description, self.module)
    }

    fn from_general_compiler(
        builder_ctx: FunctionBuilderContext,
        ctx: Context,
        data_description: DataDescription,
        module: ObjectModule
    ) -> Self where Self: Sized {
        Self {
            builder_ctx,
            ctx,
            data_description,
            module
        }
    }
}
