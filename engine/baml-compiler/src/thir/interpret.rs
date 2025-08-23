use std::{cell::RefCell, future::Future, sync::Arc};

use anyhow::{anyhow, bail, Context, Result};
use baml_types::{BamlMap, BamlValue, BamlValueWithMeta};

use crate::thir::{Block, Expr, ExprMetadata, Statement, THir};

/// A scope is a map of variable names to their values.
///
/// Variables are stored in refcells to allow for mutation.
pub struct Scope {
    pub variables: BamlMap<String, RefCell<BamlValueWithMeta<ExprMetadata>>>,
}

enum EvalValue {
    Value(BamlValueWithMeta<ExprMetadata>),
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
) -> Result<BamlValueWithMeta<ExprMetadata>>
where
    F: FnMut(String, Vec<BamlValue>) -> Fut + Send + Sync,
    Fut: Future<Output = Result<BamlValueWithMeta<ExprMetadata>>> + Send,
{
    let mut scopes = vec![Scope {
        variables: BamlMap::from_iter(
            extra_bindings
                .into_iter()
                .map(|(k, v)| (k, RefCell::new(v))),
        ),
    }];

    // Seed scope with global assignments
    for (name, gexpr) in thir.global_assignments.iter() {
        let v =
            expect_value(evaluate_expr(gexpr, &mut scopes, &thir, &mut run_llm_function).await?)?;
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
        for stmt in block.statements.iter() {
            match stmt {
                Statement::Let { name, value, .. } => {
                    let v =
                        expect_value(evaluate_expr(value, scopes, thir, run_llm_function).await?)?;
                    declare(scopes, name, v);
                }
                Statement::Declare { name, span } => {
                    declare(scopes, name, BamlValueWithMeta::Null((span.clone(), None)));
                }
                Statement::Assign { left, value } => {
                    // For now, we only support simple variable assignment (identifiers)
                    let var_name = match left {
                        Expr::Var(name, _) => name,
                        _ => {
                            return Err(anyhow::anyhow!(
                                "Complex assignment targets not yet supported"
                            ))
                        }
                    };
                    let v =
                        expect_value(evaluate_expr(value, scopes, thir, run_llm_function).await?)?;
                    assign(scopes, var_name, v)?;
                }
                Statement::DeclareAndAssign { name, value, .. } => {
                    let v =
                        expect_value(evaluate_expr(value, scopes, thir, run_llm_function).await?)?;
                    declare(scopes, name, v);
                }
                Statement::Return { expr, .. } => {
                    let v =
                        expect_value(evaluate_expr(expr, scopes, thir, run_llm_function).await?)?;
                    scopes.pop();
                    return Ok(ControlFlow::Return(v));
                }
                Statement::Expression { expr, .. } => {
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

                    // For now, we only support simple variable assignment (identifiers)
                    let var_name = match left {
                        Expr::Var(name, _) => name,
                        _ => {
                            return Err(anyhow::anyhow!(
                                "Complex assignment targets not yet supported"
                            ))
                        }
                    };

                    // Get current value of the variable
                    let current_val = lookup(scopes, var_name).with_context(|| {
                        format!("assign op to undeclared variable `{}`", var_name)
                    })?;

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
                            ) => BamlValueWithMeta::String(format!("{}{}", a, b), meta),
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

                    // Assign the result back to the variable
                    assign(scopes, var_name, result_val)?;
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

                                            // For now, we only support simple variable assignment (identifiers)
                                            let var_name = match left {
                                                Expr::Var(name, _) => name,
                                                _ => {
                                                    return Err(anyhow::anyhow!(
                                                    "Complex assignment targets not yet supported"
                                                ))
                                                }
                                            };

                                            let current_val = lookup(scopes, var_name)
                                                .with_context(|| {
                                                    format!(
                                                        "assign op to undeclared variable `{}`",
                                                        var_name
                                                    )
                                                })?;
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
                                                AssignOp::AddAssign => match (current_val.clone(), rhs_val.clone()) {
                                                    (BamlValueWithMeta::Int(a, meta), BamlValueWithMeta::Int(b, _)) => {
                                                        BamlValueWithMeta::Int(a + b, meta)
                                                    }
                                                    _ => bail!("unsupported types for += in C-for after clause"),
                                                },
                                                _ => bail!("unsupported assign op in C-for after clause"),
                                            };
                                            assign(scopes, var_name, result_val)?;
                                        }
                                        Statement::Assign { left, value } => {
                                            // For now, we only support simple variable assignment (identifiers)
                                            let var_name = match left {
                                                Expr::Var(name, _) => name,
                                                _ => {
                                                    return Err(anyhow::anyhow!(
                                                    "Complex assignment targets not yet supported"
                                                ))
                                                }
                                            };
                                            let v = expect_value(
                                                evaluate_expr(
                                                    value,
                                                    scopes,
                                                    thir,
                                                    run_llm_function,
                                                )
                                                .await?,
                                            )?;
                                            assign(scopes, var_name, v)?;
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

                                            // For now, we only support simple variable assignment (identifiers)
                                            let var_name = match left {
                                                Expr::Var(name, _) => name,
                                                _ => {
                                                    return Err(anyhow::anyhow!(
                                                    "Complex assignment targets not yet supported"
                                                ))
                                                }
                                            };

                                            let current_val = lookup(scopes, var_name)
                                                .with_context(|| {
                                                    format!(
                                                        "assign op to undeclared variable `{}`",
                                                        var_name
                                                    )
                                                })?;
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
                                                AssignOp::AddAssign => match (current_val.clone(), rhs_val.clone()) {
                                                    (BamlValueWithMeta::Int(a, meta), BamlValueWithMeta::Int(b, _)) => {
                                                        BamlValueWithMeta::Int(a + b, meta)
                                                    }
                                                    _ => bail!("unsupported types for += in C-for after clause"),
                                                },
                                                _ => bail!("unsupported assign op in C-for after clause"),
                                            };
                                            assign(scopes, var_name, result_val)?;
                                        }
                                        Statement::Assign { left, value } => {
                                            // For now, we only support simple variable assignment (identifiers)
                                            let var_name = match left {
                                                Expr::Var(name, _) => name,
                                                _ => {
                                                    return Err(anyhow::anyhow!(
                                                    "Complex assignment targets not yet supported"
                                                ))
                                                }
                                            };
                                            let v = expect_value(
                                                evaluate_expr(
                                                    value,
                                                    scopes,
                                                    thir,
                                                    run_llm_function,
                                                )
                                                .await?,
                                            )?;
                                            assign(scopes, var_name, v)?;
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
        let ret = if let Some(trailing_expr) = &block.trailing_expr {
            expect_value(evaluate_expr(trailing_expr, scopes, thir, run_llm_function).await?)?
        } else {
            // If no trailing expression, return null
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
            .insert(name.to_string(), RefCell::new(value));
    }
}

fn assign(scopes: &mut [Scope], name: &str, value: BamlValueWithMeta<ExprMetadata>) -> Result<()> {
    for s in scopes.iter_mut().rev() {
        if let Some(cell) = s.variables.get_mut(name) {
            *cell.borrow_mut() = value;
            return Ok(());
        }
    }
    bail!("assign to undeclared variable `{}`", name)
}

fn lookup(scopes: &[Scope], name: &str) -> Option<BamlValueWithMeta<ExprMetadata>> {
    for s in scopes.iter().rev() {
        if let Some(cell) = s.variables.get(name) {
            return Some(cell.borrow().clone());
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
                                format!("__LLM_FUNCTION__{}", name),
                                meta.clone(),
                            ))),
                            ty: None,
                            span: internal_baml_diagnostics::Span::fake(),
                        }),
                        meta.clone(),
                    )
                } else {
                    let v = lookup(scopes, name)
                        .with_context(|| format!("unbound variable `{}` at {:?}", name, meta.0))?;
                    EvalValue::Value(v)
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

                // Create fresh names and open body under them
                let body_expr =
                    Expr::Block(Box::new(Arc::unwrap_or_clone(body.clone())), meta.clone());
                let fresh = body_expr.fresh_names(arity);
                // let mut opened = body_expr;
                // for (i, name) in fresh.iter().enumerate() {
                //     opened = opened.open(
                //         &VarIndex {
                //             de_bruijn: 0,
                //             tuple: i as u32,
                //         },
                //         name,
                //     );
                // }

                // Create a scope binding parameters to their argument values
                scopes.push(Scope {
                    variables: fresh
                        .into_iter()
                        .zip(arg_vals)
                        .map(|(k, v)| (k, RefCell::new(v)))
                        .collect(),
                });
                // TODO: Check this.
                let result = match &body_expr {
                    Expr::Block(b, _) => evaluate_block(b, scopes, thir, run_llm_function).await?,
                    other => {
                        expect_value(evaluate_expr(other, scopes, thir, run_llm_function).await?)?
                    }
                };
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
            Expr::ClassConstructor {
                name,
                fields,
                spread,
                meta,
            } => {
                let mut field_map: BamlMap<String, BamlValueWithMeta<ExprMetadata>> =
                    BamlMap::new();

                // Handle spread first if present
                if let Some(spread_expr) = spread {
                    let spread_val = expect_value(
                        evaluate_expr(spread_expr, scopes, thir, run_llm_function).await?,
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

                // Evaluate and insert explicit fields (these override spread fields)
                for (k, v) in fields.iter() {
                    field_map.insert(
                        k.clone(),
                        expect_value(evaluate_expr(v, scopes, thir, run_llm_function).await?)?,
                    );
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
                        bail!("builtin function std::fetch_value is not supported in interpreter at {:?}", meta.0)
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
                receiver: _,
                method: _,
                args: _,
                meta: _,
            } => {
                todo!("method calls are not supported in the interpreter")
            }
            Expr::Paren(inner, _) => evaluate_expr(inner, scopes, thir, run_llm_function).await?,
        })
    })
}

fn expect_value(v: EvalValue) -> Result<BamlValueWithMeta<ExprMetadata>> {
    match v {
        EvalValue::Value(v) => Ok(v),
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
                BamlValueWithMeta::String(format!("{}{}", a, b), meta.clone())
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

// /// Convert a BamlValue to BamlValueWithMeta by adding the given metadata
// fn baml_value_to_with_meta(value: &BamlValue, meta: ExprMetadata) -> BamlValueWithMeta<ExprMetadata> {
//     match value {
//         BamlValue::String(s) => BamlValueWithMeta::String(s.clone(), meta),
//         BamlValue::Int(i) => BamlValueWithMeta::Int(*i, meta),
//         BamlValue::Float(f) => BamlValueWithMeta::Float(*f, meta),
//         BamlValue::Bool(b) => BamlValueWithMeta::Bool(*b, meta),
//         BamlValue::Map(m) => {
//             let with_meta_map = m.iter()
//                 .map(|(k, v)| (k.clone(), baml_value_to_with_meta(v, meta.clone())))
//                 .collect();
//             BamlValueWithMeta::Map(with_meta_map, meta)
//         },
//         BamlValue::List(l) => {
//             let with_meta_list = l.iter()
//                 .map(|v| baml_value_to_with_meta(v, meta.clone()))
//                 .collect();
//             BamlValueWithMeta::List(with_meta_list, meta)
//         },
//         BamlValue::Media(m) => BamlValueWithMeta::Media(m.clone(), meta),
//         BamlValue::Enum(name, val) => BamlValueWithMeta::Enum(name.clone(), val.clone(), meta),
//         BamlValue::Class(name, fields) => {
//             let with_meta_fields = fields.iter()
//                 .map(|(k, v)| (k.clone(), baml_value_to_with_meta(v, meta.clone())))
//                 .collect();
//             BamlValueWithMeta::Class(name.clone(), with_meta_fields, meta)
//         },
//         BamlValue::Null => BamlValueWithMeta::Null(meta),
//     }
// }

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use baml_types::ir_type::TypeIR;
    use internal_baml_diagnostics::Span;

    use super::*;
    use crate::thir::THir;

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
        let out = super::interpret_thir(thir, expr, mock_llm_function, BamlMap::new())
            .await
            .unwrap();
        match out {
            BamlValueWithMeta::Int(i, _) => assert_eq!(i, 1),
            v => panic!("expected int, got {:?}", v),
        }
    }

    #[tokio::test]
    async fn eval_function_call_identity() {
        let thir = empty_thir();
        // Create a simple function that just returns a constant value
        // Since parameter substitution isn't fully implemented, we test a simpler case
        let body = Block {
            env: BamlMap::new(),
            statements: vec![],
            trailing_expr: Some(Expr::Value(BamlValueWithMeta::Int(99, meta()))),
            ty: None,
            span: Span::fake(),
        };

        let func = Expr::Function(1, Arc::new(body), meta());
        let call = Expr::Call {
            func: Arc::new(func),
            type_args: vec![],
            args: vec![Expr::Value(BamlValueWithMeta::Int(42, meta()))],
            meta: meta(),
        };

        let out = super::interpret_thir(thir, call, mock_llm_function, BamlMap::new())
            .await
            .unwrap();
        match out {
            BamlValueWithMeta::Int(i, _) => assert_eq!(i, 99),
            v => panic!("expected int, got {:?}", v),
        }
    }

    #[tokio::test]
    async fn eval_function_uses_global() {
        let mut thir = empty_thir();
        thir.global_assignments.insert(
            "x".to_string(),
            Expr::Value(BamlValueWithMeta::Int(7, meta())),
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

        let out = super::interpret_thir(thir, call, mock_llm_function, BamlMap::new())
            .await
            .unwrap();
        match out {
            BamlValueWithMeta::Int(i, _) => assert_eq!(i, 7),
            v => panic!("expected int, got {:?}", v),
        }
    }

    #[tokio::test]
    async fn test_llm_function_call() {
        use baml_types::ir_type::TypeIR;

        use crate::hir::{LlmFunction, Parameter as HirParameter};

        let thir = THir {
            expr_functions: vec![],
            llm_functions: vec![LlmFunction {
                name: "SummarizeText".to_string(),
                parameters: vec![HirParameter {
                    name: "text".to_string(),
                    r#type: TypeIR::string(),
                    span: internal_baml_diagnostics::Span::fake(),
                    is_mutable: false,
                }],
                return_type: TypeIR::string(),
                client: "GPT35".to_string(),
                prompt: "Summarize the following text: {{ text }}".to_string(),
                span: internal_baml_diagnostics::Span::fake(),
            }],
            global_assignments: BamlMap::new(),
            classes: BamlMap::new(),
            enums: BamlMap::new(),
        };

        // Call the LLM function with a string argument using FreeVar reference
        let call = Expr::Call {
            func: Arc::new(Expr::Var("SummarizeText".to_string(), meta())),
            type_args: vec![],
            args: vec![Expr::Value(BamlValueWithMeta::String(
                "This is a long text that needs to be summarized.".to_string(),
                meta(),
            ))],
            meta: meta(),
        };

        // Since the interpreter uses our mock LLM function, this should fail with our mock error message
        let result = super::interpret_thir(thir, call, mock_llm_function, BamlMap::new()).await;
        assert!(result.is_ok());
        let out = result.unwrap();
        match out {
            BamlValueWithMeta::Int(i, _) => assert_eq!(i, 10),
            v => panic!("expected int, got {:?}", v),
        }
    }
}
