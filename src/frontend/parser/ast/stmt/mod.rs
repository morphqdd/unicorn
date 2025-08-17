use crate::frontend::parser::ast::expr::Expr;

pub enum Stmt {
    Expr(Expr),
    Function {
        name: Expr,
        params: Vec<Expr>,
        ret_ty: Expr,
        body: Vec<Stmt>
    }
}
