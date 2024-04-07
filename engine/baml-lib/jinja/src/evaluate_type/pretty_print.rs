use minijinja::machinery::ast::{self, Expr};

pub(super) fn pretty_print(expr: &Expr) -> String {
    match expr {
        Expr::Var(v) => v.id.to_string(),
        Expr::Const(c) => c.value.to_string(),
        Expr::Slice(c) => format!(
            "({})[{}:{}:{}]",
            pretty_print(&c.expr),
            c.start
                .as_ref()
                .map(|x| pretty_print(&x))
                .unwrap_or("".into()),
            c.stop
                .as_ref()
                .map(|x| pretty_print(&x))
                .unwrap_or("".into()),
            c.step
                .as_ref()
                .map(|x| pretty_print(&x))
                .unwrap_or("".into())
        ),
        Expr::UnaryOp(op) => {
            format!(
                "{}{}",
                match op.op {
                    ast::UnaryOpKind::Not => "not ",
                    ast::UnaryOpKind::Neg => "-",
                },
                pretty_print(&op.expr)
            )
        }
        Expr::BinOp(op) => {
            format!(
                "({} {} {})",
                pretty_print(&op.left),
                match op.op {
                    ast::BinOpKind::Add => "+",
                    ast::BinOpKind::Sub => "-",
                    ast::BinOpKind::Mul => "*",
                    ast::BinOpKind::Div => "/",
                    ast::BinOpKind::Pow => "**",
                    ast::BinOpKind::FloorDiv => "//",
                    ast::BinOpKind::Rem => "%",
                    ast::BinOpKind::Eq => "==",
                    ast::BinOpKind::Ne => "!=",
                    ast::BinOpKind::Lt => "<",
                    ast::BinOpKind::Gt => ">",
                    ast::BinOpKind::In => "in",
                    ast::BinOpKind::Lte => "<=",
                    ast::BinOpKind::Gte => ">=",
                    ast::BinOpKind::Concat => "~",
                    ast::BinOpKind::ScAnd => "and",
                    ast::BinOpKind::ScOr => "or",
                },
                pretty_print(&op.right)
            )
        }
        Expr::IfExpr(expr) => {
            format!(
                "{true_expr} if {test}{false_expr}",
                test = pretty_print(&expr.test_expr),
                true_expr = pretty_print(&expr.true_expr),
                false_expr = expr
                    .false_expr
                    .as_ref()
                    .map(|x| format!(" else {}", pretty_print(&x)))
                    .unwrap_or("".into())
            )
        }
        Expr::Filter(expr) => {
            format!(
                "{}|{}{}",
                expr.expr
                    .as_ref()
                    .map(|x| pretty_print(x))
                    .unwrap_or("".into()),
                expr.name,
                match expr.args.len() {
                    0 => "".into(),
                    _ => format!(
                        "({})",
                        expr.args
                            .iter()
                            .map(|x| pretty_print(&x))
                            .collect::<Vec<_>>()
                            .join(",")
                    ),
                }
            )
        }
        Expr::Test(expr) => {
            format!(
                "{} is {}{}",
                pretty_print(&expr.expr),
                expr.name,
                match expr.args.len() {
                    0 => "".into(),
                    _ => format!(
                        "({})",
                        expr.args
                            .iter()
                            .map(|x| pretty_print(&x))
                            .collect::<Vec<_>>()
                            .join(",")
                    ),
                }
            )
        }
        Expr::GetAttr(attr) => {
            format!("{}.{}", pretty_print(&attr.expr), attr.name)
        }
        Expr::GetItem(expr) => {
            format!(
                "{}[{}]",
                pretty_print(&expr.expr),
                pretty_print(&expr.subscript_expr)
            )
        }
        Expr::Call(expr) => {
            format!(
                "{}({})",
                pretty_print(&expr.expr),
                expr.args
                    .iter()
                    .map(|x| pretty_print(&x))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        Expr::List(expr) => {
            format!(
                "[{}]",
                expr.items
                    .iter()
                    .map(|x| pretty_print(&x))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        Expr::Map(expr) => {
            format!(
                "{{{}}}",
                expr.keys
                    .iter()
                    .zip(expr.values.iter())
                    .map(|(k, v)| format!("{}:{}", pretty_print(&k), pretty_print(&v)))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        Expr::Kwargs(expr) => {
            format!(
                "{{{}}}",
                expr.pairs
                    .iter()
                    .map(|(k, v)| format!("{}={}", k, pretty_print(&v)))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
    }
}
