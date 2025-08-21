use std::{fs, path::PathBuf, process::Command};

use anyhow::*;
use unicorn::aot::Aot;

const FOO_CODE: &str = r#"
        main: -> i64 {
            let a: i64 = foo { 20 30 }
            stdprint { a }
        }

        foo: a(i64) b(i64) -> i64 {
            add { a b }
        }
    "#;

fn main() -> Result<()> {
    let out = PathBuf::from("build");
    if !out.exists() {
        fs::create_dir(&out)?;
    }
    let aot = Aot::default();
    aot.compile(FOO_CODE, &out)?;
    let linker = Command::new("cc")
        .args([
            "-Wl,-s",
            &out.join("obj.o").display().to_string(),
            &out.join("runtime.o").display().to_string(),
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
