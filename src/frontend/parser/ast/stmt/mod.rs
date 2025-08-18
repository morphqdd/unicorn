use crate::frontend::parser::ast::expr::Expr;

pub enum Stmt {
    Expr(Expr),
}
