use cranelift::{codegen::Context, jit::{JITBuilder, JITModule}, module::{default_libcall_names, DataDescription, Module}, native, prelude::{settings, AbiParam, Configurable, FunctionBuilder, FunctionBuilderContext}};
use anyhow::*;

use crate::frontend::parser::{self, ast::expr::Expr};


pub struct Jit {
    builder_ctx: FunctionBuilderContext,
    ctx: Context,
    data_description: DataDescription,
    module: JITModule
}

impl Default for Jit {
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
        let builder = JITBuilder::with_isa(isa, default_libcall_names());
        let module = JITModule::new(builder);
        Self { 
            builder_ctx: FunctionBuilderContext::new(), 
            ctx: module.make_context(), 
            data_description: DataDescription::new(),
            module
        }
    }
}

impl Jit {
    pub fn compile(&mut self, input: &str) -> Result<*const u8> {
        let function = parser::function(input)?;
        self.translate(function)?;
        todo!()
    }

    fn translate(&mut self, expr: Expr) -> Result<()> {
        match expr {
            Expr::Function { name, function_ty, body } => match *function_ty {
                Expr::FunctionType { params, ret_ty } => {
                    // Set support type
                    let int = self.module.target_config().pointer_type();
                    
                    // Set params of function
                    for _p in params {
                        self.ctx.func.signature.params.push(AbiParam::new(int));
                    }
                    
                    // Set function return type
                    self.ctx.func.signature.returns.push(AbiParam::new(int));
                    
                    let mut builder = FunctionBuilder::new(&mut self.ctx.func, &mut self.builder_ctx);


                    // Create block for entry to function
                    let entry_block = builder.create_block();

                    // Since this is the entry block, add block parameters corresponding to
                    // the function's parameters.
                    builder.append_block_params_for_function_params(entry_block);
                    
                    // Tell the builder to emit code in this block.
                    builder.switch_to_block(entry_block);

                    // And, tell the builder that this block will have no further
                    // predecessors. Since it's the entry block, it won't have any
                    // predecessors.
                    builder.seal_block(entry_block);
                }
                _ => panic!("Translation for this function type is not support yet")
            }
            _ => panic!("Translation not support yet") 
        }


        todo!()
    }
}
