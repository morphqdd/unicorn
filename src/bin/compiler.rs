use anyhow::*;
use std::{fs, path::PathBuf, process::Command};

use unicorn::backend::Compiler;

fn main() -> Result<()> {
    let out = PathBuf::from("build");
    if !out.exists() {
        std::fs::create_dir(&out)?;
    }
    let input = fs::read_to_string("./examples/hello.uniq")?;
    let compiler = Compiler::default();
    compiler.compile(&input, &out)?;

    let linker = Command::new("cc")
        .args([
            "-Wl,-s",
            "-fuse-ld=mold",
            &out.join("obj.o").display().to_string(),
//            &out.join("runtime.o").display().to_string(),
            "-o",
            &out.join("aot-test").display().to_string(),
        ])
        .status()?;
    if !linker.success() {
        bail!("Linker failed with code: {}", linker.code().unwrap())
    }

    println!("Done!");
    Ok(())
}
