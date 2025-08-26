use std::ops::Deref;
use cranelift::codegen::ir::BlockArg;
use cranelift::module::Module;
use cranelift::prelude::{Block, FunctionBuilder, InstBuilder, MemFlags, Variable};
use crate::aot::STORE_FUNCTIONS;
use crate::general_compiler::call_malloc;

pub fn create_process(module: &mut dyn Module, builder: &mut FunctionBuilder) -> Variable {
    let target_type = module.target_config().pointer_type();

    let func_addr_size : i64 = 8;
    let virtual_process_ctx_size = func_addr_size;
    let virtual_process_dependencies_size = 8;
    let virtual_process_size = virtual_process_ctx_size;

    let virtual_process_size = builder.ins()
        .iconst(target_type, virtual_process_size);
    let after_call_block = builder.create_block();
    builder.append_block_param(after_call_block, target_type);
    call_malloc(module, builder, virtual_process_size, after_call_block, &[]);

    builder.switch_to_block(after_call_block);
    builder.seal_block(after_call_block);
    let ptr = *builder.block_params(after_call_block).get(0).unwrap();

    let after_call_block = builder.create_block();
    builder.append_block_param(after_call_block, target_type);
    builder.append_block_param(after_call_block, target_type);
    let virtual_process_ctx_size = builder.ins()
        .iconst(target_type, virtual_process_ctx_size);
    call_malloc(
        module,
        builder,
        virtual_process_ctx_size,
        after_call_block,
        &[BlockArg::Value(ptr)]
    );

    builder.switch_to_block(after_call_block);
    builder.seal_block(after_call_block);
    let new_process_ptr = *builder.block_params(after_call_block).get(0).unwrap();
    let process_ctx_ptr = *builder.block_params(after_call_block).get(1).unwrap();

    let map = STORE_FUNCTIONS.read().unwrap().deref().clone();
    let (id, _sig) = map.into_iter().nth(0).unwrap();
    let callee = module.declare_func_in_func(id, builder.func);
    let func_addr = builder.ins().func_addr(target_type, callee);
    builder.ins().store(MemFlags::new(), func_addr, process_ctx_ptr, 0);
    builder.ins().store(MemFlags::new(), process_ctx_ptr, new_process_ptr, 0);

    let process_ptr = builder.declare_var(target_type);
    builder.def_var(process_ptr, new_process_ptr);
    process_ptr
}