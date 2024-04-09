use std::collections::HashMap;

use minijinja::machinery::ast;

use super::{
    pretty_print::pretty_print,
    types::{PredefinedTypes, Type},
    ScopeTracker, TypeError,
};

fn tracker_visit_expr<'a>(
    expr: &ast::Expr<'a>,
    state: &mut ScopeTracker,
    types: &PredefinedTypes,
) -> Type {
    match expr {
        ast::Expr::Var(var) => match types.resolve(var.id) {
            Some(t) => t,
            None => {
                state.errors.push(TypeError::new_unresolved_variable(
                    var.id,
                    var.span(),
                    types.variable_names(),
                ));
                Type::Unknown
            }
        },
        ast::Expr::Const(c) => infer_const_type(&c.value),
        ast::Expr::UnaryOp(expr) => {
            let expected = match expr.op {
                ast::UnaryOpKind::Not => Type::Bool,
                ast::UnaryOpKind::Neg => Type::Number,
            };

            let inner = tracker_visit_expr(&expr.expr, state, types);
            // TODO: Check for type compatibility

            expected
        }
        ast::Expr::BinOp(expr) => {
            let lhs = tracker_visit_expr(&expr.left, state, types);
            let rhs = tracker_visit_expr(&expr.right, state, types);
            // TODO: Check for type compatibility

            match expr.op {
                ast::BinOpKind::Add => {
                    if lhs == Type::String || rhs == Type::String {
                        Type::String
                    } else {
                        Type::Number
                    }
                }
                ast::BinOpKind::Sub => Type::Number,
                ast::BinOpKind::Mul => Type::Number,
                ast::BinOpKind::Div => Type::Number,
                ast::BinOpKind::Pow => Type::Number,
                ast::BinOpKind::FloorDiv => Type::Number,
                ast::BinOpKind::Rem => Type::Number,
                ast::BinOpKind::Eq => Type::Bool,
                ast::BinOpKind::Ne => Type::Bool,
                ast::BinOpKind::Lt => Type::Bool,
                ast::BinOpKind::Gt => Type::Bool,
                ast::BinOpKind::In => Type::Bool,
                ast::BinOpKind::Lte => Type::Bool,
                ast::BinOpKind::Gte => Type::Bool,
                ast::BinOpKind::Concat => Type::String,
                ast::BinOpKind::ScAnd => Type::Bool,
                ast::BinOpKind::ScOr => Type::Bool,
            }
        }
        ast::Expr::IfExpr(expr) => {
            let _test = tracker_visit_expr(&expr.test_expr, state, types);

            let true_expr = tracker_visit_expr(&expr.true_expr, state, types);
            let false_expr = expr
                .false_expr
                .as_ref()
                .map(|x| tracker_visit_expr(x, state, types))
                .unwrap_or(Type::Unknown);
            Type::merge([true_expr, false_expr])
        }
        ast::Expr::Filter(expr) => match expr.name {
            "items" => {
                let inner = tracker_visit_expr(expr.expr.as_ref().unwrap(), state, types);
                match inner {
                    Type::Map(k, v) => Type::List(Box::new(Type::Tuple(vec![*k, *v]))),
                    _ => Type::Unknown,
                }
            }
            _ => Type::Unknown,
        },
        ast::Expr::Test(expr) => {
            let _test = tracker_visit_expr(&expr.expr, state, types);
            // TODO: Check for type compatibility
            Type::Bool
        }
        ast::Expr::GetAttr(expr) => {
            let parent = tracker_visit_expr(&expr.expr, state, types);

            match &parent {
                Type::ClassRef(c) => {
                    let (t, err) =
                        types.check_property(&pretty_print(&expr.expr), &c, expr.name, expr.span());
                    if let Some(e) = err {
                        state.errors.push(e);
                    }
                    t
                }
                t => {
                    state.errors.push(TypeError::new_invalid_type(
                        &expr.expr,
                        t,
                        "class",
                        expr.span(),
                    ));
                    Type::Unknown
                }
            }
        }
        ast::Expr::GetItem(expr) => Type::Unknown,
        ast::Expr::Slice(slice) => Type::Unknown,
        ast::Expr::Call(expr) => {
            let func = tracker_visit_expr(&expr.expr, state, types);

            match func {
                Type::FunctionRef(name) => {
                    // lets segregate positional and keyword arguments
                    let mut positional_args = Vec::new();
                    let mut kwargs = HashMap::new();
                    for arg in &expr.args {
                        match arg {
                            ast::Expr::Kwargs(kkwargs) => {
                                for (k, v) in &kkwargs.pairs {
                                    let t = tracker_visit_expr(v, state, types);
                                    kwargs.insert(*k, t);
                                }
                            }
                            _ => {
                                let t = tracker_visit_expr(arg, state, types);
                                positional_args.push(t);
                            }
                        }
                    }

                    let res = types.check_function_args((&name, expr), &positional_args, &kwargs);
                    state.errors.extend(res.1);
                    res.0
                }
                t => {
                    state.errors.push(TypeError::new_invalid_type(
                        &expr.expr,
                        &t,
                        "function",
                        expr.span(),
                    ));
                    Type::Unknown
                }
            }
        }
        ast::Expr::List(expr) => {
            let inner = Type::merge(
                expr.items
                    .iter()
                    .map(|x| tracker_visit_expr(x, state, types)),
            );
            Type::List(Box::new(inner))
        }
        ast::Expr::Map(expr) => {
            let keys = Type::merge(
                expr.keys
                    .iter()
                    .map(|x| tracker_visit_expr(x, state, types)),
            );
            let values = Type::merge(
                expr.values
                    .iter()
                    .map(|x| tracker_visit_expr(x, state, types)),
            );
            Type::Map(Box::new(keys), Box::new(values))
        }
        ast::Expr::Kwargs(expr) => Type::Unknown,
    }
}

fn infer_const_type(v: &minijinja::value::Value) -> Type {
    match v.kind() {
        minijinja::value::ValueKind::Undefined => Type::Undefined,
        minijinja::value::ValueKind::None => Type::None,
        minijinja::value::ValueKind::Bool => Type::Bool,
        minijinja::value::ValueKind::String => Type::String,
        minijinja::value::ValueKind::Seq => {
            let list = v.as_seq().unwrap();
            match list.item_count() {
                0 => Type::List(Box::new(Type::Unknown)),
                _ => {
                    let inner = list
                        .iter()
                        .map(|x| infer_const_type(&x))
                        .fold(None, |acc, x| match acc {
                            None => Some(x),
                            Some(Type::Union(mut acc)) => {
                                if acc.contains(&x) {
                                    Some(Type::Union(acc))
                                } else {
                                    acc.push(x);
                                    Some(Type::Union(acc))
                                }
                            }
                            Some(acc) => {
                                if acc == x {
                                    Some(acc)
                                } else {
                                    Some(Type::Union(vec![acc, x]))
                                }
                            }
                        })
                        .unwrap();
                    Type::List(Box::new(inner))
                }
            }
        }
        minijinja::value::ValueKind::Map => Type::Unknown,
        // We don't handle these types
        minijinja::value::ValueKind::Number => Type::Number,
        minijinja::value::ValueKind::Bytes => Type::Undefined,
    }
}

pub(super) fn evaluate_type(
    expr: &ast::Expr,
    types: &PredefinedTypes,
) -> Result<Type, Vec<TypeError>> {
    let mut state = ScopeTracker::new();
    let result = tracker_visit_expr(expr, &mut state, types);

    if state.errors.is_empty() {
        Ok(result)
    } else {
        Err(state.errors)
    }
}
