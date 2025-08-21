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
use cranelift::codegen::ir::Fact::Mem;
use cranelift::prelude::{IntCC, MemFlags};
use crate::general_compiler::type_def::{Field, TypeDef};

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

        let ty_vec = TypeDef::new("Vec", vec![
            Field::new("ptr", module.target_config().pointer_type()),
            Field::new("len", module.target_config().pointer_type()),
            Field::new("cap", module.target_config().pointer_type())
        ]);

        //let mut funcs = vec![];

        // for expr in exprs {
        //     match expr {
        //         Expr::Function {
        //             name,
        //             function_ty,
        //             body,
        //         } => match *function_ty {
        //             Expr::FunctionType { params, ret_ty } => {
        //                 // Set support type
        //                 let int = module.target_config().pointer_type();
        //
        //                 // Set params of function
        //                 for _p in params.iter() {
        //                     ctx.func.signature.params.push(AbiParam::new(int));
        //                 }
        //
        //                 // Set function return type
        //                 ctx.func.signature.returns.push(AbiParam::new(int));
        //
        //                 let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);
        //
        //                 // Create block for entry to function
        //                 let entry_block = builder.create_block();
        //                 // Since this is the entry block, add block parameters corresponding to
        //                 // the function's parameters.
        //                 builder.append_block_params_for_function_params(entry_block);
        //
        //                 // Tell the builder to emit code in this block.
        //                 builder.switch_to_block(entry_block);
        //
        //                 // And, tell the builder that this block will have no further
        //                 // predecessors. Since it's the entry block, it won't have any
        //                 // predecessors.
        //                 builder.seal_block(entry_block);
        //                 let Expr::Ident(ret_ty) = *ret_ty else {
        //                     panic!()
        //                 };
        //                 let vars = declare_variables(
        //                     int,
        //                     &mut builder,
        //                     &params
        //                         .iter()
        //                         .map(|(expr, _)| match expr {
        //                             Expr::Ident(name) => name.to_string(),
        //                             _ => panic!("Not ident"),
        //                         })
        //                         .collect::<Vec<_>>(),
        //                     &ret_ty,
        //                     &body,
        //                     entry_block,
        //                 );
        //
        //                 let mut trans = FunctionTranslator {
        //                     int,
        //                     builder,
        //                     variables: vars,
        //                     module: &mut module,
        //                 };
        //
        //                 for i in 0..body.len() - 1 {
        //                     trans.translate_expr(body[i].clone());
        //                 }
        //
        //                 let val = trans.translate_expr(body[body.len() - 1].clone());
        //
        //                 trans.builder.ins().return_(&[val]);
        //
        //                 trans.builder.finalize();
        //
        //                 let Expr::Ident(name) = *name else {
        //                     panic!("Not a name!")
        //                 };
        //
        //                 let id = module
        //                     .declare_function(
        //                         &format!("_{name}_"),
        //                         Linkage::Export,
        //                         &ctx.func.signature
        //                     )?;
        //
        //                 funcs.push(format!("_{name}_"));
        //
        //                 module.define_function(id, &mut ctx)?;
        //
        //                 module.clear_context(&mut ctx);
        //             }
        //             _ => panic!("Translation for this function type is not support yet"),
        //         },
        //         _ => panic!("Translation not support yet"),
        //     }
        // }
        let int= module.target_config().pointer_type();
        ctx.func.signature.returns.push(AbiParam::new(int));

        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);

        let entry_block = builder.create_block();
        builder.append_block_params_for_function_params(entry_block);
        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);

        let size_of_element = 8;
        let cap = 16;
        let buffer_size= builder.ins().iconst(int, size_of_element * cap);

        let mut malloc_sig = module.make_signature();
        malloc_sig.params.push(AbiParam::new(int));
        malloc_sig.returns.push(AbiParam::new(int));

        let callee_malloc = module
            .declare_function("malloc", Linkage::Import, &malloc_sig)
            .unwrap();
        let local_callee_malloc= module
            .declare_func_in_func(callee_malloc, builder.func);

        let call = builder.ins().call(local_callee_malloc, &[buffer_size]);
        let ptr = *builder.inst_results(call).get(0).unwrap();
        let zero = builder.ins().iconst(int, 0);
        builder.ins().store(MemFlags::new(), zero, ptr, 0);
        builder.ins().store(MemFlags::new(), zero, ptr, 8);


        let reductions_limit = builder.declare_var(int);
        let reductions_variable = builder.declare_var(int);
        let limit_of_reductions = builder.ins().iconst(int, REDUCTIONS_LIMIT);
        let default_reductions = builder.ins().iconst(int, 0);
        builder.def_var(reductions_limit, limit_of_reductions);
        builder.def_var(reductions_variable, default_reductions);

        let cond_block = builder.create_block();
        builder.append_block_param(cond_block, int);
        builder.ins().jump(cond_block, &[BlockArg::Value(ptr)]);
        builder.switch_to_block(cond_block);

        let limit_of_reductions = builder.use_var(reductions_limit);
        let current_reductions = builder.use_var(reductions_variable);
        let condition_value = builder.ins().icmp(IntCC::Equal, current_reductions, limit_of_reductions);

        let then_block = builder.create_block();
        let else_block = builder.create_block();
        let merge_block = builder.create_block();

        builder.append_block_param(then_block, int);
        builder.append_block_param(else_block, int);
        builder.append_block_param(merge_block, int);
        builder.append_block_param(merge_block, int);

        let ptr = *builder.block_params(cond_block).get(0).unwrap();

        builder
            .ins()
            .brif(
                condition_value,
                then_block, &[BlockArg::Value(ptr)],
                else_block, &[BlockArg::Value(ptr)]
            );

        builder.switch_to_block(then_block);
        builder.seal_block(then_block);

        builder.def_var(reductions_variable, default_reductions);

        let ptr = *builder.block_params(then_block).get(0).unwrap();
        let tmp_val = builder.ins().load(int, MemFlags::new(), ptr, 0);
        let the_one = builder.ins().iconst(int, 1);
        let new_val = builder.ins().iadd(tmp_val, the_one);
        builder.ins().store(MemFlags::new(), new_val, ptr, 0);
        builder.ins().jump(merge_block, &[BlockArg::Value(ptr), BlockArg::Value(the_one)]);

        builder.switch_to_block(else_block);
        builder.seal_block(else_block);
        let the_one = builder.ins().iconst(int, 1);
        let the_two = builder.ins().iconst(int, 2);
        let ptr = *builder.block_params(else_block).get(0).unwrap();
        let tmp_val = builder.ins().load(int, MemFlags::new(), ptr, 0);
        let tmp_val_2 = builder.ins().load(int, MemFlags::new(), ptr, 8);
        let new_val = builder.ins().iadd(tmp_val, tmp_val_2);
        builder.ins().store(MemFlags::new(), new_val, ptr, 8);
        let current_reductions = builder.use_var(reductions_variable);
        let the_new_val = builder.ins().iadd(current_reductions,the_one);
        builder.def_var(reductions_variable, the_new_val);
        builder.ins().jump(merge_block, &[BlockArg::Value(ptr), BlockArg::Value(the_two)]);


        builder.switch_to_block(merge_block);
        builder.seal_block(merge_block);

        let mut sig = module.make_signature();

        sig.params.push(AbiParam::new(int));
        sig.returns.push(AbiParam::new(int));

        let callee = module
            .declare_function("stdprint", Linkage::Import, &sig)?;

        let local_callee = module.declare_func_in_func(callee, builder.func);

        let ptr = *builder.block_params(merge_block).get(0).unwrap();
        let branch = *builder.block_params(merge_block).get(1).unwrap();
        let first = builder.ins().load(int, MemFlags::new(), ptr, 0);
        let second = builder.ins().load(int, MemFlags::new(), ptr, 8);
        let call = builder.ins().call(local_callee, &[first]);
        let call = builder.ins().call(local_callee, &[second]);
        let _ = builder.ins().call(local_callee, &[branch]);

        builder.ins().jump(cond_block, &[BlockArg::Value(ptr)]);
        builder.seal_block(cond_block);

        builder.finalize();

        let id =
            module.declare_function("main", Linkage::Export, &ctx.func.signature)?;

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
