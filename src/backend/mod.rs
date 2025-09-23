use anyhow::{Result, bail};
use base64ct::{Base64, Encoding};
use cranelift::{
    codegen::{Context, ir::BlockArg},
    frontend::Switch,
    module::{FuncId, Linkage, Module, default_libcall_names},
    native::{self, builder},
    object::{ObjectBuilder, ObjectModule},
    prelude::{
        AbiParam, Block, Configurable, EntityRef, FunctionBuilder, FunctionBuilderContext,
        InstBuilder, IntCC, MemFlags, TrapCode, Value, Variable,
        settings::{self},
    },
};
use std::{cell::RefCell, collections::HashMap, fs::write, path::Path, rc::Rc, vec};
use whirlpool::{Digest, Whirlpool};

use crate::{
    frontend::parser::{ast::expr::Expr, parser},
    general_compiler::{call_free, call_malloc},
    middleware::{Expression, Expressions},
};

const PROCESS_CTX_BUFFER_SIZE: i64 = 48;
const PROCESS_CTX_VARS: i32 = 0;
const PROCESS_CTX_VARS_LEN: i32 = 8;
const PROCESS_CTX_FUNC_ADDR: i32 = 16;
const PROCESS_CTX_TEMP_VAL: i32 = 24;
const PROCESS_CTX_DEPENDENCIES: i32 = 32;
const PROCESS_CTX_CALL_ARGS_TEMP: i32 = 40;

const RUNTIME_BUFFER_SIZE: i64 = 40;

thread_local! {
    pub static FUNCTIONS: Rc<RefCell<HashMap<String, (FuncId, usize, usize)>>>
    = Rc::new(RefCell::new(HashMap::new()));
}

#[derive(Default, Clone, Copy)]
enum TranslationType {
    #[default]
    Default,
    Call(usize),
}

#[derive(Default)]
struct TranslationContext {
    variables: HashMap<String, usize>,
    var_counter: usize,
    block_counter: usize,
    tr_type: TranslationType,
}

fn create_process(
    module: &mut ObjectModule,
    builder: &mut FunctionBuilder,
    func_name: &str,
) -> Value {
    let target_type = module.target_config().pointer_type();
    let buff = builder.ins().iconst(target_type, PROCESS_CTX_BUFFER_SIZE);
    let after_call = builder.create_block();
    builder.append_block_param(after_call, target_type);
    call_malloc(module, builder, buff, after_call, &[]);
    builder.switch_to_block(after_call);
    builder.seal_block(after_call);
    let ctx_ptr = *builder.block_params(after_call).first().unwrap();

    let after_call = builder.create_block();
    builder.append_block_param(after_call, target_type);
    let buff = builder.ins().iconst(target_type, 0);
    call_malloc(module, builder, buff, after_call, &[]);
    builder.switch_to_block(after_call);
    builder.seal_block(after_call);

    let vars_ptr = *builder.block_params(after_call).first().unwrap();
    let len = builder.ins().iconst(target_type, 0);
    let zero = builder.ins().iconst(target_type, 0);

    builder
        .ins()
        .store(MemFlags::new(), vars_ptr, ctx_ptr, PROCESS_CTX_VARS);
    builder
        .ins()
        .store(MemFlags::new(), len, ctx_ptr, PROCESS_CTX_VARS_LEN);
    builder
        .ins()
        .store(MemFlags::new(), zero, ctx_ptr, PROCESS_CTX_TEMP_VAL);

    let mut wp = Whirlpool::new();
    Digest::update(&mut wp, func_name);
    let ident = Base64::encode_string(&wp.finalize());

    let (func_id, _, _) = FUNCTIONS.with(|map| *map.borrow().get(&ident).unwrap());

    let callee = module.declare_func_in_func(func_id, builder.func);
    let func_addr = builder.ins().func_addr(target_type, callee);

    builder
        .ins()
        .store(MemFlags::new(), func_addr, ctx_ptr, PROCESS_CTX_FUNC_ADDR);

    ctx_ptr
}

pub struct Compiler {
    module: ObjectModule,
}

impl Default for Compiler {
    fn default() -> Self {
        let mut flag_builder = settings::builder();
        flag_builder.set("opt_level", "speed_and_size").unwrap();
        flag_builder.set("use_colocated_libcalls", "false").unwrap();
        flag_builder.set("is_pic", "false").unwrap();
        let isa_builder =
            native::builder().unwrap_or_else(|msg| panic!("Host machine not supported: {msg}"));
        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .unwrap();
        let builder = ObjectBuilder::new(isa, "module", default_libcall_names()).unwrap();
        let module = ObjectModule::new(builder);

        Self { module }
    }
}

impl Compiler {
    pub fn compile<P: AsRef<Path>>(mut self, input: &str, path: P) -> Result<()> {
        let frontend_ast = parser::exprs(input)?;

        let middleware_ast = Expressions::from(frontend_ast);
        println!("{middleware_ast:#?}");

        self.translate(middleware_ast)?;
        let obj = self.module.finish();
        let obj_bytes = obj.emit()?;

        write(path.as_ref().join("obj.o"), obj_bytes)?;
        Ok(())
    }

    fn declare_runtime_funcitons(&mut self) -> Result<()> {
        let target_type = self.module.target_config().pointer_type();
        let mut stdprint_sig = self.module.make_signature();
        stdprint_sig.params.push(AbiParam::new(target_type));
        let callee_stdprint =
            self.module
                .declare_function("stdprint", Linkage::Import, &stdprint_sig)?;
        let mut wp = Whirlpool::new();
        Digest::update(&mut wp, "stdprint");
        let name = Base64::encode_string(&wp.finalize());
        FUNCTIONS.with(|map| {
            map.borrow_mut().insert(
                name,
                (
                    callee_stdprint,
                    stdprint_sig.params.len(),
                    stdprint_sig.returns.len(),
                ),
            )
        });

        let mut add_sig = self.module.make_signature();
        add_sig.params.push(AbiParam::new(target_type));
        add_sig.params.push(AbiParam::new(target_type));
        add_sig.returns.push(AbiParam::new(target_type));
        let callee_add = self
            .module
            .declare_function("add", Linkage::Import, &add_sig)?;
        let mut wp = Whirlpool::new();
        Digest::update(&mut wp, "add");
        let name = Base64::encode_string(&wp.finalize());
        FUNCTIONS.with(|map| {
            map.borrow_mut().insert(
                name,
                (callee_add, add_sig.params.len(), add_sig.returns.len()),
            )
        });

        let mut now_sig = self.module.make_signature();
        now_sig.returns.push(AbiParam::new(target_type));
        let callee_now = self
            .module
            .declare_function("now", Linkage::Import, &now_sig)?;
        let mut wp = Whirlpool::new();
        Digest::update(&mut wp, "now");
        let name = Base64::encode_string(&wp.finalize());
        FUNCTIONS.with(|map| {
            map.borrow_mut().insert(
                name,
                (callee_now, now_sig.params.len(), now_sig.returns.len()),
            )
        });

        let mut elapsed_sig = self.module.make_signature();
        elapsed_sig.params.push(AbiParam::new(target_type));
        elapsed_sig.returns.push(AbiParam::new(target_type));
        let callee_elapsed =
            self.module
                .declare_function("elapsed", Linkage::Import, &elapsed_sig)?;
        let mut wp = Whirlpool::new();
        Digest::update(&mut wp, "elapsed");
        let name = Base64::encode_string(&wp.finalize());
        FUNCTIONS.with(|map| {
            map.borrow_mut().insert(
                name,
                (
                    callee_elapsed,
                    elapsed_sig.params.len(),
                    elapsed_sig.returns.len(),
                ),
            )
        });
        Ok(())
    }

    pub fn translate(&mut self, expressions: Expressions) -> Result<()> {
        let target_type = self.module.target_config().pointer_type();
        let mut builder_ctx = FunctionBuilderContext::new();
        let mut ctx = self.module.make_context();

        self.declare_runtime_funcitons()?;

        for expression in expressions.0 {
            self.translate_function(expression, &mut builder_ctx, &mut ctx)?;
        }

        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);

        let mut sig = self.module.make_signature();
        sig.params.push(AbiParam::new(target_type));
        sig.params.push(AbiParam::new(target_type));
        sig.returns.push(AbiParam::new(target_type));

        let sig_ref = builder.import_signature(sig);

        builder
            .func
            .signature
            .returns
            .push(AbiParam::new(target_type));
        let entry_block = builder.create_block();
        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);

        let runtime_process_array_size = builder.ins().iconst(target_type, RUNTIME_BUFFER_SIZE);
        let runtime_process_array_len = builder.ins().iconst(target_type, 5);
        let runtime_process_len_var = builder.declare_var(target_type);
        builder.def_var(runtime_process_len_var, runtime_process_array_len);

        let after_call = builder.create_block();
        builder.append_block_param(after_call, target_type);
        call_malloc(
            &mut self.module,
            &mut builder,
            runtime_process_array_size,
            after_call,
            &[],
        );
        builder.switch_to_block(after_call);
        builder.seal_block(after_call);

        let runtime_ptr = *builder.block_params(after_call).first().unwrap();
        let runtime_var = builder.declare_var(target_type);
        builder.def_var(runtime_var, runtime_ptr);

        let main_process_ctx = create_process(&mut self.module, &mut builder, "main");

        let while_block_entry = builder.create_block();
        let condition_block = builder.create_block();
        let action_block = builder.create_block();
        let exit_block = builder.create_block();

        let ctx_ptr_var = builder.declare_var(target_type);
        builder.def_var(ctx_ptr_var, main_process_ctx);
        builder.ins().jump(while_block_entry, &[]);

        builder.switch_to_block(while_block_entry);
        builder.seal_block(while_block_entry);

        let next_block_var = builder.declare_var(target_type);
        let zero = builder.ins().iconst(target_type, 0);
        builder.def_var(next_block_var, zero);

        builder.ins().jump(condition_block, &[]);

        builder.switch_to_block(condition_block);
        let next_block = builder.use_var(next_block_var);
        let cond = builder.ins().icmp_imm(IntCC::NotEqual, next_block, -1);
        builder.ins().brif(cond, action_block, &[], exit_block, &[]);

        builder.switch_to_block(action_block);
        builder.seal_block(action_block);

        let ctx_ptr = builder.use_var(ctx_ptr_var);

        let callee =
            builder
                .ins()
                .load(target_type, MemFlags::new(), ctx_ptr, PROCESS_CTX_FUNC_ADDR);

        let next_block = builder.use_var(next_block_var);

        let call = builder
            .ins()
            .call_indirect(sig_ref, callee, &[next_block, ctx_ptr]);

        let next_block = *builder.inst_results(call).first().unwrap();

        builder.def_var(next_block_var, next_block);
        builder.ins().jump(condition_block, &[]);
        builder.seal_block(condition_block);

        builder.switch_to_block(exit_block);
        builder.seal_block(exit_block);

        let ret = builder
            .ins()
            .load(target_type, MemFlags::new(), ctx_ptr, PROCESS_CTX_TEMP_VAL);

        builder.ins().return_(&[ret]);

        let sig = builder.func.signature.clone();
        builder.finalize();

        let id = self
            .module
            .declare_function("main", Linkage::Export, &sig)?;
        self.module.define_function(id, &mut ctx)?;

        println!("{}", ctx.func);

        self.module.clear_context(&mut ctx);
        Ok(())
    }

    pub fn translate_function(
        &mut self,
        expression: Expression,
        builder_ctx: &mut FunctionBuilderContext,
        ctx: &mut Context,
    ) -> Result<FuncId> {
        let target_type = self.module.target_config().pointer_type();
        let mut builder = FunctionBuilder::new(&mut ctx.func, builder_ctx);
        let mut translation_ctx = TranslationContext::default();

        let Expression::Function { name, body, .. } = expression else {
            bail!("Not a function!")
        };

        builder
            .func
            .signature
            .params
            .push(AbiParam::new(target_type));
        builder
            .func
            .signature
            .params
            .push(AbiParam::new(target_type));
        builder
            .func
            .signature
            .params
            .push(AbiParam::new(target_type));

        builder
            .func
            .signature
            .returns
            .push(AbiParam::new(target_type));

        let block0 = builder.create_block();
        let switch_block = builder.create_block();
        let trap_block = builder.create_block();
        builder.append_block_param(block0, target_type);
        builder.append_block_param(block0, target_type);
        builder.append_block_param(block0, target_type);

        builder.switch_to_block(block0);

        let block_index = *builder.block_params(block0).first().unwrap();
        let ctx_ptr = *builder.block_params(block0).get(1).unwrap();
        let runtime_ptr = *builder.block_params(block0).get(2).unwrap();

        let ctx_ptr_var = builder.declare_var(target_type);
        builder.def_var(ctx_ptr_var, ctx_ptr);

        let runtime_var = builder.declare_var(target_type);
        builder.def_var(runtime_var, runtime_ptr);

        builder.ins().jump(switch_block, &[]);

        let mut switch = Switch::new();

        let mut last_block_i = 0;
        for expression in body.0 {
            let (indecies, last_block, blocks) = self.translate_expression(
                expression,
                &mut builder,
                ctx_ptr_var,
                runtime_var,
                &mut translation_ctx,
            )?;

            for (index, block) in indecies.iter().zip(blocks) {
                switch.set_entry(*index as u128, block);
            }

            last_block_i = last_block;
        }

        let final_block = builder.create_block();
        builder.switch_to_block(final_block);
        let neg = builder.ins().iconst(target_type, -1);
        builder.ins().return_(&[neg]);
        switch.set_entry(last_block_i as u128, final_block);

        builder.switch_to_block(switch_block);

        switch.emit(&mut builder, block_index, trap_block);

        builder.switch_to_block(trap_block);
        builder.ins().trap(TrapCode::user(25).unwrap());
        builder.seal_all_blocks();

        let sig = builder.func.signature.clone();

        builder.finalize();

        let mut wp = Whirlpool::default();

        let Expression::Ident(name) = *name else {
            bail!("Not a ident")
        };

        Digest::update(&mut wp, name.as_bytes());
        let encoded_function_name = Base64::encode_string(&wp.finalize());

        let id = self
            .module
            .declare_function(&encoded_function_name, Linkage::Export, &sig)?;
        self.module.define_function(id, ctx)?;

        FUNCTIONS.with(|map| {
            map.borrow_mut().insert(
                encoded_function_name.clone(),
                (id, sig.params.len(), sig.returns.len()),
            );
        });

        println!("{}", ctx.func);
        println!("{:?}", translation_ctx.variables);
        self.module.clear_context(ctx);
        Ok(id)
    }

    fn translate_expression(
        &mut self,
        expression: Expression,
        builder: &mut FunctionBuilder,
        ctx_ptr_var: Variable,
        runtime_var: Variable,
        translation_ctx: &mut TranslationContext,
    ) -> Result<(Vec<usize>, usize, Vec<Block>)> {
        let target_type = self.module.target_config().pointer_type();
        match expression {
            Expression::Lit(lit) => {
                let b = builder.create_block();
                builder.switch_to_block(b);
                let ctx_ptr: Value = builder.use_var(ctx_ptr_var);
                let lit_num: i64 = lit;

                let imm = builder.ins().iconst(target_type, lit_num);

                match translation_ctx.tr_type {
                    TranslationType::Default => {
                        builder
                            .ins()
                            .store(MemFlags::new(), imm, ctx_ptr, PROCESS_CTX_TEMP_VAL);
                    }
                    TranslationType::Call(arg_i) => {
                        let args_ptr = builder.ins().load(
                            target_type,
                            MemFlags::new(),
                            ctx_ptr,
                            PROCESS_CTX_CALL_ARGS_TEMP,
                        );
                        builder
                            .ins()
                            .store(MemFlags::new(), imm, args_ptr, (arg_i * 8) as i32);
                    }
                }

                let block_count = translation_ctx.block_counter;

                let block_count_val = builder.ins().iconst(target_type, (block_count + 1) as i64);
                builder.ins().return_(&[block_count_val]);

                translation_ctx.block_counter += 1;

                Ok((vec![block_count], translation_ctx.block_counter, vec![b]))
            }
            Expression::Ident(name) => {
                let b = builder.create_block();
                builder.switch_to_block(b);
                let ctx_ptr: Value = builder.use_var(ctx_ptr_var);
                let val_index = *translation_ctx.variables.get(&name).unwrap();

                let vars_ptr =
                    builder
                        .ins()
                        .load(target_type, MemFlags::new(), ctx_ptr, PROCESS_CTX_VARS);

                let val_index = builder.ins().iconst(target_type, val_index as i64);
                let offset = builder.ins().imul_imm(val_index, 8);

                let ptr_with_offset = builder.ins().iadd(vars_ptr, offset);

                let val = builder
                    .ins()
                    .load(target_type, MemFlags::new(), ptr_with_offset, 0);

                match translation_ctx.tr_type {
                    TranslationType::Default => {
                        builder
                            .ins()
                            .store(MemFlags::new(), val, ctx_ptr, PROCESS_CTX_TEMP_VAL);
                    }
                    TranslationType::Call(arg_i) => {
                        let args_ptr = builder.ins().load(
                            target_type,
                            MemFlags::new(),
                            ctx_ptr,
                            PROCESS_CTX_CALL_ARGS_TEMP,
                        );
                        builder
                            .ins()
                            .store(MemFlags::new(), val, args_ptr, (arg_i * 8) as i32);
                    }
                }

                let block_count = translation_ctx.block_counter;

                let block_count_val = builder.ins().iconst(target_type, (block_count + 1) as i64);
                builder.ins().return_(&[block_count_val]);

                translation_ctx.block_counter += 1;

                Ok((vec![block_count], translation_ctx.block_counter, vec![b]))
            }
            Expression::BeforeCall(args_len) => {
                let b = builder.create_block();
                builder.switch_to_block(b);
                let after_call = builder.create_block();
                builder.append_block_param(after_call, target_type);
                let buffer_size = builder.ins().iconst(target_type, (args_len * 8) as i64);
                call_malloc(&mut self.module, builder, buffer_size, after_call, &[]);
                builder.switch_to_block(after_call);
                let args_ptr = *builder.block_params(after_call).first().unwrap();

                let ctx_ptr = builder.use_var(ctx_ptr_var);
                builder.ins().store(
                    MemFlags::new(),
                    args_ptr,
                    ctx_ptr,
                    PROCESS_CTX_CALL_ARGS_TEMP,
                );

                let block_count = translation_ctx.block_counter;

                let block_count_val = builder.ins().iconst(target_type, (block_count + 1) as i64);
                builder.ins().return_(&[block_count_val]);

                translation_ctx.block_counter += 1;

                Ok((vec![block_count], translation_ctx.block_counter, vec![b]))
            }
            Expression::Call { ident, args } => {
                let mut indecies = vec![];
                let mut blocks = vec![];
                let args_len = args.0.len();
                for (i, expression) in args.0.into_iter().enumerate() {
                    translation_ctx.tr_type = TranslationType::Call(i);
                    let (indecies_, last_block_, blocks_) = self.translate_expression(
                        expression,
                        builder,
                        ctx_ptr_var,
                        runtime_var,
                        translation_ctx,
                    )?;
                    indecies = [indecies, indecies_].concat();
                    blocks = [blocks, blocks_].concat();
                }
                translation_ctx.tr_type = TranslationType::Default;
                let b = builder.create_block();
                builder.switch_to_block(b);

                let ctx_ptr = builder.use_var(ctx_ptr_var);
                let args_ptr = builder.ins().load(
                    target_type,
                    MemFlags::new(),
                    ctx_ptr,
                    PROCESS_CTX_CALL_ARGS_TEMP,
                );

                let mut wp = Whirlpool::new();

                let Expression::Ident(name) = *ident else {
                    bail!("Not a ident")
                };

                Digest::update(&mut wp, name.as_bytes());
                let encoded_function_name = Base64::encode_string(&wp.finalize());

                let mut sig = self.module.make_signature();
                let (func_id, params, returns) =
                    FUNCTIONS.with(|map| *map.borrow().get(&encoded_function_name).unwrap());
                for _ in 0..params {
                    sig.params.push(AbiParam::new(target_type));
                }
                for _ in 0..returns {
                    sig.returns.push(AbiParam::new(target_type));
                }

                let callee = self.module.declare_func_in_func(func_id, builder.func);
                let callee = builder.ins().func_addr(target_type, callee);
                let sig_ref = builder.import_signature(sig);

                let mut args_vals = Vec::with_capacity(args_len);

                for i in 0..args_len {
                    args_vals.push(builder.ins().load(
                        target_type,
                        MemFlags::new(),
                        args_ptr,
                        (i * 8) as i32,
                    ));
                }

                let call = builder.ins().call_indirect(sig_ref, callee, &args_vals);

                call_free(&mut self.module, builder, args_ptr);
                let zero = builder.ins().iconst(target_type, 0);
                builder
                    .ins()
                    .store(MemFlags::new(), zero, ctx_ptr, PROCESS_CTX_CALL_ARGS_TEMP);

                let res: Option<&Value> = builder.inst_results(call).first();
                if let Some(res) = res {
                    let res = *res;
                    builder
                        .ins()
                        .store(MemFlags::new(), res, ctx_ptr, PROCESS_CTX_TEMP_VAL);
                }

                /*let deps_ptr = builder.ins().load(
                target_type,
                MemFlags::new(),
                ctx_ptr,
                PROCESS_CTX_DEPENDENCIES,
                );*/

                let block_count = translation_ctx.block_counter;

                let block_count_val = builder.ins().iconst(target_type, (block_count + 1) as i64);
                builder.ins().return_(&[block_count_val]);

                translation_ctx.block_counter += 1;

                Ok((
                    [indecies, vec![block_count]].concat(),
                    translation_ctx.block_counter,
                    [blocks, vec![b]].concat(),
                ))
            }
            Expression::Function {
                name,
                function_ty,
                body,
            } => todo!(),
            Expression::FunctionType { params, ret_ty } => todo!(),
            Expression::Assign((name, _), expr) => {
                let tr_type = translation_ctx.tr_type;
                translation_ctx.tr_type = TranslationType::Default;
                let (indecies, block_index, blocks) = self.translate_expression(
                    *expr,
                    builder,
                    ctx_ptr_var,
                    runtime_var,
                    translation_ctx,
                )?;
                translation_ctx.tr_type = tr_type;
                let b = builder.create_block();
                builder.switch_to_block(b);

                let ctx_ptr = builder.use_var(ctx_ptr_var);
                let old_vars_ptr =
                    builder
                        .ins()
                        .load(target_type, MemFlags::new(), ctx_ptr, PROCESS_CTX_VARS);
                let old_vars_ptr_len =
                    builder
                        .ins()
                        .load(target_type, MemFlags::new(), ctx_ptr, PROCESS_CTX_VARS_LEN);

                let new_len = builder.ins().iadd_imm(old_vars_ptr_len, 1);
                let new_buffer_size = builder.ins().imul_imm(new_len, 8);

                let new_len_var = builder.declare_var(target_type);
                builder.def_var(new_len_var, new_len);

                let after_realloc = builder.create_block();
                builder.append_block_param(after_realloc, target_type);
                call_realloc(
                    &mut self.module,
                    builder,
                    old_vars_ptr,
                    new_buffer_size,
                    after_realloc,
                    &[],
                );
                builder.switch_to_block(after_realloc);

                let new_ptr = *builder.block_params(after_realloc).first().unwrap();
                let ctx_ptr = builder.use_var(ctx_ptr_var);
                let new_len = builder.use_var(new_len_var);

                builder
                    .ins()
                    .store(MemFlags::new(), new_ptr, ctx_ptr, PROCESS_CTX_VARS);
                builder
                    .ins()
                    .store(MemFlags::new(), new_len, ctx_ptr, PROCESS_CTX_VARS_LEN);

                let last_i = builder.ins().iadd_imm(new_len, -1);
                let offset = builder.ins().imul_imm(last_i, 8);
                let ptr_with_offset = builder.ins().iadd(new_ptr, offset);

                let val =
                    builder
                        .ins()
                        .load(target_type, MemFlags::new(), ctx_ptr, PROCESS_CTX_TEMP_VAL);

                builder
                    .ins()
                    .store(MemFlags::new(), val, ptr_with_offset, 0);

                if let TranslationType::Call(arg_i) = translation_ctx.tr_type {
                    let args_ptr = builder.ins().load(
                        target_type,
                        MemFlags::new(),
                        ctx_ptr,
                        PROCESS_CTX_CALL_ARGS_TEMP,
                    );
                    builder
                        .ins()
                        .store(MemFlags::new(), val, args_ptr, (arg_i * 8) as i32);
                }

                let next_block = builder
                    .ins()
                    .iconst(target_type, (translation_ctx.block_counter + 1) as i64);
                builder.ins().return_(&[next_block]);

                let Expression::Ident(name) = *name else {
                    bail!("Not a ident")
                };

                let val = translation_ctx.var_counter;
                translation_ctx.variables.insert(name, val);
                translation_ctx.var_counter += 1;

                translation_ctx.block_counter += 1;

                Ok((
                    [indecies, vec![block_index]].concat(),
                    translation_ctx.block_counter,
                    [blocks, vec![b]].concat(),
                ))
            }
            Expression::Block(body) => {
                let mut indecies = vec![];
                let mut blocks = vec![];
                for expression in body.0 {
                    let (indecies_, last_block_, blocks_) = self.translate_expression(
                        expression,
                        builder,
                        ctx_ptr_var,
                        runtime_var,
                        translation_ctx,
                    )?;
                    indecies = [indecies, indecies_].concat();
                    blocks = [blocks, blocks_].concat();
                }
                let b = builder.create_block();
                builder.switch_to_block(b);

                let ctx_ptr = builder.use_var(ctx_ptr_var);

                if let TranslationType::Call(arg_i) = translation_ctx.tr_type {
                    let res_of_block = builder.ins().load(
                        target_type,
                        MemFlags::new(),
                        ctx_ptr,
                        PROCESS_CTX_TEMP_VAL,
                    );
                    let args_ptr = builder.ins().load(
                        target_type,
                        MemFlags::new(),
                        ctx_ptr,
                        PROCESS_CTX_CALL_ARGS_TEMP,
                    );
                    builder.ins().store(
                        MemFlags::new(),
                        res_of_block,
                        args_ptr,
                        (arg_i * 8) as i32,
                    );
                }

                let block_count = translation_ctx.block_counter;

                let block_count_val = builder.ins().iconst(target_type, (block_count + 1) as i64);
                builder.ins().return_(&[block_count_val]);

                translation_ctx.block_counter += 1;

                Ok((
                    [indecies, vec![block_count]].concat(),
                    translation_ctx.block_counter,
                    [blocks, vec![b]].concat(),
                ))
            }
            _ => unimplemented!(),
        }
    }
}

pub fn call_realloc(
    module: &mut dyn Module,
    builder: &mut FunctionBuilder,
    old_ptr: Value,
    buffer_size: Value,
    block_after_call: Block,
    block_args: &[BlockArg],
) {
    let ty = module.target_config().pointer_type();
    let mut realloc_sig = module.make_signature();
    realloc_sig.params.push(AbiParam::new(ty));
    realloc_sig.params.push(AbiParam::new(ty));
    realloc_sig.returns.push(AbiParam::new(ty));

    let callee_realloc = module
        .declare_function("realloc", Linkage::Import, &realloc_sig)
        .unwrap();
    let local_callee_realloc = module.declare_func_in_func(callee_realloc, builder.func);

    let call = builder
        .ins()
        .call(local_callee_realloc, &[old_ptr, buffer_size]);
    let ptr: Value = *builder.inst_results(call).get(0).unwrap();

    let cond_block = builder.create_block();
    for _ in block_args {
        builder.append_block_param(cond_block, ty);
    }
    builder.append_block_param(cond_block, ty);

    let trap_block = builder.create_block();

    builder
        .ins()
        .jump(cond_block, &[block_args, &[BlockArg::Value(ptr)]].concat());

    builder.switch_to_block(cond_block);
    builder.seal_block(cond_block);

    let len = builder.block_params(cond_block).len();
    let ptr = *builder.block_params(cond_block).last().unwrap();
    let block_args: Vec<BlockArg> = (&builder.block_params(cond_block)[..len - 1])
        .iter()
        .map(|x| BlockArg::Value(*x))
        .collect();

    let is_null = builder.ins().icmp_imm(IntCC::Equal, ptr, 0);
    builder.ins().brif(
        is_null,
        trap_block,
        &[],
        block_after_call,
        &[&block_args[..], &[BlockArg::Value(ptr)]].concat(),
    );

    builder.switch_to_block(trap_block);
    builder.seal_block(trap_block);
    builder.ins().trap(TrapCode::HEAP_OUT_OF_BOUNDS);
}
