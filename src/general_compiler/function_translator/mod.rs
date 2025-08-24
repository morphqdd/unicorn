use crate::frontend::parser::ast::expr::Expr;
use cranelift::frontend::{FunctionBuilder, Variable};
use cranelift::module::{Linkage, Module};
use cranelift::prelude::{AbiParam, Block, InstBuilder, Value, types};
use std::collections::HashMap;

struct FunctionTranslator<'a> {
    int: types::Type,
    builder: FunctionBuilder<'a>,
    variables: HashMap<String, Variable>,
    module: &'a mut dyn Module,
}

impl<'a> FunctionTranslator<'a> {
    pub fn translate_expr(&mut self, expr: Expr) -> Value {
        match expr {
            Expr::Ident(name) => {
                let var = self.variables.get(&name).expect("Variable not define");
                self.builder.use_var(*var)
            }
            Expr::Call { ident, args } => match *ident {
                Expr::Ident(name) => {
                    let mut sig = self.module.make_signature();

                    for _arg in &args {
                        sig.params.push(AbiParam::new(self.int))
                    }

                    sig.returns.push(AbiParam::new(self.int));

                    let callee = self
                        .module
                        .declare_function(&name, Linkage::Import, &sig)
                        .expect("Problem declaration function");

                    let local_callee = self.module.declare_func_in_func(callee, self.builder.func);

                    let mut arg_values = vec![];

                    for arg in args {
                        arg_values.push(self.translate_expr(arg))
                    }

                    let call = self.builder.ins().call(local_callee, &arg_values);
                    *self.builder.inst_results(call).get(0).unwrap()
                }
                _ => todo!(),
            },
            Expr::Lit(lit) => {
                let imm: i32 = lit.parse().unwrap();
                self.builder.ins().iconst(self.int, i64::from(imm))
            }
            Expr::Function {
                name,
                function_ty,
                body,
            } => todo!(),
            Expr::FunctionType { params, ret_ty } => todo!(),
            Expr::Assign((name, _), expr) => match *name {
                Expr::Ident(name) => {
                    let val = self.translate_expr(*expr);
                    let var = self.builder.declare_var(self.int);
                    self.builder.def_var(var, val);
                    self.variables.insert(name, var);
                    val
                }
                _ => todo!(),
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
