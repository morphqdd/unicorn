use anyhow::*;
use unicorn::aot::Aot;

const FOO_CODE: &str = "main: -> i64 { 20; }";

fn main() -> Result<()> {
    let aot = Aot::default();
    aot.compile(FOO_CODE)?;
    Ok(())
}
