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
            true_bindings
                .into_iter()
                .for_each(|(k, v)| state.add_variable(k.as_str(), v));
            stmt.true_body.iter().for_each(|x| track_walk(x, state));
            state.start_else_branch();
            false_bindings
                .into_iter()
                .for_each(|(k, v)| state.add_variable(k.as_str(), v));
            stmt.false_body.iter().for_each(|x| track_walk(x, state));
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
            _ => vec![],
        },
        _ => vec![],
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
