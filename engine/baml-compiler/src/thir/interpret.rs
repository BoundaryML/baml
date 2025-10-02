use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, bail, Context, Result};
use baml_types::{BamlMap, BamlValue, BamlValueWithMeta};
use internal_baml_diagnostics::Span;

use crate::thir::{Block, ClassConstructorField, Expr, ExprMetadata, Statement, THir};

// TODO:
//  - Variables should be expressions, not BamlValues. Because we want to be able to
//    mutate them across REPL prompts and see the same downstream effects on their
//    containers that we would for mutating values within functions.

/// A scope is a map of variable names to their values.
///
/// Variables are stored in refcells to allow for mutation.
pub struct Scope {
    pub variables: BamlMap<String, Arc<Mutex<BamlValueWithMeta<ExprMetadata>>>>,
}

enum EvalValue {
    Value(BamlValueWithMeta<ExprMetadata>),
    Reference(Arc<Mutex<BamlValueWithMeta<ExprMetadata>>>),
    Function(usize, Arc<Block<ExprMetadata>>, ExprMetadata),
}

#[derive(Debug)]
enum ControlFlow {
    Normal(BamlValueWithMeta<ExprMetadata>),
    Break,
    Continue,
    Return(BamlValueWithMeta<ExprMetadata>),
}

pub async fn interpret_thir<F, Fut>(
    thir: THir<ExprMetadata>,
    expr: Expr<ExprMetadata>,
    mut run_llm_function: F,
    extra_bindings: BamlMap<String, BamlValueWithMeta<ExprMetadata>>,
    env_vars: HashMap<String, String>,
) -> Result<BamlValueWithMeta<ExprMetadata>>
where
    F: FnMut(String, Vec<BamlValue>) -> Fut + Send + Sync,
    Fut: Future<Output = Result<BamlValueWithMeta<ExprMetadata>>> + Send,
{
    let env_vars_map = env_vars;
    let mut scopes = vec![Scope {
        variables: BamlMap::from_iter(
            extra_bindings
                .into_iter()
                .map(|(k, v)| (k, Arc::new(Mutex::new(v)))),
        ),
    }];

    let mut env_entries = BamlMap::new();
    for (key, value) in env_vars_map {
        env_entries.insert(
            key,
            BamlValueWithMeta::String(value, (internal_baml_diagnostics::Span::fake(), None)),
        );
    }
    scopes[0].variables.insert(
        "__env_vars__".to_string(),
        Arc::new(Mutex::new(BamlValueWithMeta::Map(
            env_entries,
            (Span::fake(), None),
        ))),
    );

    // Seed scope with global assignments
    for (name, g) in thir.global_assignments.iter() {
        let v =
            expect_value(evaluate_expr(&g.expr, &mut scopes, &thir, &mut run_llm_function).await?)?;
        declare(&mut scopes, name, v);
    }

    // Evaluate provided expression
    let result =
        expect_value(evaluate_expr(&expr, &mut scopes, &thir, &mut run_llm_function).await?)?;
    Ok(result)
}

fn evaluate_block_with_control_flow<'a, F, Fut>(
    block: &'a Block<ExprMetadata>,
    scopes: &'a mut Vec<Scope>,
    thir: &'a THir<ExprMetadata>,
    run_llm_function: &'a mut F,
) -> std::pin::Pin<Box<dyn Future<Output = Result<ControlFlow>> + Send + 'a>>
where
    F: FnMut(String, Vec<BamlValue>) -> Fut + Send + Sync,
    Fut: Future<Output = Result<BamlValueWithMeta<ExprMetadata>>> + Send,
{
    Box::pin(async move {
        scopes.push(Scope {
            variables: BamlMap::new(),
        });

        // Check if we should treat the last statement as the implicit return value
        let use_last_expr_as_return = block.trailing_expr.is_none()
            && matches!(block.statements.last(), Some(Statement::Expression { .. }));

        let statements_to_execute = if use_last_expr_as_return {
            block.statements.len().saturating_sub(1)
        } else {
            block.statements.len()
        };

        for stmt in block.statements.iter().take(statements_to_execute) {
            match stmt {
                Statement::Let { name, value, .. } => {
                    match evaluate_expr(value, scopes, thir, run_llm_function).await? {
                        EvalValue::Value(v) => {
                            declare(scopes, name, v);
                        }
                        EvalValue::Reference(cell) => {
                            declare_with_cell(scopes, name, cell);
                        }
                        EvalValue::Function(_, _, _) => {
                            bail!("cannot assign function to variable `{}`", name);
                        }
                    }
                }
                Statement::Declare { name, span } => {
                    declare(scopes, name, BamlValueWithMeta::Null((span.clone(), None)));
                }
                Statement::Assign { left, value } => {
                    let assigned_value =
                        expect_value(evaluate_expr(value, scopes, thir, run_llm_function).await?)?;
                    assign_to_expr(left, assigned_value, scopes, thir, run_llm_function).await?;
                }
                Statement::DeclareAndAssign { name, value, .. } => {
                    match evaluate_expr(value, scopes, thir, run_llm_function).await? {
                        EvalValue::Value(v) => {
                            declare(scopes, name, v);
                        }
                        EvalValue::Reference(cell) => {
                            declare_with_cell(scopes, name, cell);
                        }
                        EvalValue::Function(_, _, _) => {
                            bail!("cannot assign function to variable `{}`", name);
                        }
                    }
                }
                Statement::Return { expr, .. } => {
                    let v =
                        expect_value(evaluate_expr(expr, scopes, thir, run_llm_function).await?)?;
                    scopes.pop();
                    return Ok(ControlFlow::Return(v));
                }
                Statement::Expression { expr, .. } => {
                    // For expression statements, we still need to evaluate them for side effects
                    // (and the last one might be the implicit return value)
                    let _ = evaluate_expr(expr, scopes, thir, run_llm_function).await?;
                }
                Statement::Break(_) => {
                    scopes.pop();
                    return Ok(ControlFlow::Break);
                }
                Statement::Continue(_) => {
                    scopes.pop();
                    return Ok(ControlFlow::Continue);
                }
                Statement::While {
                    condition, block, ..
                } => loop {
                    let cond_val = expect_value(
                        evaluate_expr(condition, scopes, thir, run_llm_function).await?,
                    )?;
                    match cond_val {
                        BamlValueWithMeta::Bool(true, _) => match evaluate_block_with_control_flow(
                            block,
                            scopes,
                            thir,
                            run_llm_function,
                        )
                        .await?
                        {
                            ControlFlow::Break => break,
                            ControlFlow::Continue => continue,
                            ControlFlow::Normal(_) => {}
                            ControlFlow::Return(val) => {
                                scopes.pop();
                                return Ok(ControlFlow::Return(val));
                            }
                        },
                        BamlValueWithMeta::Bool(false, _) => break,
                        _ => bail!("while condition must be boolean"),
                    }
                },
                Statement::ForLoop {
                    identifier,
                    iterator,
                    block,
                    ..
                } => {
                    let iterable_val = expect_value(
                        evaluate_expr(iterator, scopes, thir, run_llm_function).await?,
                    )?;
                    match iterable_val {
                        BamlValueWithMeta::List(items, _) => {
                            for item_val in items.iter() {
                                // Create new scope for loop iteration
                                scopes.push(Scope {
                                    variables: BamlMap::new(),
                                });
                                declare(scopes, identifier, item_val.clone());

                                match evaluate_block_with_control_flow(
                                    block,
                                    scopes,
                                    thir,
                                    run_llm_function,
                                )
                                .await?
                                {
                                    ControlFlow::Break => {
                                        scopes.pop();
                                        break;
                                    }
                                    ControlFlow::Continue => {
                                        scopes.pop();
                                        continue;
                                    }
                                    ControlFlow::Normal(_) => {
                                        scopes.pop();
                                    }
                                    ControlFlow::Return(val) => {
                                        scopes.pop();
                                        scopes.pop();
                                        return Ok(ControlFlow::Return(val));
                                    }
                                }
                            }
                        }
                        _ => bail!("for loop requires iterable (list)"),
                    }
                }
                Statement::AssignOp {
                    left,
                    value,
                    assign_op,
                    ..
                } => {
                    use crate::hir::AssignOp;

                    let current_val =
                        expect_value(evaluate_expr(left, scopes, thir, run_llm_function).await?)?;

                    // Evaluate the right-hand side expression
                    let rhs_val =
                        expect_value(evaluate_expr(value, scopes, thir, run_llm_function).await?)?;

                    // Perform the compound assignment operation
                    let result_val = match assign_op {
                        AssignOp::AddAssign => match (current_val.clone(), rhs_val.clone()) {
                            (BamlValueWithMeta::Int(a, meta), BamlValueWithMeta::Int(b, _)) => {
                                BamlValueWithMeta::Int(a + b, meta)
                            }
                            (BamlValueWithMeta::Float(a, meta), BamlValueWithMeta::Float(b, _)) => {
                                BamlValueWithMeta::Float(a + b, meta)
                            }
                            (BamlValueWithMeta::Int(a, meta), BamlValueWithMeta::Float(b, _)) => {
                                BamlValueWithMeta::Float(a as f64 + b, meta)
                            }
                            (BamlValueWithMeta::Float(a, meta), BamlValueWithMeta::Int(b, _)) => {
                                BamlValueWithMeta::Float(a + (b as f64), meta)
                            }
                            (
                                BamlValueWithMeta::String(a, meta),
                                BamlValueWithMeta::String(b, _),
                            ) => BamlValueWithMeta::String(format!("{a}{b}"), meta),
                            _ => bail!("unsupported types for += operator"),
                        },
                        AssignOp::SubAssign => match (current_val.clone(), rhs_val.clone()) {
                            (BamlValueWithMeta::Int(a, meta), BamlValueWithMeta::Int(b, _)) => {
                                BamlValueWithMeta::Int(a - b, meta)
                            }
                            (BamlValueWithMeta::Float(a, meta), BamlValueWithMeta::Float(b, _)) => {
                                BamlValueWithMeta::Float(a - b, meta)
                            }
                            (BamlValueWithMeta::Int(a, meta), BamlValueWithMeta::Float(b, _)) => {
                                BamlValueWithMeta::Float((a as f64) - b, meta)
                            }
                            (BamlValueWithMeta::Float(a, meta), BamlValueWithMeta::Int(b, _)) => {
                                BamlValueWithMeta::Float(a - (b as f64), meta)
                            }
                            _ => bail!("unsupported types for -= operator"),
                        },
                        AssignOp::MulAssign => match (current_val.clone(), rhs_val.clone()) {
                            (BamlValueWithMeta::Int(a, meta), BamlValueWithMeta::Int(b, _)) => {
                                BamlValueWithMeta::Int(a * b, meta)
                            }
                            (BamlValueWithMeta::Float(a, meta), BamlValueWithMeta::Float(b, _)) => {
                                BamlValueWithMeta::Float(a * b, meta)
                            }
                            (BamlValueWithMeta::Int(a, meta), BamlValueWithMeta::Float(b, _)) => {
                                BamlValueWithMeta::Float((a as f64) * b, meta)
                            }
                            (BamlValueWithMeta::Float(a, meta), BamlValueWithMeta::Int(b, _)) => {
                                BamlValueWithMeta::Float(a * (b as f64), meta)
                            }
                            _ => bail!("unsupported types for *= operator"),
                        },
                        AssignOp::DivAssign => match (current_val.clone(), rhs_val.clone()) {
                            (BamlValueWithMeta::Int(a, meta), BamlValueWithMeta::Int(b, _)) => {
                                if b == 0 {
                                    bail!("division by zero in /= operator");
                                }
                                BamlValueWithMeta::Float((a as f64) / (b as f64), meta)
                            }
                            (BamlValueWithMeta::Float(a, meta), BamlValueWithMeta::Float(b, _)) => {
                                if b == 0.0 {
                                    bail!("division by zero in /= operator");
                                }
                                BamlValueWithMeta::Float(a / b, meta)
                            }
                            (BamlValueWithMeta::Int(a, meta), BamlValueWithMeta::Float(b, _)) => {
                                if b == 0.0 {
                                    bail!("division by zero in /= operator");
                                }
                                BamlValueWithMeta::Float((a as f64) / b, meta)
                            }
                            (BamlValueWithMeta::Float(a, meta), BamlValueWithMeta::Int(b, _)) => {
                                if b == 0 {
                                    bail!("division by zero in /= operator");
                                }
                                BamlValueWithMeta::Float(a / (b as f64), meta)
                            }
                            _ => bail!("unsupported types for /= operator"),
                        },
                        AssignOp::ModAssign => match (current_val.clone(), rhs_val.clone()) {
                            (BamlValueWithMeta::Int(a, meta), BamlValueWithMeta::Int(b, _)) => {
                                if b == 0 {
                                    bail!("modulo by zero in %= operator");
                                }
                                BamlValueWithMeta::Int(a % b, meta)
                            }
                            _ => bail!("unsupported types for %= operator"),
                        },
                        AssignOp::BitXorAssign => match (current_val.clone(), rhs_val.clone()) {
                            (BamlValueWithMeta::Int(a, meta), BamlValueWithMeta::Int(b, _)) => {
                                BamlValueWithMeta::Int(a ^ b, meta)
                            }
                            _ => bail!("bitwise ^= requires integer operands"),
                        },
                        AssignOp::BitAndAssign => match (current_val.clone(), rhs_val.clone()) {
                            (BamlValueWithMeta::Int(a, meta), BamlValueWithMeta::Int(b, _)) => {
                                BamlValueWithMeta::Int(a & b, meta)
                            }
                            _ => bail!("bitwise &= requires integer operands"),
                        },
                        AssignOp::BitOrAssign => match (current_val.clone(), rhs_val.clone()) {
                            (BamlValueWithMeta::Int(a, meta), BamlValueWithMeta::Int(b, _)) => {
                                BamlValueWithMeta::Int(a | b, meta)
                            }
                            _ => bail!("bitwise |= requires integer operands"),
                        },
                        AssignOp::ShlAssign => match (current_val.clone(), rhs_val.clone()) {
                            (BamlValueWithMeta::Int(a, meta), BamlValueWithMeta::Int(b, _)) => {
                                if b < 0 {
                                    bail!("negative shift amount in <<= operator");
                                }
                                BamlValueWithMeta::Int(a << b, meta)
                            }
                            _ => bail!("shift <<= requires integer operands"),
                        },
                        AssignOp::ShrAssign => match (current_val.clone(), rhs_val.clone()) {
                            (BamlValueWithMeta::Int(a, meta), BamlValueWithMeta::Int(b, _)) => {
                                if b < 0 {
                                    bail!("negative shift amount in >>= operator");
                                }
                                BamlValueWithMeta::Int(a >> b, meta)
                            }
                            _ => bail!("shift >>= requires integer operands"),
                        },
                    };

                    // Assign the result back to the target expression
                    assign_to_expr(left, result_val, scopes, thir, run_llm_function).await?;
                }
                Statement::SemicolonExpression { expr, .. } => {
                    let _ = evaluate_expr(expr, scopes, thir, run_llm_function).await?;
                }
                Statement::CForLoop {
                    condition,
                    after,
                    block,
                } => {
                    loop {
                        // Check condition (if present)
                        if let Some(cond_expr) = condition {
                            let cond_val = expect_value(
                                evaluate_expr(cond_expr, scopes, thir, run_llm_function).await?,
                            )?;
                            match cond_val {
                                BamlValueWithMeta::Bool(false, _) => break,
                                BamlValueWithMeta::Bool(true, _) => {}
                                _ => bail!("C-style for loop condition must be boolean"),
                            }
                        }

                        // Execute loop body
                        match evaluate_block_with_control_flow(
                            block,
                            scopes,
                            thir,
                            run_llm_function,
                        )
                        .await?
                        {
                            ControlFlow::Break => break,
                            ControlFlow::Continue => {
                                // Execute after statement if present
                                if let Some(after_stmt) = after {
                                    // Execute the after statement in the current scope context
                                    match after_stmt.as_ref() {
                                        Statement::AssignOp {
                                            left,
                                            value,
                                            assign_op,
                                            ..
                                        } => {
                                            use crate::hir::AssignOp;

                                            let current_val = expect_value(
                                                evaluate_expr(left, scopes, thir, run_llm_function)
                                                    .await?,
                                            )?;
                                            let rhs_val = expect_value(
                                                evaluate_expr(
                                                    value,
                                                    scopes,
                                                    thir,
                                                    run_llm_function,
                                                )
                                                .await?,
                                            )?;

                                            let result_val = match assign_op {
                                                AssignOp::AddAssign => {
                                                    match (current_val.clone(), rhs_val.clone()) {
                                                        (
                                                            BamlValueWithMeta::Int(a, meta),
                                                            BamlValueWithMeta::Int(b, _),
                                                        ) => BamlValueWithMeta::Int(a + b, meta),
                                                        _ => bail!(
                                                            "unsupported types for += in C-for after clause"
                                                        ),
                                                    }
                                                }
                                                _ => bail!(
                                                    "unsupported assign op in C-for after clause"
                                                ),
                                            };
                                            assign_to_expr(
                                                left,
                                                result_val,
                                                scopes,
                                                thir,
                                                run_llm_function,
                                            )
                                            .await?;
                                        }
                                        Statement::Assign { left, value } => {
                                            let v = expect_value(
                                                evaluate_expr(
                                                    value,
                                                    scopes,
                                                    thir,
                                                    run_llm_function,
                                                )
                                                .await?,
                                            )?;
                                            assign_to_expr(left, v, scopes, thir, run_llm_function)
                                                .await?;
                                        }
                                        _ => bail!(
                                            "unsupported statement type in C-for after clause"
                                        ),
                                    }
                                }
                                continue;
                            }
                            ControlFlow::Normal(_) => {
                                // Execute after statement if present
                                if let Some(after_stmt) = after {
                                    // Execute the after statement in the current scope context
                                    match after_stmt.as_ref() {
                                        Statement::AssignOp {
                                            left,
                                            value,
                                            assign_op,
                                            ..
                                        } => {
                                            use crate::hir::AssignOp;

                                            let current_val = expect_value(
                                                evaluate_expr(left, scopes, thir, run_llm_function)
                                                    .await?,
                                            )?;
                                            let rhs_val = expect_value(
                                                evaluate_expr(
                                                    value,
                                                    scopes,
                                                    thir,
                                                    run_llm_function,
                                                )
                                                .await?,
                                            )?;

                                            let result_val = match assign_op {
                                                AssignOp::AddAssign => {
                                                    match (current_val.clone(), rhs_val.clone()) {
                                                        (
                                                            BamlValueWithMeta::Int(a, meta),
                                                            BamlValueWithMeta::Int(b, _),
                                                        ) => BamlValueWithMeta::Int(a + b, meta),
                                                        _ => bail!(
                                                            "unsupported types for += in C-for after clause"
                                                        ),
                                                    }
                                                }
                                                _ => bail!(
                                                    "unsupported assign op in C-for after clause"
                                                ),
                                            };
                                            assign_to_expr(
                                                left,
                                                result_val,
                                                scopes,
                                                thir,
                                                run_llm_function,
                                            )
                                            .await?;
                                        }
                                        Statement::Assign { left, value } => {
                                            let v = expect_value(
                                                evaluate_expr(
                                                    value,
                                                    scopes,
                                                    thir,
                                                    run_llm_function,
                                                )
                                                .await?,
                                            )?;
                                            assign_to_expr(left, v, scopes, thir, run_llm_function)
                                                .await?;
                                        }
                                        _ => bail!(
                                            "unsupported statement type in C-for after clause"
                                        ),
                                    }
                                }
                            }
                            ControlFlow::Return(val) => {
                                scopes.pop();
                                return Ok(ControlFlow::Return(val));
                            }
                        }
                    }
                }
                Statement::Assert { condition, .. } => {
                    let cond_val = expect_value(
                        evaluate_expr(condition, scopes, thir, run_llm_function).await?,
                    )?;
                    match cond_val {
                        BamlValueWithMeta::Bool(true, _) => {}
                        BamlValueWithMeta::Bool(false, _) => bail!("assertion failed"),
                        _ => bail!("assert condition must be boolean"),
                    }
                }
            }
        }

        // Compute the return value
        let ret = if let Some(trailing_expr) = &block.trailing_expr {
            // Explicit trailing expression
            expect_value(evaluate_expr(trailing_expr, scopes, thir, run_llm_function).await?)?
        } else if use_last_expr_as_return {
            // No explicit trailing expression, but last statement is an expression statement,
            // so use that as the implicit return value (handles cases like if-else at the end of a block)
            if let Some(Statement::Expression { expr, .. }) = block.statements.last() {
                expect_value(evaluate_expr(expr, scopes, thir, run_llm_function).await?)?
            } else {
                unreachable!("use_last_expr_as_return is true but last statement is not Expression")
            }
        } else {
            // No trailing expression and last statement is not an expression, return null
            BamlValueWithMeta::Null((internal_baml_diagnostics::Span::fake(), None))
        };
        scopes.pop();
        Ok(ControlFlow::Normal(ret))
    })
}

async fn evaluate_block<F, Fut>(
    block: &Block<ExprMetadata>,
    scopes: &mut Vec<Scope>,
    thir: &THir<ExprMetadata>,
    run_llm_function: &mut F,
) -> Result<BamlValueWithMeta<ExprMetadata>>
where
    F: FnMut(String, Vec<BamlValue>) -> Fut + Send + Sync,
    Fut: Future<Output = Result<BamlValueWithMeta<ExprMetadata>>> + Send,
{
    match evaluate_block_with_control_flow(block, scopes, thir, run_llm_function).await? {
        ControlFlow::Normal(val) => Ok(val),
        ControlFlow::Return(val) => Ok(val),
        ControlFlow::Break => bail!("break statement not in loop context"),
        ControlFlow::Continue => bail!("continue statement not in loop context"),
    }
}

fn declare(scopes: &mut [Scope], name: &str, value: BamlValueWithMeta<ExprMetadata>) {
    if let Some(scope) = scopes.last_mut() {
        scope
            .variables
            .insert(name.to_string(), Arc::new(Mutex::new(value)));
    }
}

fn declare_with_cell(
    scopes: &mut [Scope],
    name: &str,
    cell: Arc<Mutex<BamlValueWithMeta<ExprMetadata>>>,
) {
    if let Some(scope) = scopes.last_mut() {
        scope.variables.insert(name.to_string(), cell);
    }
}

fn assign(scopes: &mut [Scope], name: &str, value: BamlValueWithMeta<ExprMetadata>) -> Result<()> {
    for s in scopes.iter_mut().rev() {
        if let Some(cell) = s.variables.get_mut(name) {
            *cell.lock().unwrap() = value;
            return Ok(());
        }
    }
    bail!("assign to undeclared variable `{}`", name)
}

async fn assign_to_expr<F, Fut>(
    target: &Expr<ExprMetadata>,
    new_value: BamlValueWithMeta<ExprMetadata>,
    scopes: &mut Vec<Scope>,
    thir: &THir<ExprMetadata>,
    run_llm_function: &mut F,
) -> Result<()>
where
    F: FnMut(String, Vec<BamlValue>) -> Fut + Send + Sync,
    Fut: Future<Output = Result<BamlValueWithMeta<ExprMetadata>>> + Send,
{
    let mut current_expr = target;
    let mut value_to_assign = new_value;

    loop {
        match current_expr {
            Expr::Var(name, _) => return assign(scopes, name, value_to_assign),
            Expr::FieldAccess { base, field, .. } => {
                let mut base_value =
                    expect_value(evaluate_expr(base, scopes, thir, run_llm_function).await?)?;

                match &mut base_value {
                    BamlValueWithMeta::Class(_, fields, _) => {
                        let entry = fields.get_mut(field).with_context(|| {
                            format!("field `{}` not found for assignment", field)
                        })?;
                        *entry = value_to_assign.clone();
                    }
                    BamlValueWithMeta::Map(fields, _) => {
                        let entry = fields.get_mut(field).with_context(|| {
                            format!("field `{}` not found for assignment", field)
                        })?;
                        *entry = value_to_assign.clone();
                    }
                    _ => bail!("field assignment on non-map/class"),
                }

                value_to_assign = base_value;
                current_expr = base.as_ref();
            }
            Expr::ArrayAccess { base, index, meta } => {
                let mut base_value =
                    expect_value(evaluate_expr(base, scopes, thir, run_llm_function).await?)?;
                let index_value =
                    expect_value(evaluate_expr(index, scopes, thir, run_llm_function).await?)?;

                let idx = match index_value {
                    BamlValueWithMeta::Int(i, _) if i >= 0 => i as usize,
                    _ => bail!(
                        "array assignment requires a non-negative integer index at {:?}",
                        meta.0
                    ),
                };

                match &mut base_value {
                    BamlValueWithMeta::List(items, _) => {
                        if idx >= items.len() {
                            bail!("array assignment index out of bounds");
                        }
                        items[idx] = value_to_assign.clone();
                    }
                    _ => bail!("array assignment on non-list value at {:?}", meta.0),
                }

                value_to_assign = base_value;
                current_expr = base.as_ref();
            }
            _ => return Err(anyhow!("Complex assignment targets not yet supported")),
        }
    }
}

fn lookup(scopes: &[Scope], name: &str) -> Option<BamlValueWithMeta<ExprMetadata>> {
    for s in scopes.iter().rev() {
        if let Some(cell) = s.variables.get(name) {
            return Some(cell.lock().unwrap().clone());
        }
    }
    None
}

fn lookup_cell(
    scopes: &[Scope],
    name: &str,
) -> Option<Arc<Mutex<BamlValueWithMeta<ExprMetadata>>>> {
    for s in scopes.iter().rev() {
        if let Some(cell) = s.variables.get(name) {
            return Some(cell.clone());
        }
    }
    None
}

/// Convert BamlValueWithMeta to BamlValue by stripping metadata
fn baml_value_with_meta_to_baml_value(value: BamlValueWithMeta<ExprMetadata>) -> BamlValue {
    match value {
        BamlValueWithMeta::String(s, _) => BamlValue::String(s),
        BamlValueWithMeta::Int(i, _) => BamlValue::Int(i),
        BamlValueWithMeta::Float(f, _) => BamlValue::Float(f),
        BamlValueWithMeta::Bool(b, _) => BamlValue::Bool(b),
        BamlValueWithMeta::Map(m, _) => {
            let converted_map = m
                .into_iter()
                .map(|(k, v)| (k, baml_value_with_meta_to_baml_value(v)))
                .collect();
            BamlValue::Map(converted_map)
        }
        BamlValueWithMeta::List(l, _) => {
            let converted_list = l
                .into_iter()
                .map(baml_value_with_meta_to_baml_value)
                .collect();
            BamlValue::List(converted_list)
        }
        BamlValueWithMeta::Media(m, _) => BamlValue::Media(m),
        BamlValueWithMeta::Enum(name, val, _) => BamlValue::Enum(name, val),
        BamlValueWithMeta::Class(name, fields, _) => {
            let converted_fields = fields
                .into_iter()
                .map(|(k, v)| (k, baml_value_with_meta_to_baml_value(v)))
                .collect();
            BamlValue::Class(name, converted_fields)
        }
        BamlValueWithMeta::Null(_) => BamlValue::Null,
    }
}

fn evaluate_expr<'a, F, Fut>(
    expr: &'a Expr<ExprMetadata>,
    scopes: &'a mut Vec<Scope>,
    thir: &'a THir<ExprMetadata>,
    run_llm_function: &'a mut F,
) -> std::pin::Pin<Box<dyn Future<Output = Result<EvalValue>> + Send + 'a>>
where
    F: FnMut(String, Vec<BamlValue>) -> Fut + Send + Sync,
    Fut: Future<Output = Result<BamlValueWithMeta<ExprMetadata>>> + Send,
{
    Box::pin(async move {
        Ok(match expr {
            Expr::Value(v) => EvalValue::Value(v.clone()),
            Expr::List(items, meta) => {
                let mut out = Vec::with_capacity(items.len());
                for it in items.iter() {
                    out.push(expect_value(
                        evaluate_expr(it, scopes, thir, run_llm_function).await?,
                    )?);
                }
                EvalValue::Value(BamlValueWithMeta::List(out, meta.clone()))
            }
            Expr::Map(entries, meta) => {
                let mut out: BamlMap<String, BamlValueWithMeta<ExprMetadata>> = BamlMap::new();
                for (k, v) in entries.iter() {
                    out.insert(
                        k.clone(),
                        expect_value(evaluate_expr(v, scopes, thir, run_llm_function).await?)?,
                    );
                }
                EvalValue::Value(BamlValueWithMeta::Map(out, meta.clone()))
            }
            Expr::Block(block, _meta) => {
                let v = evaluate_block(block, scopes, thir, run_llm_function).await?;
                EvalValue::Value(v)
            }
            Expr::Var(name, meta) => {
                // First check if it's an LLM function
                if let Some(_llm_func) = thir.llm_functions.iter().find(|f| &f.name == name) {
                    // Return a special marker for LLM functions that can be called
                    // We'll handle the actual calling in the Call expression
                    EvalValue::Function(
                        0,
                        Arc::new(Block {
                            env: BamlMap::new(),
                            statements: vec![],
                            trailing_expr: Some(Expr::Value(BamlValueWithMeta::String(
                                format!("__LLM_FUNCTION__{name}"),
                                meta.clone(),
                            ))),
                            ty: None,
                            span: internal_baml_diagnostics::Span::fake(),
                        }),
                        meta.clone(),
                    )
                }
                // Check if it's an expression function
                else if let Some(expr_func) = thir.expr_functions.iter().find(|f| &f.name == name)
                {
                    EvalValue::Function(
                        expr_func.parameters.len(),
                        Arc::new(expr_func.body.clone()),
                        meta.clone(),
                    )
                } else {
                    let cell = lookup_cell(scopes, name)
                        .with_context(|| format!("unbound variable `{}` at {:?}", name, meta.0))?;
                    EvalValue::Reference(cell)
                }
            }
            Expr::Function(arity, body, meta) => {
                EvalValue::Function(*arity, body.clone(), meta.clone())
            }
            Expr::Call {
                func,
                type_args: _,
                args,
                meta: _,
            } => {
                if let Expr::Var(func_name, _) = func.as_ref() {
                    if func_name == "env.get" {
                        if args.len() != 1 {
                            bail!("env.get expects exactly one argument");
                        }

                        let key_val = expect_value(
                            evaluate_expr(&args[0], scopes, thir, run_llm_function).await?,
                        )?;

                        let key = match key_val {
                            BamlValueWithMeta::String(value, _) => value,
                            _ => bail!("env.get argument must be a string"),
                        };

                        let env_map = lookup(scopes, "__env_vars__")
                            .ok_or_else(|| anyhow!("environment context missing"))?;

                        let map = match env_map {
                            BamlValueWithMeta::Map(ref entries, _) => entries,
                            _ => bail!("environment context corrupted"),
                        };

                        if let Some(value) = map.get(&key) {
                            return Ok(EvalValue::Value(value.clone()));
                        } else {
                            bail!("Environment variable '{}' not found", key);
                        }
                    }
                }

                let callee = evaluate_expr(func, scopes, thir, run_llm_function).await?;
                let (arity, body, meta) = match callee {
                    EvalValue::Function(a, b, m) => (a, b, m),
                    _ => bail!("attempted to call non-function"),
                };

                // Check if this is an LLM function call
                if let Some(Expr::Value(BamlValueWithMeta::String(marker, _))) = &body.trailing_expr
                {
                    if marker.starts_with("__LLM_FUNCTION__") {
                        let fn_name = marker.strip_prefix("__LLM_FUNCTION__").unwrap().to_string();

                        // Evaluate arguments and convert to BamlValue
                        let mut llm_args: Vec<BamlValue> = Vec::with_capacity(args.len());
                        for a in args.iter() {
                            let arg_val = expect_value(
                                evaluate_expr(a, scopes, thir, run_llm_function).await?,
                            )?;
                            llm_args.push(baml_value_with_meta_to_baml_value(arg_val));
                        }

                        // Call the LLM function
                        let result = run_llm_function(fn_name, llm_args).await?;
                        return Ok(EvalValue::Value(result));
                    }
                }

                if arity != args.len() {
                    bail!(
                        "arity mismatch: expected {} args, got {}",
                        arity,
                        args.len()
                    );
                }

                // Evaluate arguments first
                let mut arg_vals: Vec<BamlValueWithMeta<ExprMetadata>> =
                    Vec::with_capacity(args.len());
                for a in args.iter() {
                    arg_vals.push(expect_value(
                        evaluate_expr(a, scopes, thir, run_llm_function).await?,
                    )?);
                }

                // Check if this is an expression function call to get parameter names
                let param_names = if let Expr::Var(func_name, _) = func.as_ref() {
                    if let Some(expr_func) =
                        thir.expr_functions.iter().find(|f| &f.name == func_name)
                    {
                        // Use actual parameter names from expression function
                        expr_func
                            .parameters
                            .iter()
                            .map(|p| p.name.clone())
                            .collect::<Vec<_>>()
                    } else {
                        // Use fresh names for anonymous functions
                        let body_expr =
                            Expr::Block(Box::new(Arc::unwrap_or_clone(body.clone())), meta.clone());
                        body_expr.fresh_names(arity)
                    }
                } else {
                    // Use fresh names for complex function expressions
                    let body_expr =
                        Expr::Block(Box::new(Arc::unwrap_or_clone(body.clone())), meta.clone());
                    body_expr.fresh_names(arity)
                };

                // Create a scope binding parameters to their argument values
                scopes.push(Scope {
                    variables: param_names
                        .into_iter()
                        .zip(arg_vals)
                        .map(|(k, v)| (k, Arc::new(Mutex::new(v))))
                        .collect(),
                });

                // Execute the function body
                let result = evaluate_block(&body, scopes, thir, run_llm_function).await?;
                scopes.pop();
                EvalValue::Value(result)
            }
            Expr::If(cond, then, else_, meta) => {
                let cv = expect_value(evaluate_expr(cond, scopes, thir, run_llm_function).await?)?;
                let b = match cv {
                    BamlValueWithMeta::Bool(v, _) => v,
                    _ => bail!("condition not bool at {:?}", meta.0),
                };
                if b {
                    EvalValue::Value(expect_value(
                        evaluate_expr(then, scopes, thir, run_llm_function).await?,
                    )?)
                } else if let Some(e) = else_ {
                    EvalValue::Value(expect_value(
                        evaluate_expr(e, scopes, thir, run_llm_function).await?,
                    )?)
                } else {
                    EvalValue::Value(BamlValueWithMeta::Null(meta.clone()))
                }
            }
            Expr::ArrayAccess { base, index, meta } => {
                let b = expect_value(evaluate_expr(base, scopes, thir, run_llm_function).await?)?;
                let i = expect_value(evaluate_expr(index, scopes, thir, run_llm_function).await?)?;
                let arr = match b.clone() {
                    BamlValueWithMeta::List(v, _) => v,
                    _ => bail!("array access on non-list at {:?}", meta),
                };
                let idx = match i {
                    BamlValueWithMeta::Int(ii, _) => ii as usize,
                    _ => bail!("index not int at {:?}", meta),
                };
                let v = arr.get(idx).cloned().context("index out of bounds")?;
                EvalValue::Value(v.clone())
            }
            Expr::FieldAccess { base, field, meta } => {
                let b = expect_value(evaluate_expr(base, scopes, thir, run_llm_function).await?)?;
                match b.clone() {
                    BamlValueWithMeta::Map(m, _) => {
                        let v = m.get(field).context("missing field")?;
                        EvalValue::Value(v.clone())
                    }
                    BamlValueWithMeta::Class(_, m, _) => {
                        let v = m.get(field).context("missing field")?;
                        EvalValue::Value(v.clone())
                    }
                    _ => bail!("field access on non-map/class at {:?}", meta.0),
                }
            }
            Expr::ClassConstructor { name, fields, meta } => {
                let mut field_map: BamlMap<String, BamlValueWithMeta<ExprMetadata>> =
                    BamlMap::new();

                for field in fields {
                    match field {
                        ClassConstructorField::Named { name, value } => {
                            field_map.insert(
                                name.clone(),
                                expect_value(
                                    evaluate_expr(value, scopes, thir, run_llm_function).await?,
                                )?,
                            );
                        }

                        ClassConstructorField::Spread { value } => {
                            let spread_val = expect_value(
                                evaluate_expr(value, scopes, thir, run_llm_function).await?,
                            )?;
                            match spread_val.clone() {
                                BamlValueWithMeta::Class(_, spread_fields, _) => {
                                    for (k, v) in spread_fields.iter() {
                                        field_map.insert(k.clone(), v.clone());
                                    }
                                }
                                // // TODO: Allow maps to be spread?
                                // BamlValueWithMeta::Map(spread_fields) => {
                                //     for (k, v) in spread_fields.iter() {
                                //         field_map.insert(k.clone(), v.clone());
                                //     }
                                // }
                                _ => bail!(
                                    "spread operator can only be used on classes at {:?}",
                                    meta.0
                                ),
                            }
                        }
                    }
                }

                EvalValue::Value(BamlValueWithMeta::Class(
                    name.clone(),
                    field_map,
                    meta.clone(),
                ))
            }
            Expr::Builtin(builtin, meta) => {
                use crate::thir::Builtin;
                match builtin {
                    Builtin::FetchValue => {
                        // FetchValue requires network access and is not supported in the interpreter
                        bail!(
                            "builtin function std::fetch_value is not supported in interpreter at {:?}",
                            meta.0
                        )
                    }
                }
            }
            Expr::BinaryOperation {
                left,
                operator,
                right,
                meta,
            } => {
                let left_val =
                    expect_value(evaluate_expr(left, scopes, thir, run_llm_function).await?)?;
                let right_val =
                    expect_value(evaluate_expr(right, scopes, thir, run_llm_function).await?)?;

                let result = evaluate_binary_op(operator, &left_val, &right_val, meta)?;
                EvalValue::Value(result)
            }
            Expr::UnaryOperation {
                operator,
                expr,
                meta,
            } => {
                let val = expect_value(evaluate_expr(expr, scopes, thir, run_llm_function).await?)?;

                let result = evaluate_unary_op(operator, &val, meta)?;
                EvalValue::Value(result)
            }
            Expr::MethodCall {
                receiver,
                method,
                args,
                meta,
            } => {
                let receiver_val =
                    expect_value(evaluate_expr(receiver, scopes, thir, run_llm_function).await?)?;

                // Extract method name
                let method_name = match method.as_ref() {
                    Expr::Var(name, _) => name.clone(),
                    _ => bail!("method name must be an identifier at {:?}", meta.0),
                };

                // Evaluate arguments
                let mut arg_vals: Vec<BamlValueWithMeta<ExprMetadata>> =
                    Vec::with_capacity(args.len());
                for arg in args.iter() {
                    arg_vals.push(expect_value(
                        evaluate_expr(arg, scopes, thir, run_llm_function).await?,
                    )?);
                }

                let result = evaluate_method_call(&receiver_val, &method_name, &arg_vals, meta)?;
                EvalValue::Value(result)
            }
            Expr::Paren(inner, _) => evaluate_expr(inner, scopes, thir, run_llm_function).await?,
        })
    })
}

fn expect_value(v: EvalValue) -> Result<BamlValueWithMeta<ExprMetadata>> {
    match v {
        EvalValue::Value(v) => Ok(v),
        EvalValue::Reference(cell) => Ok(cell.lock().unwrap().clone()),
        EvalValue::Function(_, _, _) => bail!("expected value, found function"),
    }
}

fn evaluate_binary_op(
    operator: &crate::hir::BinaryOperator,
    left_val: &BamlValueWithMeta<ExprMetadata>,
    right_val: &BamlValueWithMeta<ExprMetadata>,
    meta: &ExprMetadata,
) -> Result<BamlValueWithMeta<ExprMetadata>> {
    use crate::hir::BinaryOperator;
    Ok(match operator {
        // Arithmetic operations
        BinaryOperator::Add => match (left_val.clone(), right_val.clone()) {
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Int(b, _)) => {
                BamlValueWithMeta::Int(a + b, meta.clone())
            }
            (BamlValueWithMeta::Float(a, _), BamlValueWithMeta::Float(b, _)) => {
                BamlValueWithMeta::Float(a + b, meta.clone())
            }
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Float(b, _)) => {
                BamlValueWithMeta::Float(a as f64 + b, meta.clone())
            }
            (BamlValueWithMeta::Float(a, _), BamlValueWithMeta::Int(b, _)) => {
                BamlValueWithMeta::Float(a + (b as f64), meta.clone())
            }
            (BamlValueWithMeta::String(a, _), BamlValueWithMeta::String(b, _)) => {
                BamlValueWithMeta::String(format!("{a}{b}"), meta.clone())
            }
            _ => bail!("unsupported types for + operator at {:?}", meta.0),
        },
        BinaryOperator::Sub => match (left_val.clone(), right_val.clone()) {
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Int(b, _)) => {
                BamlValueWithMeta::Int(a - b, meta.clone())
            }
            (BamlValueWithMeta::Float(a, _), BamlValueWithMeta::Float(b, _)) => {
                BamlValueWithMeta::Float(a - b, meta.clone())
            }
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Float(b, _)) => {
                BamlValueWithMeta::Float((a as f64) - b, meta.clone())
            }
            (BamlValueWithMeta::Float(a, _), BamlValueWithMeta::Int(b, _)) => {
                BamlValueWithMeta::Float(a - (b as f64), meta.clone())
            }
            _ => bail!("unsupported types for - operator at {:?}", meta.0),
        },
        BinaryOperator::Mul => match (left_val.clone(), right_val.clone()) {
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Int(b, _)) => {
                BamlValueWithMeta::Int(a * b, meta.clone())
            }
            (BamlValueWithMeta::Float(a, _), BamlValueWithMeta::Float(b, _)) => {
                BamlValueWithMeta::Float(a * b, meta.clone())
            }
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Float(b, _)) => {
                BamlValueWithMeta::Float((a as f64) * b, meta.clone())
            }
            (BamlValueWithMeta::Float(a, _), BamlValueWithMeta::Int(b, _)) => {
                BamlValueWithMeta::Float(a * (b as f64), meta.clone())
            }
            _ => bail!("unsupported types for * operator at {:?}", meta.0),
        },
        BinaryOperator::Div => match (left_val.clone(), right_val.clone()) {
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Int(b, _)) => {
                if b == 0 {
                    bail!("division by zero at {:?}", meta.0);
                }
                BamlValueWithMeta::Float((a as f64) / (b as f64), meta.clone())
            }
            (BamlValueWithMeta::Float(a, _), BamlValueWithMeta::Float(b, _)) => {
                if b == 0.0 {
                    bail!("division by zero at {:?}", meta.0);
                }
                BamlValueWithMeta::Float(a / b, meta.clone())
            }
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Float(b, _)) => {
                if b == 0.0 {
                    bail!("division by zero at {:?}", meta.0);
                }
                BamlValueWithMeta::Float((a as f64) / b, meta.clone())
            }
            (BamlValueWithMeta::Float(a, _), BamlValueWithMeta::Int(b, _)) => {
                if b == 0 {
                    bail!("division by zero at {:?}", meta.0);
                }
                BamlValueWithMeta::Float(a / (b as f64), meta.clone())
            }
            _ => bail!("unsupported types for / operator at {:?}", meta.0),
        },
        BinaryOperator::Mod => match (left_val.clone(), right_val.clone()) {
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Int(b, _)) => {
                if b == 0 {
                    bail!("modulo by zero at {:?}", meta.0);
                }
                BamlValueWithMeta::Int(a % b, meta.clone())
            }
            _ => bail!("unsupported types for % operator at {:?}", meta.0),
        },

        // Comparison operations
        BinaryOperator::Eq => {
            let equal = values_equal(&left_val.clone(), &right_val.clone());
            BamlValueWithMeta::Bool(equal, meta.clone())
        }
        BinaryOperator::Neq => {
            let not_equal = !values_equal(&left_val.clone(), &right_val.clone());
            BamlValueWithMeta::Bool(not_equal, meta.clone())
        }
        BinaryOperator::Lt => {
            let ord_opt = compare_values(&left_val.clone(), &right_val.clone())?;
            let less = ord_opt
                .map(|ord| matches!(ord, std::cmp::Ordering::Less))
                .ok_or_else(|| anyhow!("unsupported types for < operator at {:?}", meta.0))?;
            BamlValueWithMeta::Bool(less, meta.clone())
        }
        BinaryOperator::LtEq => {
            let ord_opt = compare_values(&left_val.clone(), &right_val.clone())?;
            let less_eq = ord_opt
                .map(|ord| matches!(ord, std::cmp::Ordering::Less | std::cmp::Ordering::Equal))
                .ok_or_else(|| anyhow!("unsupported types for <= operator at {:?}", meta.0))?;
            BamlValueWithMeta::Bool(less_eq, meta.clone())
        }
        BinaryOperator::Gt => {
            let ord_opt = compare_values(&left_val.clone(), &right_val.clone())?;
            let greater = ord_opt
                .map(|ord| matches!(ord, std::cmp::Ordering::Greater))
                .ok_or_else(|| anyhow!("unsupported types for > operator at {:?}", meta.0))?;
            BamlValueWithMeta::Bool(greater, meta.clone())
        }
        BinaryOperator::GtEq => {
            let ord_opt = compare_values(&left_val.clone(), &right_val.clone())?;
            let greater_eq = ord_opt
                .map(|ord| matches!(ord, std::cmp::Ordering::Greater | std::cmp::Ordering::Equal))
                .ok_or_else(|| anyhow!("unsupported types for >= operator at {:?}", meta.0))?;
            BamlValueWithMeta::Bool(greater_eq, meta.clone())
        }

        // Logical operations
        BinaryOperator::And => match left_val.clone() {
            BamlValueWithMeta::Bool(false, _) => BamlValueWithMeta::Bool(false, meta.clone()),
            BamlValueWithMeta::Bool(true, _) => match right_val.clone() {
                BamlValueWithMeta::Bool(b, _) => BamlValueWithMeta::Bool(b, meta.clone()),
                _ => bail!("right operand of && must be bool at {:?}", meta.0),
            },
            _ => bail!("left operand of && must be bool at {:?}", meta.0),
        },
        BinaryOperator::Or => match left_val.clone() {
            BamlValueWithMeta::Bool(true, _) => BamlValueWithMeta::Bool(true, meta.clone()),
            BamlValueWithMeta::Bool(false, _) => match right_val.clone() {
                BamlValueWithMeta::Bool(b, _) => BamlValueWithMeta::Bool(b, meta.clone()),
                _ => bail!("right operand of || must be bool at {:?}", meta.0),
            },
            _ => bail!("left operand of || must be bool at {:?}", meta.0),
        },

        // Bitwise operations (integer only)
        BinaryOperator::BitAnd => match (left_val.clone(), right_val.clone()) {
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Int(b, _)) => {
                BamlValueWithMeta::Int(a & b, meta.clone())
            }
            _ => bail!("bitwise & requires integer operands at {:?}", meta.0),
        },
        BinaryOperator::BitOr => match (left_val.clone(), right_val.clone()) {
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Int(b, _)) => {
                BamlValueWithMeta::Int(a | b, meta.clone())
            }
            _ => bail!("bitwise | requires integer operands at {:?}", meta.0),
        },
        BinaryOperator::BitXor => match (left_val.clone(), right_val.clone()) {
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Int(b, _)) => {
                BamlValueWithMeta::Int(a ^ b, meta.clone())
            }
            _ => bail!("bitwise ^ requires integer operands at {:?}", meta.0),
        },
        BinaryOperator::Shl => match (left_val.clone(), right_val.clone()) {
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Int(b, _)) => {
                if b < 0 {
                    bail!("negative shift amount at {:?}", meta.0);
                }
                BamlValueWithMeta::Int(a << b, meta.clone())
            }
            _ => bail!("shift << requires integer operands at {:?}", meta.0),
        },
        BinaryOperator::Shr => match (left_val.clone(), right_val.clone()) {
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Int(b, _)) => {
                if b < 0 {
                    bail!("negative shift amount at {:?}", meta.0);
                }
                BamlValueWithMeta::Int(a >> b, meta.clone())
            }
            _ => bail!("shift >> requires integer operands at {:?}", meta.0),
        },
        BinaryOperator::InstanceOf => match (left_val.clone(), right_val.clone()) {
            (BamlValueWithMeta::Class(class, ..), BamlValueWithMeta::Class(right_class, ..)) => {
                BamlValueWithMeta::Bool(class == right_class, meta.clone())
            }
            _ => bail!("instanceof requires class operands at {:?}", meta.0),
        },
    })
}

fn evaluate_unary_op(
    operator: &crate::hir::UnaryOperator,
    val: &BamlValueWithMeta<ExprMetadata>,
    meta: &ExprMetadata,
) -> Result<BamlValueWithMeta<ExprMetadata>> {
    use crate::hir::UnaryOperator;
    Ok(match operator {
        UnaryOperator::Not => match val.clone() {
            BamlValueWithMeta::Bool(b, _) => BamlValueWithMeta::Bool(!b, meta.clone()),
            _ => bail!("! operator requires boolean operand at {:?}", meta.0),
        },
        UnaryOperator::Neg => match val.clone() {
            BamlValueWithMeta::Int(i, _) => BamlValueWithMeta::Int(-i, meta.clone()),
            BamlValueWithMeta::Float(f, _) => BamlValueWithMeta::Float(-f, meta.clone()),
            _ => bail!("- operator requires numeric operand at {:?}", meta.0),
        },
    })
}

fn values_equal(
    left: &BamlValueWithMeta<ExprMetadata>,
    right: &BamlValueWithMeta<ExprMetadata>,
) -> bool {
    match (left, right) {
        (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Int(b, _)) => a == b,
        (BamlValueWithMeta::Float(a, _), BamlValueWithMeta::Float(b, _)) => a == b,
        (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Float(b, _)) => *a as f64 == *b,
        (BamlValueWithMeta::Float(a, _), BamlValueWithMeta::Int(b, _)) => *a == *b as f64,
        (BamlValueWithMeta::String(a, _), BamlValueWithMeta::String(b, _)) => a == b,
        (BamlValueWithMeta::Null(_), BamlValueWithMeta::Null(_)) => true,
        _ => false,
    }
}

fn compare_values(
    left: &BamlValueWithMeta<ExprMetadata>,
    right: &BamlValueWithMeta<ExprMetadata>,
) -> Result<Option<std::cmp::Ordering>> {
    Ok(match (left, right) {
        (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Int(b, _)) => Some(a.cmp(b)),
        (BamlValueWithMeta::Float(a, _), BamlValueWithMeta::Float(b, _)) => a.partial_cmp(b),
        (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Float(b, _)) => {
            (*a as f64).partial_cmp(b)
        }
        (BamlValueWithMeta::Float(a, _), BamlValueWithMeta::Int(b, _)) => {
            a.partial_cmp(&(*b as f64))
        }
        (BamlValueWithMeta::String(a, _), BamlValueWithMeta::String(b, _)) => Some(a.cmp(b)),
        _ => None,
    })
}

fn evaluate_method_call(
    receiver: &BamlValueWithMeta<ExprMetadata>,
    method_name: &str,
    args: &[BamlValueWithMeta<ExprMetadata>],
    meta: &ExprMetadata,
) -> Result<BamlValueWithMeta<ExprMetadata>> {
    match method_name {
        "len" => {
            // Array/List length method
            match receiver {
                BamlValueWithMeta::List(items, _) => {
                    if !args.is_empty() {
                        bail!("len() method takes no arguments at {:?}", meta.0);
                    }
                    Ok(BamlValueWithMeta::Int(items.len() as i64, meta.clone()))
                }
                BamlValueWithMeta::String(s, _) => {
                    if !args.is_empty() {
                        bail!("len() method takes no arguments at {:?}", meta.0);
                    }
                    Ok(BamlValueWithMeta::Int(s.len() as i64, meta.clone()))
                }
                BamlValueWithMeta::Map(map, _) => {
                    if !args.is_empty() {
                        bail!("len() method takes no arguments at {:?}", meta.0);
                    }
                    Ok(BamlValueWithMeta::Int(map.len() as i64, meta.clone()))
                }
                _ => bail!(
                    "len() method not available on type {:?} at {:?}",
                    receiver,
                    meta.0
                ),
            }
        }
        _ => bail!(
            "unknown method '{}' at {:?}, should have been caught during typechecking",
            method_name,
            meta.0
        ),
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use baml_types::ir_type::TypeIR;
    use internal_baml_ast::parse_standalone_expression;
    use internal_baml_diagnostics::{Diagnostics, SourceFile, Span};
    use std::path::PathBuf;

    use super::*;
    use crate::hir::{self, Hir};
    use crate::thir;
    use crate::thir::typecheck::typecheck_expression;
    use crate::thir::{typecheck::typecheck_returning_context, GlobalAssignment, THir};

    fn meta() -> ExprMetadata {
        (Span::fake(), None)
    }

    fn empty_thir() -> THir<ExprMetadata> {
        THir {
            expr_functions: vec![],
            llm_functions: vec![],
            global_assignments: BamlMap::new(),
            classes: BamlMap::new(),
            enums: BamlMap::new(),
        }
    }

    /// Convenience function for creating THIR test fixtures.
    fn thir_from_src(
        src: &'static str,
        expr: &'static str,
    ) -> (THir<ExprMetadata>, thir::Expr<ExprMetadata>) {
        let parser_db = crate::test::ast(src).unwrap_or_else(|e| panic!("{}", e));
        let hir = Hir::from_ast(&parser_db.ast);
        let mut diagnostics = Diagnostics::new(PathBuf::from("test.baml"));
        diagnostics.set_source(&SourceFile::new_static(PathBuf::from("test.baml"), src));
        let (thir, typing_context) = typecheck_returning_context(&hir, &mut diagnostics);
        let expr_ast = parse_standalone_expression(expr, &mut diagnostics)
            .expect("Failed to parse expression");
        let expr_hir = hir::Expression::from_ast(&expr_ast);
        let expr_thir = typecheck_expression(&expr_hir, &typing_context, &mut diagnostics);
        (thir, expr_thir)
    }

    async fn mock_llm_function(
        _fn_name: String,
        _args: Vec<BamlValue>,
    ) -> Result<BamlValueWithMeta<ExprMetadata>> {
        // Mock LLM function that returns an error to simulate unsupported operation
        Ok(BamlValueWithMeta::Int(10, (Span::fake(), None)))
    }

    #[tokio::test]
    async fn eval_atom_int() {
        let thir = empty_thir();
        let expr = Expr::Value(BamlValueWithMeta::Int(1, meta()));
        let out = super::interpret_thir(
            thir,
            expr,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await
        .unwrap();
        match out {
            BamlValueWithMeta::Int(i, _) => assert_eq!(i, 1),
            v => panic!("expected int, got {v:?}"),
        }
    }

    #[tokio::test]
    async fn eval_function_call_identity() {
        let src = r#"
            function ConstantFunction(x: int) -> int {
                99
            }
        "#;

        let (thir, call) = thir_from_src(src, "ConstantFunction(42)");

        let out = super::interpret_thir(
            thir,
            call,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await
        .unwrap();
        match out {
            BamlValueWithMeta::Int(i, _) => assert_eq!(i, 99),
            v => panic!("expected int, got {v:?}"),
        }
    }

    #[tokio::test]
    async fn eval_function_uses_global() {
        let mut thir = empty_thir();
        thir.global_assignments.insert(
            "x".to_string(),
            GlobalAssignment {
                expr: Expr::Value(BamlValueWithMeta::Int(7, meta())),
                annotated_type: None,
            },
        );

        // Function with arity 0 returning free var `x`
        let body = Block {
            env: BamlMap::new(),
            statements: vec![],
            trailing_expr: Some(Expr::Var("x".to_string(), meta())),
            ty: None,
            span: Span::fake(),
        };
        let func = Expr::Function(0, Arc::new(body), meta());
        let call = Expr::Call {
            func: Arc::new(func),
            type_args: vec![],
            args: vec![],
            meta: meta(),
        };

        let out = super::interpret_thir(
            thir,
            call,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await
        .unwrap();
        match out {
            BamlValueWithMeta::Int(i, _) => assert_eq!(i, 7),
            v => panic!("expected int, got {v:?}"),
        }
    }

    #[tokio::test]
    async fn test_llm_function_call() {
        let src = r##"
            client<llm> GPT35 {
                provider baml-openai-chat
                options {
                    model gpt-3.5-turbo
                    api_key env.OPENAI_API_KEY
                }
            }

            function SummarizeText(text: string) -> string {
                client GPT35
                prompt #"
                    Summarize the following text: {{ text }}
                "#
            }
        "##;

        let (thir, call) = thir_from_src(
            src,
            r#"SummarizeText("This is a long text that needs to be summarized.")"#,
        );

        // Since the interpreter uses our mock LLM function, this should return our mock value
        let result = super::interpret_thir(
            thir,
            call,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await;
        assert!(result.is_ok());
        let out = result.unwrap();
        match out {
            BamlValueWithMeta::Int(i, _) => assert_eq!(i, 10),
            v => panic!("expected int, got {v:?}"),
        }
    }

    #[tokio::test]
    async fn test_method_call_array_len() {
        let (thir, expr) = thir_from_src("", "[1, 2, 3].len()");

        let result = super::interpret_thir(
            thir,
            expr,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await
        .unwrap();

        match result {
            BamlValueWithMeta::Int(len, _) => assert_eq!(len, 3),
            v => panic!("expected int, got {v:?}"),
        }
    }

    #[tokio::test]
    async fn test_method_call_string_len() {
        let (thir, expr) = thir_from_src("", r#""hello".len()"#);

        let result = super::interpret_thir(
            thir,
            expr,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await
        .unwrap();

        match result {
            BamlValueWithMeta::Int(len, _) => assert_eq!(len, 5),
            v => panic!("expected int, got {v:?}"),
        }
    }

    #[tokio::test]
    async fn env_get_returns_value() {
        let thir = empty_thir();
        let call = Expr::Call {
            func: Arc::new(Expr::Var("env.get".to_string(), meta())),
            type_args: vec![],
            args: vec![Expr::Value(BamlValueWithMeta::String(
                "API_KEY".to_string(),
                meta(),
            ))],
            meta: meta(),
        };

        let mut env_vars = HashMap::new();
        env_vars.insert("API_KEY".to_string(), "secret123".to_string());

        let result = super::interpret_thir(thir, call, mock_llm_function, BamlMap::new(), env_vars)
            .await
            .unwrap();

        match result {
            BamlValueWithMeta::String(value, _) => assert_eq!(value, "secret123"),
            v => panic!("expected string, got {v:?}"),
        }
    }

    #[tokio::test]
    async fn test_method_call_unknown_method() {
        let (thir, expr) = thir_from_src("", r#""hello".unknown_method()"#);

        let result = super::interpret_thir(
            thir,
            expr,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains(&format!("unknown {}", "method")));
    }

    #[tokio::test]
    async fn test_fibonacci_function() {
        let src = r#"
            function Fib(n: int) -> int {
                let a = 0;
                let b = 1;
                while (n > 0) {
                    n -= 1;
                    let t = a + b;
                    b = a;
                    a = t;
                }
                a
            }
        "#;

        let (thir, fib_call) = thir_from_src(src, "Fib(5)");

        let result = super::interpret_thir(
            thir,
            fib_call,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await
        .unwrap();

        match result {
            BamlValueWithMeta::Int(actual, _) => {
                assert_eq!(actual, 5);
            }
            v => {
                panic!("Expected int result, got {:?}", v);
            }
        }
    }

    #[tokio::test]
    async fn test_bool_to_int_with_if_else() {
        // Test if (true) { 1 } else { 0 }
        let (thir, if_expr_true) = thir_from_src("", "if (true) { 1 } else { 0 }");

        let result = super::interpret_thir(
            thir,
            if_expr_true,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await
        .unwrap();

        match result {
            BamlValueWithMeta::Int(actual, _) => {
                assert_eq!(actual, 1, "if (true) should return 1, got {actual}");
            }
            v => panic!("Expected int result for if (true), got {v:?}"),
        }

        // Test if (false) { 1 } else { 0 }
        let (thir, if_expr_false) = thir_from_src("", "if (false) { 1 } else { 0 }");

        let result = super::interpret_thir(
            thir,
            if_expr_false,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await
        .unwrap();

        match result {
            BamlValueWithMeta::Int(actual, _) => {
                assert_eq!(actual, 0, "if (false) should return 0, got {actual}");
            }
            v => panic!("Expected int result for if (false), got {v:?}"),
        }
    }

    #[tokio::test]
    async fn test_if_else_with_function_equivalent() {
        let src = r#"
            function BoolToIntWithIfElse(b: bool) -> int {
                let result = if (b) { 1 } else { 0 };
                result
            }
        "#;

        // Test with true
        let (thir, call_true) = thir_from_src(src, "BoolToIntWithIfElse(true)");

        let result = super::interpret_thir(
            thir,
            call_true,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await
        .unwrap();

        match result {
            BamlValueWithMeta::Int(actual, _) => {
                assert_eq!(
                    actual, 1,
                    "BoolToIntWithIfElse(true) should return 1, got {actual}"
                );
            }
            v => panic!("Expected int result for BoolToIntWithIfElse(true), got {v:?}"),
        }

        // Test with false
        let (thir, call_false) = thir_from_src(src, "BoolToIntWithIfElse(false)");

        let result = super::interpret_thir(
            thir,
            call_false,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await
        .unwrap();

        match result {
            BamlValueWithMeta::Int(actual, _) => {
                assert_eq!(
                    actual, 0,
                    "BoolToIntWithIfElse(false) should return 0, got {actual}"
                );
            }
            v => panic!("Expected int result for BoolToIntWithIfElse(false), got {v:?}"),
        }
    }

    #[tokio::test]
    async fn test_store_fn_call_in_local_var() {
        let src = r#"
            function ReturnNumber(n: int) -> int {
                n
            }

            function StoreFnCallInLocalVar(n: int) -> int {
                let result = ReturnNumber(n);
                result
            }
        "#;

        // Test with value 42
        let (thir, call_expr) = thir_from_src(src, "StoreFnCallInLocalVar(42)");

        let result = super::interpret_thir(
            thir,
            call_expr,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await
        .unwrap();

        match result {
            BamlValueWithMeta::Int(actual, _) => {
                assert_eq!(
                    actual, 42,
                    "StoreFnCallInLocalVar(42) should return 42, got {actual}"
                );
            }
            v => panic!("Expected int result for StoreFnCallInLocalVar(42), got {v:?}"),
        }
    }

    #[tokio::test]
    async fn test_declare_and_assign_exactly_like_thir() {
        let src = r#"
            function AssignElseIfExpr(a: bool, b: bool) -> int {
                let result = if (a) { 1 } else if (b) { 2 } else { 3 };
                result
            }
        "#;

        // Test with (true, false) - should return 1
        let (thir, call_expr) = thir_from_src(src, "AssignElseIfExpr(true, false)");

        let result = super::interpret_thir(
            thir,
            call_expr,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await
        .unwrap();

        match result {
            BamlValueWithMeta::Int(actual, _) => {
                assert_eq!(
                    actual, 1,
                    "AssignElseIfExpr(true, false) should return 1, got {actual}"
                );
            }
            v => panic!("Expected int result for AssignElseIfExpr(true, false), got {v:?}"),
        }
    }

    #[tokio::test]
    async fn test_compile_real_baml_to_thir() {
        // Test compiling real BAML code to see what THIR is actually generated
        use crate::hir::Hir;
        use crate::thir::typecheck::typecheck;
        use internal_baml_diagnostics::Diagnostics;
        use internal_baml_parser_database::parse_and_diagnostics;

        let baml_code = r#"
            function AssignElseIfExpr(a: bool, b: bool) -> int {
                let result = if (a) {
                    1
                } else if (b) {
                    2
                } else {
                    3
                };

                result
            }
        "#;

        // Parse BAML code to AST
        let (db, parse_diagnostics) =
            parse_and_diagnostics(baml_code).expect(&format!("Failed to parse BAML {}", "code"));

        if parse_diagnostics.has_errors() {
            let errors = parse_diagnostics.to_pretty_string();
            panic!("Parse errors: {errors}");
        }

        let ast = db.ast().clone();

        // Convert AST to HIR
        let hir = Hir::from_ast(&ast);

        // Convert HIR to THIR
        let mut diagnostics = Diagnostics::new("test".into());
        let thir = typecheck(&hir, &mut diagnostics);

        if diagnostics.has_errors() {
            let errors = diagnostics.to_pretty_string();
            panic!("Compilation errors: {errors}");
        }

        // Find the AssignElseIfExpr function
        let function = thir
            .expr_functions
            .iter()
            .find(|f| f.name == "AssignElseIfExpr")
            .expect(&format!("AssignElseIfExpr function not {}", "found"));

        // Test the function
        let call_expr = Expr::Call {
            func: Arc::new(Expr::Var("AssignElseIfExpr".to_string(), meta())),
            type_args: vec![],
            args: vec![
                Expr::Value(BamlValueWithMeta::Bool(true, meta())),
                Expr::Value(BamlValueWithMeta::Bool(false, meta())),
            ],
            meta: meta(),
        };

        let result = super::interpret_thir(
            thir.clone(),
            call_expr,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await;

        match result {
            Ok(BamlValueWithMeta::Int(actual, _)) => {
                assert_eq!(
                    actual, 1,
                    "AssignElseIfExpr(true, false) should return 1, got {actual}"
                );
            }
            Ok(v) => panic!("Expected int result, got {v:?}"),
            Err(e) => {
                println!("❌ Real BAML compilation test failed with error: {e}");
                // This might be the actual bug we need to fix
                panic!("Function failed to execute: {e}");
            }
        }
    }

    #[tokio::test]
    async fn test_debug_bool_to_int_with_if_else() {
        // Debug the BoolToIntWithIfElse function that's returning None
        use crate::hir::Hir;
        use crate::thir::typecheck::typecheck;
        use internal_baml_diagnostics::Diagnostics;
        use internal_baml_parser_database::parse_and_diagnostics;

        let baml_code = r#"
            function BoolToIntWithIfElse(b: bool) -> int {
                let result = if (b) { 1 } else { 0 };
                result
            }
        "#;

        // Parse and compile BAML code
        let (db, parse_diagnostics) =
            parse_and_diagnostics(baml_code).expect(&format!("Failed to parse BAML {}", "code"));

        if parse_diagnostics.has_errors() {
            let errors = parse_diagnostics.to_pretty_string();
            panic!("Parse errors: {errors}");
        }

        let ast = db.ast().clone();
        let hir = Hir::from_ast(&ast);
        let mut diagnostics = Diagnostics::new("test".into());
        let thir = typecheck(&hir, &mut diagnostics);

        if diagnostics.has_errors() {
            let errors = diagnostics.to_pretty_string();
            panic!("Compilation errors: {errors}");
        }

        // Find the function and debug its structure
        let function = thir
            .expr_functions
            .iter()
            .find(|f| f.name == "BoolToIntWithIfElse")
            .expect(&format!("BoolToIntWithIfElse function not {}", "found"));

        println!("Function THIR: {}", function.body.dump_str());
        println!("Statements count: {}", function.body.statements.len());
        println!(
            "Has trailing expr: {}",
            function.body.trailing_expr.is_some()
        );

        if let Some(trailing_expr) = &function.body.trailing_expr {
            println!("Trailing expr: {}", trailing_expr.dump_str());
        }

        // Test with true
        let call_expr = Expr::Call {
            func: Arc::new(Expr::Var("BoolToIntWithIfElse".to_string(), meta())),
            type_args: vec![],
            args: vec![Expr::Value(BamlValueWithMeta::Bool(true, meta()))],
            meta: meta(),
        };

        let result = super::interpret_thir(
            thir.clone(),
            call_expr,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await;

        match result {
            Ok(value) => {
                println!("Result: {value:?}");
                match value {
                    BamlValueWithMeta::Int(actual, _) => {
                        assert_eq!(actual, 1, "Expected 1, got {actual}");
                    }
                    _ => panic!("Expected int result, got {value:?}"),
                }
            }
            Err(e) => {
                panic!("Function failed: {e}");
            }
        }

        println!("✅ BoolToIntWithIfElse debug test passed!");
    }

    #[tokio::test]
    async fn test_iterative_fibonacci() {
        // Test the iterative Fibonacci function implementation
        use crate::hir::Hir;
        use crate::thir::typecheck::typecheck;
        use internal_baml_diagnostics::Diagnostics;
        use internal_baml_parser_database::parse_and_diagnostics;

        // function IterativeFibonacci(n: int) -> int {
        //     let a = 0;
        //     let b = 1;
        //
        //     if (n == 0) {
        //         b
        //     } else {
        //         let i = 1;
        //         while (i <= n) {
        //             let c = a + b;
        //             a = b;
        //             b = c;
        //             i += 1;
        //         }
        //         a
        //     }
        // }

        let baml_code = r#"
            function IterativeFibonacci(n: int) -> int {
                let a = 0;
                let b = 1;

                if (n == 0) {
                    b
                } else {
                    let i = 1;
                    while (i <= n) {
                        let c = a + b;
                        a = b;
                        b = c;
                        i += 1;
                    }
                    a
                }
            }
        "#;

        let src = baml_code;

        let (thir, call_expr) = thir_from_src(src, "IterativeFibonacci(5)");

        let result = super::interpret_thir(
            thir,
            call_expr,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await
        .unwrap();

        match result {
            BamlValueWithMeta::Int(actual, _) => {
                assert_eq!(actual, 5);
            }
            v => {
                panic!("Expected int result, got {:?}", v);
            }
        }
    }
}
