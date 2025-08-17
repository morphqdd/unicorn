use peg::*;
use crate::frontend::parser::ast::expr::Expr;
pub use crate::frontend::parser::parser::*;

pub mod ast;

parser! {
    pub grammar parser() for str {
        pub rule number() -> Expr = n:$(['0'..='9']+) { Expr::Lit(n.to_string()) }
        rule _() = quiet!{[' ' | '\t']*}
    }
}


#[cfg(test)]
mod test {
    use crate::frontend::parser::{self, ast::expr::Expr};

    #[test]
    fn number_parse() {
        assert_eq!(parser::number("32"), Ok(Expr::Lit("32".into())))
    }
}
