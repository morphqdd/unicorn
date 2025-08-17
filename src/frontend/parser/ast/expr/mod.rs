#[derive(Debug, PartialEq, Eq)]
pub enum Expr {
    Ident(String),
    Call {
        ident: Box<Expr>,
        args: Vec<Expr>
    },
    Lit(String)
}
