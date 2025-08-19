use std::{collections::HashMap, ops::DerefMut};
use anyhow::*;
use cranelift::{codegen::Context, module::{DataDescription, Module}, prelude::{types, AbiParam, Block, FunctionBuilder, FunctionBuilderContext, InstBuilder, Value, Variable}};

use crate::frontend::parser::ast::expr::Expr;

pub trait GeneralCompiler<T: Module> {
    fn from_general_compiler(
        builder_ctx: FunctionBuilderContext,
        ctx: Context,
        data_description: DataDescription,
        module: T
    ) -> Self where Self: Sized;
    fn unwrap(self) -> (FunctionBuilderContext, Context, DataDescription, T);
    fn translate(self, expr: Expr) -> Result<Self> where Self: Sized {
        match expr {
            Expr::Function { name, function_ty, body } => match *function_ty {
                Expr::FunctionType { params, ret_ty } => {
                    let (mut builder_ctx, mut ctx, data_description, mut module) = self.unwrap();
                    // Set support type
                    let int = module.target_config().pointer_type();
                    
                    // Set params of function
                    for _p in params.iter() {
                        ctx.func.signature.params.push(AbiParam::new(int));
                    }
                    
                    // Set function return type
                    ctx.func.signature.returns.push(AbiParam::new(int));
                    

                    let mut builder = FunctionBuilder::new(
                        &mut ctx.func, 
                        &mut builder_ctx
                    );


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
                        module: &mut module                  
                    };

                    for i in 0..body.len()-1 {
                        trans.translate_expr(body[i].clone());
                    }

                    let val = trans.translate_expr(body[body.len()-1].clone());

                    trans.builder.ins().return_(&[val]);

                    trans.builder.finalize();
                    Ok(Self::from_general_compiler(builder_ctx, ctx, data_description, module))
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
    module: &'a mut dyn Module
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
            Expr::Assign((name, _), expr) => {
                match *name {
                    Expr::Ident(name) => {
                        let val = self.translate_expr(*expr);
                        let var = self.builder.declare_var(self.int);
                        self.builder.def_var(var, val);
                        self.variables.insert(name, var);
                        val
                    }
                    _ => todo!()
                }
            },
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
