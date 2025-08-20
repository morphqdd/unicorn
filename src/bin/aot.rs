use std::{fs, path::PathBuf, process::Command};

use anyhow::*;
use unicorn::aot::Aot;

const FOO_CODE: &str =
    r#"
        main: -> i64 {
            foo {}
        }

        foo: -> i64 {
            20
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
            &out.join("obj.o").display().to_string(),
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
