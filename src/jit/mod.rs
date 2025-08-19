use std::{collections::HashMap, fs::File, panic};
use cranelift::{codegen::Context, jit::{JITBuilder, JITModule}, module::{default_libcall_names, DataDescription, FuncId, Linkage, Module}, native, prelude::{settings, types, AbiParam, Block, Configurable, EntityRef, FunctionBuilder, FunctionBuilderContext, InstBuilder, Value, Variable}};
use anyhow::*;

use crate::{frontend::parser::{self, ast::expr::Expr}, general_compiler::GeneralCompiler};


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
    pub fn compile(self, input: &str) -> Result<*const u8> {
        let function = parser::function(input)?;
        let mut jit = self.translate(function.clone())?;

        let Expr::Function { name, function_ty, body } = function else { panic!("Not a funtion") };
        let Expr::Ident(name) = *name else { panic!("Not a name!") };

        let id = jit
            .module
            .declare_function(&name, Linkage::Export, &jit.ctx.func.signature)?;

        jit.module
            .define_function(id, &mut jit.ctx)?;

        jit.module.clear_context(&mut jit.ctx);

        jit.module.finalize_definitions().unwrap();
        
        let code = jit.module.get_finalized_function(id);
        Ok(code)
    }

}

impl GeneralCompiler<JITModule> for Jit {

    fn unwrap(self) -> (FunctionBuilderContext, Context, DataDescription, JITModule) {
        (self.builder_ctx, self.ctx, self.data_description, self.module)
    }

    fn from_general_compiler(
        builder_ctx: FunctionBuilderContext,
        ctx: Context,
        data_description: DataDescription,
        module: JITModule
    ) -> Self where Self: Sized {
        Self {
            builder_ctx,
            ctx,
            data_description,
            module
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
