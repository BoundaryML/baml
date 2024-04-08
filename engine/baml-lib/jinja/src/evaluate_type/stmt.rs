use minijinja::machinery::ast::{self, Stmt};

use crate::evaluate_type::types::Type;

use super::{expr::evaluate_type, types::PredefinedTypes, TypeError};

fn track_walk<'a>(node: &ast::Stmt<'a>, state: &mut PredefinedTypes) {
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
            let iter_type = evaluate_type(&stmt.iter, state);
            let iter_type = if iter_type.is_err() {
                state.errors_mut().extend(iter_type.err().unwrap());
                Type::Unknown
            } else {
                match iter_type.unwrap() {
                    Type::List(t) => *t,
                    Type::Map(k, _) => *k,
                    _ => Type::Unknown,
                }
            };

            let filter_type = stmt.filter_expr.as_ref().map(|x| evaluate_type(x, state));

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
            let expr_type = evaluate_type(&stmt.expr, state);

            // Record variables in each branch and their types (fuse them if they are the same)
            state.start_branch();
            stmt.true_body.iter().for_each(|x| track_walk(x, state));
            state.start_else_branch();
            stmt.false_body.iter().for_each(|x| track_walk(x, state));
            state.resolve_branch();
        }
        ast::Stmt::WithBlock(_) => todo!(),
        ast::Stmt::Set(stmt) => {
            let expr_type = evaluate_type(&stmt.expr, state);

            let expr_type = if expr_type.is_err() {
                state.errors_mut().extend(expr_type.err().unwrap());
                Type::Unknown
            } else {
                expr_type.unwrap()
            };

            match &stmt.target {
                ast::Expr::Var(var) => state.add_variable(var.id, expr_type),
                _ => {}
            }
        }
        ast::Stmt::SetBlock(stmt) => {
            let target_type = evaluate_type(&stmt.target, state);
            let filter_type = stmt.filter.as_ref().map(|x| evaluate_type(x, state));
            stmt.body.iter().for_each(|x| track_walk(x, state));
        }
        ast::Stmt::AutoEscape(_) => todo!(),
        ast::Stmt::FilterBlock(_) => todo!(),
        ast::Stmt::Macro(stmt) => {}
        ast::Stmt::CallBlock(_) => todo!(),
        ast::Stmt::Do(_) => todo!(),
    }
}

pub fn get_variable_types<'a>(stmt: &Stmt, state: &mut PredefinedTypes) -> Vec<TypeError> {
    track_walk(stmt, state);
    state.errors().to_vec()
}
