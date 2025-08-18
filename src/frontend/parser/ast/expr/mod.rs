#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Expr {
    Ident(String),
    Call {
        ident: Box<Expr>,
        args: Vec<Expr>,
    },
    Lit(String),
    Function {
        name: Box<Expr>,
        function_ty: Box<Expr>,
        body: Vec<Expr>,
    },
    FunctionType {
        params: Vec<(Expr, Expr)>,
        ret_ty: Box<Expr>,
    },
    Assign((Box<Expr>, Box<Expr>), Box<Expr>),
    GlobalDataAddr(Box<Expr>),
}
