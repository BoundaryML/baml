use std::collections::HashMap;

use minijinja::machinery::ast;

use super::{
    pretty_print::pretty_print,
    types::{PredefinedTypes, Type},
    ScopeTracker, TypeError,
};

fn parse_as_function_call<'a>(
    expr: &ast::Spanned<ast::Call>,
    state: &mut ScopeTracker,
    types: &PredefinedTypes,
    t: &Type,
) -> (Type, Vec<TypeError>) {
    match t {
        Type::FunctionRef(name) => {
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

            types.check_function_args((&name, expr), &positional_args, &kwargs)
        }
        Type::Both(x, y) => {
            match (x.as_ref(), y.as_ref()) {
                (Type::FunctionRef(_), Type::FunctionRef(_)) => {}
                (Type::FunctionRef(_), _) => return parse_as_function_call(expr, state, types, x),
                (_, Type::FunctionRef(_)) => return parse_as_function_call(expr, state, types, y),
                _ => {}
            }

            let (t1, e1) = parse_as_function_call(expr, state, types, x);
            let (t2, e2) = parse_as_function_call(expr, state, types, y);
            match (e1.is_empty(), e2.is_empty()) {
                (true, true) => (Type::merge([t1, t2]), vec![]),
                (true, false) => (t1, e1),
                (false, true) => (t2, e2),
                (false, false) => (
                    Type::merge([t1, t2]),
                    e1.into_iter().chain(e2.into_iter()).collect(),
                ),
            }
        }
        Type::Union(items) => {
            let items = items
                .iter()
                .map(|x| parse_as_function_call(expr, state, types, x))
                .reduce(|acc, x| {
                    let (t1, e1) = acc;
                    let (t2, e2) = x;
                    (
                        Type::merge([t1, t2]),
                        e1.into_iter().chain(e2.into_iter()).collect(),
                    )
                });
            match items {
                Some(x) => x,
                None => (
                    Type::Unknown,
                    vec![TypeError::new_invalid_type(
                        &expr.expr,
                        t,
                        "function",
                        expr.span(),
                    )],
                ),
            }
        }
        _ => (
            Type::Unknown,
            vec![TypeError::new_invalid_type(
                &expr.expr,
                t,
                "function",
                expr.span(),
            )],
        ),
    }
}

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

            let _inner = tracker_visit_expr(&expr.expr, state, types);
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
        ast::Expr::Filter(expr) => {
            // Filters have a name
            let inner = tracker_visit_expr(expr.expr.as_ref().unwrap(), state, types);

            let mut ensure_type = |error_string: &str| {
                state.errors.push(TypeError::new_invalid_type(
                    expr.expr.as_ref().unwrap(),
                    &inner,
                    error_string,
                    expr.span(),
                ));
            };

            let valid_filters = vec![
                "abs",
                "attrs",
                "batch",
                "bool",
                "capitalize",
                "escape",
                "first",
                "last",
                "default",
                "float",
                "indent",
                "int",
                "dictsort",
                "items",
                "join",
                "length",
                "list",
                "lower",
                "upper",
                "map",
                "max",
                "min",
                "pprint",
                "reject",
                "rejectattr",
                "replace",
                "reverse",
                "round",
                "safe",
                "select",
                "selectattr",
                "slice",
                "sort",
                "split",
                "title",
                "tojson",
                "json",
                "trim",
                "unique",
                "urlencode",
            ];
            match expr.name {
                "abs" => {
                    if inner.matches(&Type::Number) {
                        ensure_type("number");
                    }
                    Type::Number
                }
                "attrs" => Type::Unknown,
                "batch" => Type::Unknown,
                "bool" => Type::Bool,
                "capitalize" | "escape" => {
                    if inner.matches(&Type::String) {
                        ensure_type("string");
                    }
                    Type::String
                }
                "first" | "last" => match inner {
                    Type::List(t) => Type::merge([*t, Type::None]),
                    Type::Unknown => Type::Unknown,
                    _ => {
                        ensure_type("list");
                        Type::Unknown
                    }
                },
                "default" => Type::Unknown,
                "float" => Type::Float,
                "indent" => Type::String,
                "int" => Type::Int,
                "dictsort" | "items" => match inner {
                    Type::Map(k, v) => Type::List(Box::new(Type::Tuple(vec![*k, *v]))),
                    Type::ClassRef(_) => {
                        Type::List(Box::new(Type::Tuple(vec![Type::String, Type::Unknown])))
                    }
                    _ => {
                        ensure_type("map or class");
                        Type::Unknown
                    }
                },
                "join" => Type::String,
                "length" => match inner {
                    Type::List(_) | Type::String | Type::ClassRef(_) | Type::Map(_, _) => Type::Int,
                    Type::Unknown => Type::Unknown,
                    _ => {
                        ensure_type("list, string, class or map");
                        Type::Unknown
                    }
                },
                "list" => Type::List(Box::new(Type::Unknown)),
                "lower" | "upper" => {
                    if inner.matches(&Type::String) {
                        ensure_type("string");
                    }
                    Type::String
                }
                "map" => Type::Unknown,
                "max" => Type::Unknown,
                "min" => Type::Unknown,
                "pprint" => Type::Unknown,
                "reject" => Type::Unknown,
                "rejectattr" => Type::Unknown,
                "replace" => Type::String,
                "reverse" => Type::Unknown,
                "round" => Type::Float,
                "safe" => Type::String,
                "select" => Type::Unknown,
                "selectattr" => Type::Unknown,
                "slice" => Type::Unknown,
                "sort" => Type::Unknown,
                "split" => Type::List(Box::new(Type::String)),
                "title" => Type::String,
                "tojson" | "json" => Type::String,
                "trim" => Type::String,
                "unique" => Type::Unknown,
                "urlencode" => Type::String,
                other => {
                    state.errors.push(TypeError::new_invalid_filter(
                        other,
                        expr.span(),
                        &valid_filters,
                    ));
                    Type::Unknown
                }
            }
        }
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
                Type::Unknown => Type::Unknown,
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
        ast::Expr::GetItem(_expr) => Type::Unknown,
        ast::Expr::Slice(_slice) => Type::Unknown,
        ast::Expr::Call(expr) => {
            let func = tracker_visit_expr(&expr.expr, state, types);
            let (t, errs) = parse_as_function_call(expr, state, types, &func);
            state.errors.extend(errs);
            t
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
        ast::Expr::Kwargs(_) => Type::Unknown,
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
