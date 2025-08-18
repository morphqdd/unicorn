use std::{collections::HashMap, panic};
use cranelift::{codegen::Context, jit::{JITBuilder, JITModule}, module::{default_libcall_names, DataDescription, Linkage, Module}, native, prelude::{settings, types, AbiParam, Block, Configurable, EntityRef, FunctionBuilder, FunctionBuilderContext, InstBuilder, Value, Variable}};
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
        self.translate(function.clone())?;
        
        let Expr::Function { name, function_ty, body } = function else { panic!("Not a funtion") };
        let Expr::Ident(name) = *name else { panic!("Not a name!") };

        let id = self
            .module
            .declare_function(&name, Linkage::Export, &self.ctx.func.signature)?;

        self.module
            .define_function(id, &mut self.ctx)?;

        self.module.clear_context(&mut self.ctx);

        self.module.finalize_definitions().unwrap();

        let code = self.module.get_finalized_function(id);

        Ok(code)
    }

    fn translate(&mut self, expr: Expr) -> Result<()> {
        match expr {
            Expr::Function { name, function_ty, body } => match *function_ty {
                Expr::FunctionType { params, ret_ty } => {
                    // Set support type
                    let int = self.module.target_config().pointer_type();
                    
                    // Set params of function
                    for _p in params.iter() {
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
                    let Expr::Ident(ret_ty) = *ret_ty else { panic!() };
                    let vars = declare_variables(
                        int, 
                        &mut builder, 
                        &params.iter()
                            .map(|(expr, _)| match expr {
                                Expr::Ident(name) => name.to_string(),
                                _ => panic!("Not ident")
                            }).collect::<Vec<_>>(), 
                        &ret_ty, 
                        &body, 
                        entry_block
                    );

                    let mut trans = FunctionTranslator {
                        int,
                        builder,
                        variables: vars,
                        module: &mut self.module
                    };

                    for i in 0..body.len()-1 {
                        trans.translate_expr(body[i].clone());
                    }

                    let val = trans.translate_expr(body[body.len()-1].clone());

                    trans.builder.ins().return_(&[val]);

                    trans.builder.finalize();
                    Ok(())
                }
                _ => panic!("Translation for this function type is not support yet")
            }
            _ => panic!("Translation not support yet") 
        }
    }
}

struct FunctionTranslator<'a> {
    int: types::Type,
    builder: FunctionBuilder<'a>,
    variables: HashMap<String, Variable>,
    module: &'a mut JITModule
}

impl<'a> FunctionTranslator<'a> {
    pub fn translate_expr(&mut self, expr: Expr) -> Value {
        match expr {
            Expr::Ident(name) => {
                let var = self.variables.get(&name).expect("Variable not define");
                self.builder.use_var(*var)
            },
            Expr::Call { ident, args } => todo!(),
            Expr::Lit(lit) => {
                let imm: i32 = lit.parse().unwrap();
                self.builder.ins().iconst(self.int, i64::from(imm))
            },
            Expr::Function { name, function_ty, body } => todo!(),
            Expr::FunctionType { params, ret_ty } => todo!(),
            Expr::Assign(_, expr) => todo!(),
            Expr::GlobalDataAddr(expr) => todo!(),
        }    
    } 
}

fn declare_variables(
    int: types::Type,
    builder: &mut FunctionBuilder,
    params: &[String],
    the_return: &str,
    stmt: &[Expr],
    entry_block: Block
) -> HashMap<String, Variable> {
    let mut vars = HashMap::new();

    for (i, name) in params.iter().enumerate() {
        let val = builder.block_params(entry_block)[i];
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

fn declare_variable(
    int: types::Type,
    builder: &mut FunctionBuilder,
    variables: &mut HashMap<String, Variable>,
    index: &mut usize,
    name: &str
) -> Variable {
    let var = Variable::new(*index);
    if !variables.contains_key(name) {
        variables.insert(name.into(), var);
        builder.declare_var(int);
        *index += 1;
    }
    var
}
