use crate::frontend::parser::ast::expr::Expr;

#[derive(Debug)]
pub struct Expressions(pub Vec<Expression>);

#[derive(Debug)]
pub enum Expression {
    Lit(i64),
    Ident(String),
    Call {
        ident: Box<Expression>,
        args: Expressions,
    },
    ReturnCall {
        ident: Box<Expression>,
        args: Expressions,
    },
    FFICall {
        ident: Box<Expression>,
        args: Expressions,
    },
    BeforeCall(usize),
    Assign((Box<Expression>, Box<Expression>), Box<Expression>),
    Function {
        name: Box<Expression>,
        function_ty: Box<Expression>,
        body: Expressions,
    },
    FunctionType {
        params: Vec<(Expression, Expression)>,
        ret_ty: Box<Expression>,
    },
    Block(Expressions),
}

impl From<Vec<Expr>> for Expressions {
    fn from(value: Vec<Expr>) -> Self {
        let mut expressions = vec![];
        let exprs_len = value.len();
        for (i, expr) in value.into_iter().enumerate() {
            match expr {
                Expr::Call { ident, args } => {
                    let ident = Box::new(Expression::from(*ident));
                    let args = Expressions::from(args);
                    expressions.append(&mut vec![
                        Expression::BeforeCall(args.0.len()),
                        if i == exprs_len - 1 {
                            Expression::ReturnCall { ident, args }
                        } else {
                            Expression::Call { ident, args }
                        },
                    ])
                }
                expr => expressions.push(Expression::from(expr)),
            }
        }
        Expressions(expressions)
    }
}

impl From<Expr> for Expression {
    fn from(value: Expr) -> Self {
        match value {
            Expr::Lit(lit) => Expression::Lit(lit.parse().unwrap()),
            Expr::Ident(ident) => Expression::Ident(ident),
            Expr::FunctionType { params, ret_ty } => {
                let params = params
                    .into_iter()
                    .map(|(ident, ty)| (Expression::from(ident), Expression::from(ty)))
                    .collect::<Vec<_>>();

                let ret_ty = Box::new(Expression::from(*ret_ty));
                Expression::FunctionType { params, ret_ty }
            }
            Expr::Function {
                name,
                function_ty,
                body,
            } => {
                let name = Box::new(Expression::from(*name));
                let function_ty = Box::new(Expression::from(*function_ty));
                let body = Expressions::from(body);
                Expression::Function {
                    name,
                    function_ty,
                    body,
                }
            }
            Expr::Assign((ident, ty), expr) => {
                let ident = Box::new(Expression::from(*ident));
                let ty = Box::new(Expression::from(*ty));
                let expr = Box::new(Expression::from(*expr));
                Expression::Assign((ident, ty), expr)
            }
            Expr::Call { ident, args } => {
                let ident = Box::new(Expression::from(*ident));
                let args = Expressions::from(args);
                Expression::Block(Expressions(vec![
                    Expression::BeforeCall(args.0.len()),
                    Expression::Call { ident, args },
                ]))
            }

            _ => unimplemented!(),
        }
    }
}
