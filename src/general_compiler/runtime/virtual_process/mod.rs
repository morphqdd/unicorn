use cranelift::module::Module;
use cranelift::prelude::{Block, FunctionBuilder, InstBuilder, Variable};
use crate::general_compiler::call_malloc;

pub fn create_process(module: &mut dyn Module, builder: &mut FunctionBuilder) -> Variable {
    let target_type = module.target_config().pointer_type();

    let func_addr_size = 8;
    let virtual_process_ctx_size = func_addr_size;
    let virtual_process_dependencies_size = 8;
    let virtual_process_size = virtual_process_ctx_size + virtual_process_dependencies_size;

    let virtual_process_size = builder.ins()
        .iconst(target_type, virtual_process_size);
    let after_call_block = builder.create_block();
    builder.append_block_param(after_call_block, target_type);
    call_malloc(module, builder, virtual_process_size, after_call_block);

    builder.switch_to_block(after_call_block);
    builder.seal_block(after_call_block);
    let ptr = *builder.block_params(after_call_block).get(0).unwrap();
    let process_ptr = builder.declare_var(target_type);
    builder.def_var(process_ptr, ptr);
    process_ptr
}