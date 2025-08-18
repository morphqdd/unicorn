use crate::frontend::parser::ast::expr::Expr;
pub use crate::frontend::parser::parser::*;
use peg::*;

pub mod ast;

parser! {
    pub grammar parser() for str {
        pub rule function() -> Expr
            = _ name:ident() _ ":" _ t:ty() _ "{" _ body:exprs() _ "}" _
            { Expr::Function { name: Box::new(name), function_ty: Box::new(t), body } }
        rule function_ty() -> Expr
            = _ params:(( i:expr() "(" _ t:ty() _ ")" { (i, t) }) ** " ")
            _ "->" _ ret_ty:(_ i:ident() _ {i}) { Expr::FunctionType { params, ret_ty: Box::new(ret_ty) } }
        rule exprs() -> Vec<Expr> = _ n:(_ e:expr() _ ";" _ {e})* _ { n }
        rule expr() -> Expr = function() / assign() / call() / ident() / literal()
        rule assign() -> Expr = _ "let" _ i:ident() _ ":" _ t:ty() _ "=" _ e:expr() _ { Expr::Assign((Box::new(i), Box::new(t)), Box::new(e)) }
        rule ty() -> Expr = function_ty() / ident()
        rule call() -> Expr = _ i:ident() _ "{" _ args:((e:expr() { e }) ** " ") _ "}" _ { Expr::Call { ident: Box::new(i), args } }
        rule ident() -> Expr
            = quiet!{ n:$(['a'..='z' | 'A'..='Z' | '_']['a'..='z' | 'A'..='Z' | '0'..='9' | '_']*)
            { Expr::Ident(n.to_owned()) } }
        / expected!("identifier")
        rule literal() -> Expr
            = n:$(['0'..='9']+) { Expr::Lit(n.to_owned()) }
            / "&" i:ident() { Expr::GlobalDataAddr(Box::new(i)) }
        rule _() = quiet!{[' ' | '\t' | '\n' ]*}
    }
}

#[cfg(test)]
mod test {
    use std::vec;

    use crate::frontend::parser::{self, ast::expr::Expr};

    #[test]
    fn simple_function_parse() {
        assert_eq!(
            parser::function("foo : a(T) b(None) -> nil {}"),
            Ok(Expr::Function {
                name: Box::new(Expr::Ident(String::from("foo"))),
                function_ty: Box::new(Expr::FunctionType {
                    params: vec![
                        (
                            Expr::Ident(String::from("a")),
                            Expr::Ident(String::from("T"))
                        ),
                        (
                            Expr::Ident(String::from("b")),
                            Expr::Ident(String::from("None"))
                        )
                    ],
                    ret_ty: Box::new(Expr::Ident(String::from("nil"))),
                }),
                body: vec![]
            })
        )
    }

    #[test]
    fn complicated_function_parse() {
        assert_eq!(
            parser::function(
                r#"foo : bar( a(i32) b(i32) -> nil ) -> nil {
                    buzz : a(i32) b(i32) -> nil {}; 
                    nil; 
                }"#
            ),
            Ok(Expr::Function {
                name: Box::new(Expr::Ident("foo".into())),
                function_ty: Box::new(Expr::FunctionType {
                    params: vec![(
                        Expr::Ident("bar".into()),
                        Expr::FunctionType {
                            params: vec![
                                (Expr::Ident("a".into()), Expr::Ident("i32".into())),
                                (Expr::Ident("b".into()), Expr::Ident("i32".into()))
                            ],
                            ret_ty: Box::new(Expr::Ident("nil".into())),
                        }
                    )],
                    ret_ty: Box::new(Expr::Ident("nil".into())),
                }),
                body: vec![
                    Expr::Function {
                        name: Box::new(Expr::Ident("buzz".into())),
                        function_ty: Box::new(Expr::FunctionType {
                            params: vec![
                                (Expr::Ident("a".into()), Expr::Ident("i32".into())),
                                (Expr::Ident("b".into()), Expr::Ident("i32".into()))
                            ],
                            ret_ty: Box::new(Expr::Ident("nil".into())),
                        }),
                        body: vec![]
                    },
                    Expr::Ident("nil".into())
                ]
            })
        )
    }

    #[test]
    fn let_expr_parse() {
        assert_eq!(
            parser::function("main : -> nil { let a : i32 = 20; nil; } "),
            Ok(Expr::Function {
                name: Box::new(Expr::Ident("main".into())),
                function_ty: Box::new(Expr::FunctionType {
                    params: vec![],
                    ret_ty: Box::new(Expr::Ident("nil".into()))
                }),
                body: vec![
                    Expr::Assign(
                        (
                            Box::new(Expr::Ident("a".into())),
                            Box::new(Expr::Ident("i32".into()))
                        ),
                        Box::new(Expr::Lit("20".into()))
                    ),
                    Expr::Ident("nil".into())
                ]
            })
        )
    }

    /// It's fun, but I haven't found a use for it yet))
    #[test]
    fn some_strange_things() {
        assert_eq!(
            parser::function("main : i32 {} "),
            Ok(Expr::Function {
                name: Box::new(Expr::Ident("main".into())),
                function_ty: Box::new(Expr::Ident("i32".into())),
                body: vec![]
            })
        );
    }

    #[test]
    fn call_parse() {
        assert_eq!(
            parser::function(
                r#"main : -> nil { 
                    b : a(i32) -> nil {};
                    b { 20 };
                } "#
            ),
            Ok(Expr::Function {
                name: Box::new(Expr::Ident("main".into())),
                function_ty: Box::new(Expr::FunctionType {
                    params: vec![],
                    ret_ty: Box::new(Expr::Ident("nil".into()))
                }),
                body: vec![
                    Expr::Function {
                        name: Box::new(Expr::Ident("b".into())),
                        function_ty: Box::new(Expr::FunctionType {
                            params: vec![(Expr::Ident("a".into()), Expr::Ident("i32".into())),],
                            ret_ty: Box::new(Expr::Ident("nil".into()))
                        }),
                        body: vec![]
                    },
                    Expr::Call {
                        ident: Box::new(Expr::Ident("b".into())),
                        args: vec![Expr::Lit("20".into())]
                    }
                ]
            })
        );
    }
}
