use anyhow::*;
use unicorn::jit::Jit;

const FOO_CODE: &str = "main: -> i64 { 20; }";

fn main() -> Result<()> {
    let jit = Jit::default();
    let res: i64 = unsafe { run_code(jit, FOO_CODE, ())? };
    println!("Result: {res}");
    Ok(())
}

/// Executes the given code using the cranelift JIT compiler.
///
/// Feeds the given input into the JIT compiled function and returns the resulting output.
///
/// # Safety
///
/// This function is unsafe since it relies on the caller to provide it with the correct
/// input and output types. Using incorrect types at this point may corrupt the program's state.
unsafe fn run_code<I, O>(jit: Jit, code: &str, input: I) -> Result<O> {
    let code_ptr = jit.compile(code)?;
    let code_fn = unsafe { std::mem::transmute::<*const u8, fn(I) -> O>(code_ptr) };
    Ok(code_fn(input))
}
