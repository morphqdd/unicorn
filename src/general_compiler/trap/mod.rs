use cranelift::prelude::TrapCode;

pub enum CompilerTrapCode {
    EndOfBlocks,
}

impl From<CompilerTrapCode> for TrapCode {
    fn from(value: CompilerTrapCode) -> Self {
        match value {
            CompilerTrapCode::EndOfBlocks => TrapCode::user(25).unwrap(),
        }
    }
}
