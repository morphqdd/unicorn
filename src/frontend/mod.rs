use peg::*;

parser! {
    pub grammar parser() for str {
        pub rule number() -> i32 = n:$(['0'..='9']+) { ? n.parse().or(Err("i32")) }
        pub rule binary_op() -> i32 = precedence! {
            a:@ _ "+" _ b:(@) { a + b }
            --
            a:@ _ "*" _ b:(@) { a * b }
            --
            n:number() { n }
        }
        rule _() = quiet!{[' ' | '\t']*}
    }
}


#[cfg(test)]
mod test {
    use crate::frontend::parser;

    #[test]
    fn number_parse() {
        assert_eq!(parser::number("32"), Ok(32))
    }

    #[test]
    fn sum_parse() {
        assert_eq!(parser::binary_op("3 + 2"), Ok(5))
    }

    #[test]
    fn wrong_expr() {
        assert_ne!(parser::binary_op("4 * 3 + 2"), Ok(20))
    }
}
