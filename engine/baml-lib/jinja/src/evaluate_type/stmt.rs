use baml_types::LiteralValue;
use minijinja::machinery::ast::{self, Spanned, Stmt, UnaryOpKind};

use super::{expr::evaluate_type, types::PredefinedTypes, TypeError};
use crate::evaluate_type::types::Type;

fn track_walk(node: &ast::Stmt<'_>, state: &mut PredefinedTypes) {
    match node {
        ast::Stmt::Template(stmt) => {
            state.start_scope();
            stmt.children.iter().for_each(|x| track_walk(x, state));
            state.end_scope();
        }
        ast::Stmt::EmitExpr(expr) => {
            let expr_type = evaluate_type(&expr.expr, state);
            if expr_type.is_err() {
                state.errors_mut().extend(expr_type.err().unwrap());
            }
        }
        ast::Stmt::EmitRaw(_) => {}
        ast::Stmt::ForLoop(stmt) => {
            let iter_type = match evaluate_type(&stmt.iter, state) {
                Ok(t) => match t {
                    Type::List(t) => *t,
                    Type::Map(k, _) => *k,
                    _ => Type::Unknown,
                },
                Err(e) => {
                    state.errors_mut().extend(e);
                    Type::Unknown
                }
            };

            let _filter_type = stmt.filter_expr.as_ref().map(|x| evaluate_type(x, state));

            state.start_scope();
            match &stmt.target {
                ast::Expr::Var(var) => state.add_variable(var.id, iter_type),
                ast::Expr::List(list) => match iter_type {
                    Type::List(t) => {
                        list.items.iter().for_each(|x| {
                            if let ast::Expr::Var(var) = x {
                                state.add_variable(var.id, *t.clone());
                            }
                        });
                    }
                    Type::Tuple(items) => {
                        if list.items.len() != items.len() {
                            state.errors_mut().push(TypeError {
                                message: format!("Expected {} items", items.len()),
                                span: list.span(),
                            });
                            list.items.iter().for_each(|x| {
                                if let ast::Expr::Var(var) = x {
                                    state.add_variable(var.id, Type::Unknown);
                                }
                            });
                        } else {
                            list.items.iter().zip(items.iter()).for_each(|(x, t)| {
                                if let ast::Expr::Var(var) = x {
                                    state.add_variable(var.id, t.clone());
                                } else {
                                    state.errors_mut().push(TypeError {
                                        message: "Expected variable".to_string(),
                                        span: list.span(),
                                    });
                                }
                            });
                        }
                    }
                    _ => {}
                },
                _ => {
                    state.errors_mut().push(TypeError {
                        message: "Not a sequence".to_string(),
                        span: stmt.span(),
                    });
                }
            }

            // We need to set some variables here

            state.start_scope();
            state.add_variable("loop", Type::ClassRef("jinja::loop".into()));
            stmt.body.iter().for_each(|x| track_walk(x, state));
            state.end_scope();
            state.start_scope();
            stmt.else_body.iter().for_each(|x| track_walk(x, state));
            state.end_scope();
            state.end_scope();
        }
        ast::Stmt::IfCond(stmt) => {
            match evaluate_type(&stmt.expr, state) {
                Ok(_expr_type) => {}
                Err(e) => {
                    state.errors_mut().extend(e);
                }
            }

            let true_bindings = predicate_implications(&stmt.expr, state, true);
            let false_bindings = predicate_implications(&stmt.expr, state, false);

            // Record variables in each branch and their types (fuse them if they are the same)
            state.start_branch();

            // Use a narrowing scope for type guards in the true branch.
            // This ensures narrowed types are visible in the branch body but don't
            // participate in branch merging (they should revert after the branch).
            state.start_narrowing_scope();
            true_bindings
                .into_iter()
                .for_each(|(k, v)| state.add_narrowing(k.as_str(), v));
            stmt.true_body.iter().for_each(|x| track_walk(x, state));
            state.end_narrowing_scope();

            state.start_else_branch();

            // Same for the false/else branch
            state.start_narrowing_scope();
            false_bindings
                .into_iter()
                .for_each(|(k, v)| state.add_narrowing(k.as_str(), v));
            stmt.false_body.iter().for_each(|x| track_walk(x, state));
            state.end_narrowing_scope();

            state.resolve_branch();
        }
        ast::Stmt::WithBlock(_) => todo!(),
        ast::Stmt::Set(stmt) => {
            let expr_type = match evaluate_type(&stmt.expr, state) {
                Ok(expr_type) => expr_type,
                Err(e) => {
                    state.errors_mut().extend(e);
                    Type::Unknown
                }
            };

            if let ast::Expr::Var(var) = &stmt.target {
                state.add_variable(var.id, expr_type)
            }
        }
        ast::Stmt::SetBlock(stmt) => {
            let _target_type = evaluate_type(&stmt.target, state);
            let _filter_type = stmt.filter.as_ref().map(|x| evaluate_type(x, state));
            stmt.body.iter().for_each(|x| track_walk(x, state));
        }
        ast::Stmt::AutoEscape(_) => todo!(),
        ast::Stmt::FilterBlock(_) => todo!(),
        ast::Stmt::Macro(_stmt) => {}
        ast::Stmt::CallBlock(_) => todo!(),
        ast::Stmt::Do(_) => todo!(),
    }
}

pub fn get_variable_types(stmt: &Stmt, state: &mut PredefinedTypes) -> Vec<TypeError> {
    track_walk(stmt, state);
    state.errors().to_vec()
}

/// For a given predicate, find all the implications on the contained types if
/// truthyness of the predicate is equal to the branch parameter.
///
/// For example, in the context where `a: Number | null`, the expr `a` implies
/// `a: Number`.
/// So `predicate_implications(Var("a"), true)` should return `[("a", Number)]`.
/// `predicate_implications(Var("!a"), false)` should
/// return `[("a", Number)]`, because if `!a` is false,
/// then `a` is true.
///
/// More complex examples (all assuming `branch: true`):
///
/// Γ: { a: Number | null, b: Number | null }
/// (a && b) -> [(a: Number), (b: Number)]
///
/// Γ: { a: Number | null }
/// (!!!!a) -> [(a: Number)]
///
/// Γ: { a: Number | null }
/// (a && true) -> [(a: Number)]
///
/// Γ: { a: Number | null }
/// (a && false) -> []
///
/// Γ: { a: Number | null }
/// (!!!a) -> []
pub fn predicate_implications<'a>(
    expr: &'a ast::Expr<'a>,
    context: &'a mut PredefinedTypes,
    branch: bool,
) -> Vec<(String, Type)> {
    use ast::Expr::*;
    match expr {
        Var(var_name) => context
            .resolve(var_name.id)
            .and_then(|var_type| truthy(&var_type))
            .map_or(vec![], |truthy_type| {
                if branch {
                    vec![(var_name.id.to_string(), truthy_type)]
                } else {
                    vec![(var_name.id.to_string(), Type::None)]
                }
            }),
        UnaryOp(unary_op) => {
            let next_branch = match unary_op.op {
                UnaryOpKind::Not => !branch,
                UnaryOpKind::Neg => branch,
            };
            predicate_implications(&unary_op.expr, context, next_branch)
        }
        BinOp(binary_op) => match binary_op.op {
            ast::BinOpKind::ScAnd => {
                let mut left_implications =
                    predicate_implications(&binary_op.left, context, branch);
                let right_implications = predicate_implications(&binary_op.right, context, branch);
                left_implications.extend(right_implications);
                left_implications
            }

            ast::BinOpKind::ScOr => {
                if branch {
                    // For `A or B` being TRUE: at least one is true
                    // We need to union the narrowed types for each variable
                    let left_implications = predicate_implications(&binary_op.left, context, true);
                    let right_implications =
                        predicate_implications(&binary_op.right, context, true);

                    // Merge implications by variable name, creating unions where needed
                    let mut merged: std::collections::HashMap<String, Type> =
                        std::collections::HashMap::new();

                    for (var_name, var_type) in left_implications {
                        merged.insert(var_name, var_type);
                    }

                    for (var_name, var_type) in right_implications {
                        merged
                            .entry(var_name)
                            .and_modify(|existing| {
                                *existing = Type::merge([existing.clone(), var_type.clone()]);
                            })
                            .or_insert(var_type);
                    }

                    merged.into_iter().collect()
                } else {
                    // For `A or B` being FALSE: both must be false
                    // This is like AND on the false branches
                    let mut left_implications =
                        predicate_implications(&binary_op.left, context, false);
                    let right_implications =
                        predicate_implications(&binary_op.right, context, false);
                    left_implications.extend(right_implications);
                    left_implications
                }
            }

            ast::BinOpKind::Ne => {
                let maybe_non_null_variable = match (&binary_op.left, &binary_op.right) {
                    (Var { .. }, Const(n)) if fuzzy_null(n) => Some(&binary_op.left),
                    (Const(n), Var { .. }) if fuzzy_null(n) => Some(&binary_op.right),
                    _ => None,
                };
                if let Some(non_null_variable) = maybe_non_null_variable {
                    predicate_implications(non_null_variable, context, branch)
                } else {
                    vec![]
                }
            }
            // Narrow union attr access in the form of `if var.kind == "literal"`
            ast::BinOpKind::Eq => {
                // If statement is false then we don't care.
                if !branch {
                    return vec![];
                }

                // Must be exactly `var_name == "const literal"`
                let (get_attr, const_expr) = match (&binary_op.left, &binary_op.right) {
                    (GetAttr(get_attr), Const(const_expr))
                    | (Const(const_expr), GetAttr(get_attr)) => (get_attr, const_expr),

                    _ => return vec![],
                };

                // Nothing to narrow if it's not a var.
                let Var(var) = &get_attr.expr else {
                    return vec![];
                };

                narrow_attr_access_on_union_var(var, get_attr, const_expr, context)
            }
            _ => vec![],
        },
        _ => vec![],
    }
}

/// Narrows the type of a variable based on the value of a const expression.
///
/// Used for these cases:
///
/// ```ignore
/// class UserMessage {
///     kind "user_message"
///     user_message String
/// }
///
/// class AssistantMessage {
///     kind "assistant_message"
///     assistant_message String
/// }
///
/// type Message = UserMessage | AssistantMessage
///
/// {% if message.kind == "user_message" %}
///     {{ message.user_message }}
/// {% elif message.kind == "assistant_message" %}
///     {{ message.assistant_message }}
/// {% endif %}
/// ```
///
/// TODO: This function is very similar to `typecheck_attr_access_on_union` in
/// `expr.rs`. Reusing the code is not straightforward though (at least if we
/// want it to be readable), but we should try something because this is kind of
/// error prone if we add more types that need to be covered.
fn narrow_attr_access_on_union_var(
    var: &Spanned<ast::Var<'_>>,
    get_attr: &Spanned<ast::GetAttr<'_>>,
    const_expr: &Spanned<ast::Const>,
    context: &mut PredefinedTypes,
) -> Vec<(String, Type)> {
    let Some(var_type) = context.resolve(var.id) else {
        return vec![];
    };

    let union_items = match &var_type {
        Type::Union(items) => items,
        Type::Alias { resolved, .. } => match resolved.as_ref() {
            Type::Union(items) => items,
            _ => return vec![],
        },
        _ => return vec![],
    };

    let mut implications = vec![];
    let mut attr_type = None;

    let mut stack = Vec::from_iter(union_items.iter());

    while let Some(union_item_type) = stack.pop() {
        match union_item_type {
            Type::ClassRef(class_name) => {
                let (prop_type, err) = context.check_class_property(
                    &crate::evaluate_type::pretty_print::pretty_print(&get_attr.expr),
                    class_name,
                    get_attr.name,
                    get_attr.span(),
                );

                if err.is_some() {
                    return vec![];
                }

                match &attr_type {
                    None => attr_type = Some(prop_type.clone()),

                    Some(known_type) => {
                        if !prop_type.equals_ignoring_literal_values(known_type) {
                            return vec![];
                        }
                    }
                }

                match prop_type {
                    Type::Literal(LiteralValue::String(literal_string)) => {
                        if let Some(value) = const_expr.value.as_str() {
                            if value == literal_string {
                                implications.push((var.id.to_string(), union_item_type.clone()));
                            }
                        }
                    }
                    Type::Literal(LiteralValue::Int(literal_int)) => {
                        if let Some(value) = const_expr.value.as_i64() {
                            if value == literal_int {
                                implications.push((var.id.to_string(), union_item_type.clone()));
                            }
                        }
                    }
                    // TODO: Jinja works with truthy | falsy, we can't check if
                    // this is literal true or false?
                    Type::Literal(LiteralValue::Bool(_)) => {
                        return vec![];
                    }

                    // TODO: Can't narrow against other types, there are no
                    // literal values to check. Maybe we could for enums?
                    _ => return vec![],
                }
            }

            // Recurse.
            Type::Union(nested) => stack.extend(nested.iter()),

            // Resolve aliases.
            Type::Alias { resolved, .. } => stack.push(resolved),

            _ => {
                return vec![];
            }
        }
    }

    // Finding exactly one match means it's safe to infer the type. More than
    // one is ambiguous.
    if implications.len() == 1 {
        implications
    } else {
        vec![]
    }
}

/// Whether an identifier is `None` or `none`.
fn fuzzy_null(t: &Spanned<ast::Const>) -> bool {
    t.value.to_string().as_str().to_lowercase() == "none"
}

/// Type-narrowing by truthiness. The truthy version of a value's
/// type is a new type that would be implied by the value being truthy.
/// For example, `truthy( Number | null )` is `Number`, because if some
/// value `a: Number | null` is truthy, we can conclude that `a: Number`.
///
/// Some types like `Number` offer no additional information if they
/// are truthy - in these cases we return None.
pub fn truthy(ty: &Type) -> Option<Type> {
    match ty {
        Type::Unknown => None,
        Type::Undefined => None,
        Type::None => None,
        Type::Int => None,
        Type::Float => None,
        Type::Number => None,
        Type::String => None,
        Type::Bool => None,
        Type::Literal(_) => None,
        Type::List(_) => None,
        Type::Map(_, _) => None,
        Type::Tuple(_) => None,
        Type::Union(variants) => {
            let truthy_variants: Vec<Type> = variants
                .iter()
                .filter(|variant| !NULLISH.contains(variant))
                .cloned()
                .collect();
            match truthy_variants.len() {
                0 => None,
                1 => Some(truthy_variants[0].clone()),
                _ => Some(Type::Union(truthy_variants)),
            }
        }
        Type::Both(x, y) => match (truthy(x), truthy(y)) {
            (None, None) => None,
            (Some(truthy_x), None) => Some(truthy_x),
            (None, Some(truthy_y)) => Some(truthy_y),
            (Some(truthy_x), Some(truthy_y)) => {
                Some(Type::Both(Box::new(truthy_x), Box::new(truthy_y)))
            }
        },
        Type::ClassRef(_) => None,
        Type::EnumTypeRef(_) => None,
        Type::EnumValueRef(_) => None,
        Type::FunctionRef(_) => None,
        Type::Alias { resolved, .. } => truthy(resolved),
        Type::RecursiveTypeAlias(_) => None,
        Type::Image => None,
        Type::Audio => None,
    }
}

const NULLISH: [Type; 2] = [Type::Undefined, Type::None];

#[cfg(test)]
mod tests {
    use ast::{Expr, Spanned, Var};
    use minijinja::machinery::Span;

    use super::*;
    use crate::JinjaContext;

    #[test]
    fn truthy_union() {
        let input = Type::Union(vec![Type::ClassRef("Foo".to_string()), Type::Undefined]);
        let expected = Type::ClassRef("Foo".to_string());
        assert_eq!(truthy(&input).unwrap(), expected);
    }

    #[test]
    fn truthy_union_2() {
        let input = Type::Union(vec![Type::Undefined, Type::ClassRef("Foo".to_string())]);
        let expected = Type::ClassRef("Foo".to_string());
        assert_eq!(truthy(&input).unwrap(), expected);
    }

    #[test]
    fn implication_from_nullable() {
        let mut context = PredefinedTypes::default(JinjaContext::Prompt);
        context.add_variable("a", Type::Union(vec![Type::Int, Type::None]));
        let expr = Expr::Var(Spanned::new(Var { id: "a" }, Span::default()));
        let new_vars = predicate_implications(&expr, &mut context, true);
        match new_vars.as_slice() {
            [(name, Type::Int)] => {
                assert_eq!(name.as_str(), "a");
            }
            _ => panic!("Expected singleton list with Type::Int"),
        }
    }
}
