use std::fs::create_dir;
use cranelift::frontend::FunctionBuilder;
use cranelift::module::Module;
use cranelift::prelude::{Block, InstBuilder, IntCC, MemFlags, Value, Variable};
use crate::general_compiler::call_malloc;

pub struct Runtime {
    pub processes_ptr: Variable,
    pub current_process: Variable,
    pub v_process_array_len: Variable,
}

pub fn init_runtime(module: &mut dyn Module, builder: &mut FunctionBuilder, end_block: Block) -> Runtime {
    let target_type = module.target_config().pointer_type();
    let zero = builder.ins().iconst(target_type, 0);

    let current_process = builder.declare_var(target_type);
    builder.def_var(current_process, zero);

    let v_process_ptr_size: i64 = 8;
    let capacity = 8;

    let v_process_array_len = builder.declare_var(target_type);
    let initial_len = builder.ins().iconst(target_type, v_process_ptr_size * capacity);
    builder.def_var(v_process_array_len, initial_len);

    let after_call_block = builder.create_block();
    builder.append_block_param(after_call_block, target_type);

    call_malloc(module, builder, initial_len, after_call_block);
    builder.switch_to_block(after_call_block);
    builder.seal_block(after_call_block);

    let ptr = *builder.block_params(after_call_block).get(0).unwrap();
    let processes_ptr = builder.declare_var(target_type);
    builder.def_var(processes_ptr, ptr);

    let entry_block = builder.create_block();
    let counter_block = builder.create_block();
    let condition_block = builder.create_block();
    let action_block = builder.create_block();
    let exit_block = builder.create_block();

    builder
        .ins()
        .jump(
            entry_block,
            &[
            ]
        );


    builder.switch_to_block(entry_block);
    builder.seal_block(entry_block);

    let counter = builder.declare_var(target_type);
    let zero = builder.ins().iconst(target_type, 0);
    builder.def_var(counter, zero);
    builder
        .ins()
        .jump(
            counter_block,
            &[
            ],
        );

    builder.switch_to_block(condition_block);

    let process_array_len: Value = builder.use_var(v_process_array_len);
    let current_counter = builder.use_var(counter);
    let cond_val = builder
        .ins()
        .icmp(IntCC::UnsignedLessThan, current_counter, process_array_len);
    builder
        .ins()
        .brif(
            cond_val,
            action_block,
            &[],
            exit_block,
            &[]
        );

    builder.switch_to_block(action_block);
    builder.seal_block(action_block);

    let ptr: Value = builder.use_var(processes_ptr);
    let current_counter = builder.use_var(counter);
    let offset = builder.ins().imul_imm(current_counter, 8);
    let ptr = builder.ins().iadd(ptr, offset);
    let zero = builder.ins().iconst(target_type, 0);
    builder.ins().store(MemFlags::new(), zero, ptr, 0);
    builder
        .ins()
        .jump(
            counter_block,
            &[]
        );


    builder.switch_to_block(counter_block);
    builder.seal_block(counter_block);

    let current_counter = builder.use_var(counter);
    let new_counter = builder.ins().iadd_imm(current_counter, 1);
    builder.def_var(counter, new_counter);
    builder
        .ins()
        .jump(
            condition_block,
            &[]
        );
    builder.seal_block(condition_block);

    builder.switch_to_block(exit_block);
    builder.seal_block(exit_block);

    builder
        .ins()
        .jump(
            end_block,
            &[]
        );

    Runtime {
        current_process,
        v_process_array_len,
        processes_ptr
    }
}