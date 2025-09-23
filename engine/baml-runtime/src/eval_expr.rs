use std::{
    collections::HashMap,
    str::FromStr,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, Context};
use baml_types::{
    expr::{Builtin, Expr, ExprMetadata, Name, VarIndex},
    type_meta::base::TypeMeta,
    Arrow, BamlMap, BamlValue, BamlValueWithMeta, EvaluationContext, TypeIR, TypeValue,
};
use futures::{
    channel::mpsc,
    stream::{self as stream, StreamExt},
};
use internal_baml_core::{
    internal_baml_diagnostics::SerializedSpan,
    internal_baml_parser_database::coerce,
    ir::{builtin, repr::IntermediateRepr},
};
use internal_baml_jinja::types::OutputFormatContent;
use jsonish::{deserializer::deserialize_flags::Flag, helpers::render_output_format};

use crate::{BamlRuntime, FunctionResult, TripWire};

const MAX_STEPS: usize = 1000;

pub struct EvalEnv<'a> {
    pub context: HashMap<Name, Expr<ExprMetadata>>,
    pub runtime: &'a BamlRuntime,
    pub expr_tx: Option<mpsc::UnboundedSender<Vec<SerializedSpan>>>,
    /// Evaluated top-level expressions.
    pub evaluated_cache: Arc<Mutex<HashMap<Name, Expr<ExprMetadata>>>>,
    pub env_vars: HashMap<String, String>,
}

impl<'a> EvalEnv<'a> {
    pub fn dump_ctx(&self) -> String {
        self.context
            .iter()
            .map(|(k, v)| format!("{}: {}", k, v.dump_str()))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Substitute val for var_name in expr.
fn subst<'a>(
    expr: &Expr<ExprMetadata>,
    var_name: &VarIndex,
    val: &Expr<ExprMetadata>,
    _env: &EvalEnv<'a>,
) -> anyhow::Result<Expr<ExprMetadata>> {
    let res: anyhow::Result<Expr<ExprMetadata>> = match expr {
        Expr::BoundVar(expr_var_name, _) => {
            if expr_var_name == var_name {
                Ok(val.clone())
            } else {
                Ok(expr.clone())
            }
        }
        Expr::Builtin(builtin, meta) => Ok(expr.clone()),
        Expr::FreeVar(name, _) => Ok(expr.clone()),
        Expr::Atom(_) => Ok(expr.clone()),
        Expr::App {
            func,
            args,
            meta,
            type_args,
        } => {
            let f2 = subst(func, var_name, val, _env)?;
            let x2 = subst(args, var_name, val, _env)?;
            Ok(Expr::App {
                func: Arc::new(f2),
                args: Arc::new(x2),
                meta: meta.clone(),
                type_args: type_args.clone(),
            })
        }
        Expr::Lambda(params, body, meta) => Ok(Expr::Lambda(
            *params,
            Arc::new(subst(body, var_name, val, _env)?),
            meta.clone(),
        )),
        Expr::ArgsTuple(args, meta) => {
            let mut new_args = Vec::new();
            for arg in args {
                new_args.push(subst(arg, var_name, val, _env)?);
            }
            Ok(Expr::ArgsTuple(new_args, meta.clone()))
        }
        Expr::LLMFunction(_, _, _) => Ok(expr.clone()),
        Expr::Let(name, value, body, meta) => {
            let new_value = subst(value, var_name, val, _env)?;
            let new_body = subst(body, var_name, val, _env)?;
            Ok(Expr::Let(
                name.clone(),
                Arc::new(new_value),
                Arc::new(new_body),
                meta.clone(),
            ))
        }
        Expr::List(items, meta) => {
            let new_items = items
                .iter()
                .map(|item| subst(item, var_name, val, _env))
                .collect::<anyhow::Result<Vec<_>>>()?;
            Ok(Expr::List(new_items, meta.clone()))
        }
        Expr::Map(items, meta) => {
            let new_items = items
                .iter()
                .map(|(key, value)| {
                    let new_value = subst(value, var_name, val, _env)?;
                    Ok((key.clone(), new_value))
                })
                .collect::<anyhow::Result<BamlMap<_, _>>>()?;
            Ok(Expr::Map(new_items, meta.clone()))
        }
        Expr::ClassConstructor {
            name,
            fields,
            spread,
            meta,
        } => {
            let new_fields = fields
                .iter()
                .map(|(key, value)| {
                    let new_value = subst(value, var_name, val, _env)?;
                    Ok((key.clone(), new_value))
                })
                .collect::<anyhow::Result<BamlMap<_, _>>>()?;
            let new_spread = spread
                .as_ref()
                .map(|spread| {
                    subst(spread, var_name, val, _env).map(|spread| Box::new(spread.clone()))
                })
                .transpose()?;
            Ok(Expr::ClassConstructor {
                name: name.clone(),
                fields: new_fields,
                spread: new_spread,
                meta: meta.clone(),
            })
        }
        Expr::If(cond, then, else_, meta) => {
            let new_cond = subst(cond, var_name, val, _env)?;
            let new_then = subst(then, var_name, val, _env)?;
            let new_else = else_
                .as_ref()
                .map(|e| subst(e, var_name, val, _env))
                .transpose()?;
            Ok(Expr::If(
                Arc::new(new_cond),
                Arc::new(new_then),
                new_else.map(Arc::new),
                meta.clone(),
            ))
        }
        Expr::ForLoop {
            item,
            iterable,
            body,
            meta,
        } => {
            let new_iterable = subst(iterable, var_name, val, _env)?;
            let new_body = subst(body, var_name, val, _env)?;
            Ok(Expr::ForLoop {
                item: item.clone(),
                iterable: Arc::new(new_iterable),
                body: Arc::new(new_body),
                meta: meta.clone(),
            })
        }
        Expr::ArrayAccess { base, index, meta } => {
            let new_base = subst(base, var_name, val, _env)?;
            let new_index = subst(index, var_name, val, _env)?;
            Ok(Expr::ArrayAccess {
                base: Arc::new(new_base),
                index: Arc::new(new_index),
                meta: meta.clone(),
            })
        }
        Expr::FieldAccess { base, field, meta } => {
            let new_base = subst(base, var_name, val, _env)?;
            Ok(Expr::FieldAccess {
                base: Arc::new(new_base),
                field: field.clone(),
                meta: meta.clone(),
            })
        }
        Expr::BinaryOperation {
            left,
            operator,
            right,
            meta,
        } => {
            let new_left = subst(left, var_name, val, _env)?;
            let new_right = subst(right, var_name, val, _env)?;
            Ok(Expr::BinaryOperation {
                left: Arc::new(new_left),
                operator: operator.clone(),
                right: Arc::new(new_right),
                meta: meta.clone(),
            })
        }
        Expr::UnaryOperation {
            expr,
            operator,
            meta,
        } => {
            let new_expr = subst(expr, var_name, val, _env)?;
            Ok(Expr::UnaryOperation {
                expr: Arc::new(new_expr),
                operator: operator.clone(),
                meta: meta.clone(),
            })
        }
    };
    let res = res?;
    Ok(res)
}

/// Perform a single beta reduction. Note that we ignore env.context
/// here. Only use env for the runtime.
async fn beta_reduce<'a>(
    env: &EvalEnv<'a>,
    expr: &Expr<ExprMetadata>,
    eval_final_llm_fn: bool,
) -> anyhow::Result<Expr<ExprMetadata>> {
    match expr {
        Expr::Atom(_) => Ok(expr.clone()),
        Expr::Let(name, value, body, meta) => {
            // First evaluate the bound expression
            let evaluated_value = Box::pin(beta_reduce(env, value, eval_final_llm_fn)).await?;

            // Then substitute the evaluated value into the body
            let target = VarIndex {
                de_bruijn: 0,
                tuple: 0,
            };
            let closed_body = body.close(&target, name);
            let substituted_body = subst(&closed_body, &target, &evaluated_value, env)?;

            // Finally evaluate the body with the substitution
            Box::pin(beta_reduce(env, &substituted_body, eval_final_llm_fn)).await
        }
        Expr::App {
            func,
            args,
            meta,
            type_args,
        } => match (func.as_ref(), args.as_ref()) {
            (Expr::Lambda(arity, body, _), Expr::ArgsTuple(args, _)) => {
                let pairs: Vec<(VarIndex, Expr<ExprMetadata>)> = args
                    .iter()
                    .enumerate()
                    .map(|(index, arg)| {
                        (
                            VarIndex {
                                de_bruijn: 0,
                                tuple: index as u32,
                            },
                            arg.clone(),
                        )
                    })
                    .collect::<Vec<_>>();
                let new_body = pairs
                    .iter()
                    .fold(body.as_ref().clone(), |acc, (param, arg)| {
                        subst(&acc, param, arg, env).as_ref().unwrap().clone()
                    });
                Box::pin(beta_reduce(env, &new_body, eval_final_llm_fn)).await
            }
            (Expr::Lambda(arity, body, _), arg) => {
                let args = match arg {
                    Expr::ArgsTuple(args, _) => args.clone(),
                    x => vec![x.clone()],
                };
                let substitutions: Vec<(VarIndex, Expr<ExprMetadata>)> = args
                    .iter()
                    .enumerate()
                    .map(|(index, arg)| {
                        (
                            VarIndex {
                                de_bruijn: 0,
                                tuple: index as u32,
                            },
                            arg.clone(),
                        )
                    })
                    .collect();
                let new_body = substitutions
                    .iter()
                    .fold(body.as_ref().clone(), |acc, (param, arg)| {
                        subst(&acc, param, arg, env).as_ref().unwrap().clone()
                    });
                Box::pin(beta_reduce(env, &new_body, eval_final_llm_fn)).await
            }
            (Expr::LLMFunction(name, arg_names, _), Expr::ArgsTuple(args, _)) => {
                let evaluated_args = eval_args(env, args).await?;

                let params = evaluated_args
                    .into_iter()
                    .zip(arg_names.iter())
                    .map(|(arg, name)| (name.clone(), arg))
                    .collect::<HashMap<_, _>>();
                let args_map = BamlMap::from_iter(params.into_iter());
                let ctx = env
                    .runtime
                    .create_ctx_manager(BamlValue::String("none".to_string()), None);

                let app_span = SerializedSpan::serialize(&expr.meta().0);
                if let Some(tx) = &env.expr_tx {
                    tx.unbounded_send(vec![app_span]).unwrap();
                }
                if eval_final_llm_fn {
                    // TODO: env vars are not supported yet for expressions.
                    let res: anyhow::Result<FunctionResult> = env
                        .runtime
                        .call_function(
                            name.clone(),
                            &args_map,
                            &ctx,
                            None,
                            None,
                            None,
                            env.env_vars.clone(),
                            TripWire::new(None),
                        )
                        .await
                        .0;

                    if let Some(tx) = &env.expr_tx {
                        tx.unbounded_send(vec![]).unwrap();
                    }
                    let val = res?
                        .parsed()
                        .as_ref()
                        .ok_or(anyhow!("Impossible case - empty value in parsed result."))?
                        .as_ref()
                        .map_err(|e| anyhow!("{e}"))?
                        .clone()
                        .0
                        .map_meta(|_| ());
                    Ok(Expr::Atom(val.map_meta(|_| meta.clone())))
                } else {
                    Ok(expr.clone())
                }
            }
            (Expr::FreeVar(name, _), _) => {
                let var_lookup = env
                    .context
                    .get(name)
                    .context(format!("Variable not found: {name:?}"))?;
                let new_app = Expr::App {
                    func: Arc::new(var_lookup.clone()),
                    args: args.clone(),
                    meta: meta.clone(),
                    type_args: type_args.clone(),
                };
                let res = Box::pin(beta_reduce(env, &new_app, eval_final_llm_fn)).await?;
                Ok(res)
            }

            (Expr::Builtin(builtin, builtin_meta), Expr::ArgsTuple(args, _)) => match builtin {
                Builtin::FetchValue => {
                    let evaluated_args = eval_args(env, args).await?;

                    // TODO: Type checking / validation elsewhere.
                    let BamlValue::Class(_, fields) = &evaluated_args[0] else {
                        return Err(anyhow!(
                            "{fetch_value} expects a {request_type} parameter but got: {evaluated_args:?}",
                            fetch_value = builtin::functions::FETCH_VALUE,
                            request_type = builtin::classes::REQUEST,
                        ));
                    };

                    // Builtin meta should be set.
                    let arrow = match builtin_meta.1.as_ref() {
                        Some(TypeIR::Arrow(arrow, _)) => arrow,

                        other => {
                            return Err(anyhow!(
                                "Internal error: {fetch} meta contains no arrow type: {other:?}",
                                fetch = builtin::functions::FETCH_VALUE,
                            ))
                        }
                    };

                    // TODO: Type checking / validation elsewhere.
                    let mut base_url = fields
                        .get("base_url")
                        .map(BamlValue::as_str)
                        .ok_or(anyhow!(
                            "{fetch_value} argument has no 'base_url' field",
                            fetch_value = builtin::functions::FETCH_VALUE
                        ))?
                        .ok_or(anyhow!("Can't convert 'base_url' to string"))?;

                    let headers = fields
                        .get("headers")
                        .map(BamlValue::as_map)
                        .ok_or(anyhow!(
                            "{fetch_value} argument has no 'headers' field",
                            fetch_value = builtin::functions::FETCH_VALUE
                        ))?
                        .ok_or(anyhow!("Can't convert 'headers' to map"))?;

                    let query_params = fields
                        .get("query_params")
                        .map(BamlValue::as_map)
                        .ok_or(anyhow!(
                            "{fetch_value} argument has no 'query_params' field",
                            fetch_value = builtin::functions::FETCH_VALUE
                        ))?
                        .ok_or(anyhow!("Can't convert 'query_params' to map"))?;

                    let mut header_map = reqwest::header::HeaderMap::new();
                    for (key, value) in headers {
                        header_map.insert(
                            reqwest::header::HeaderName::from_str(key)?,
                            reqwest::header::HeaderValue::from_str(value.as_str().ok_or(
                                anyhow!("Can't convert header value to string: {:?}", value),
                            )?)?,
                        );
                    }

                    // TODO: There's some code that handles proxy URL extraction
                    // better in baml-lib/llm-client/src/clients/helpers.rs
                    // use that here.
                    if let Some(proxy_url) = env.env_vars.get("BOUNDARY_PROXY_URL") {
                        header_map.insert(
                            reqwest::header::HeaderName::from_static("baml-original-url"),
                            reqwest::header::HeaderValue::from_str(base_url)?,
                        );
                        base_url = proxy_url;
                    }

                    // Highlight.
                    let app_span = SerializedSpan::serialize(&expr.meta().0);
                    if let Some(tx) = &env.expr_tx {
                        tx.unbounded_send(vec![app_span]).unwrap();
                    }

                    let client = reqwest::Client::new();

                    // eprintln!(
                    //     "Sending HTTP request: {:?}",
                    //     client
                    //         .get(base_url)
                    //         .query(query_params)
                    //         .headers(header_map.clone())
                    // );

                    let response = client
                        .get(base_url)
                        .query(query_params)
                        .headers(header_map)
                        .send()
                        .await?;

                    let status = response.status();

                    let body = response.text().await?;

                    if status.is_client_error() || status.is_server_error() {
                        return Err(anyhow!(
                            "HTTP request failed: HTTP {:?}\nBody: {}",
                            status,
                            body
                        ));
                    }

                    // TODO: If the lines above fail (? operator) then this
                    // won't run. We need to wrap this function in another
                    // function that empties the channel no matter if beta
                    // reduction succeeds or fails.
                    if let Some(tx) = &env.expr_tx {
                        tx.unbounded_send(vec![]).unwrap();
                    }

                    let output_format = render_output_format(
                        &env.runtime.inner.ir,
                        &arrow.return_type,
                        &EvaluationContext::default(),
                        baml_types::StreamingMode::NonStreaming,
                    )?;

                    let parsed = jsonish::from_str(&output_format, &arrow.return_type, &body, true)
                        .context("(jsonish) Failed parsing response of fetch_value call")?;

                    Ok(Expr::Atom(
                        BamlValueWithMeta::<Vec<Flag>>::from(parsed).map_meta(|_| meta.clone()),
                    ))
                }
            },

            _ => Err(anyhow!("Not a function: {:?}", func)),
        },
        Expr::FreeVar(name, _) => {
            if let Some(cached) = env.evaluated_cache.lock().unwrap().get(name) {
                return Ok(cached.clone());
            }

            let var_lookup = env
                .context
                .get(name)
                .context(format!("Variable not found: {name:?}"))?;

            // Evaluate the expression
            let evaluated = Box::pin(beta_reduce(env, var_lookup, eval_final_llm_fn)).await?;

            // Cache the result
            env.evaluated_cache
                .lock()
                .unwrap()
                .insert(name.clone(), evaluated.clone());

            Ok(evaluated)
        }
        Expr::BoundVar(_, _) => Ok(expr.clone()),
        Expr::List(_, _) => Ok(expr.clone()),
        Expr::Map(_, _) => Ok(expr.clone()),
        Expr::ClassConstructor { .. } => Ok(expr.clone()),
        Expr::ArgsTuple(_, _) => Ok(expr.clone()),
        Expr::Lambda(_, _, _) => Ok(expr.clone()),
        Expr::If(cond, then, else_, meta) => {
            let new_cond = Box::pin(beta_reduce(env, cond, eval_final_llm_fn)).await?;
            let new_then = Box::pin(beta_reduce(env, then, eval_final_llm_fn)).await?;
            let new_else = match else_ {
                None => None,
                Some(else_) => {
                    let new_else = Box::pin(beta_reduce(env, else_, eval_final_llm_fn)).await?;
                    Some(Arc::new(new_else))
                }
            };
            Ok(Expr::If(
                Arc::new(new_cond),
                Arc::new(new_then),
                new_else,
                meta.clone(),
            ))
        }
        Expr::ForLoop {
            item,
            iterable,
            body,
            meta,
        } => match iterable.as_ref() {
            Expr::List(iterable_items, meta) => {
                let new_index = VarIndex {
                    de_bruijn: 0,
                    tuple: 0,
                };
                let closed_body = body.close(&new_index, item);
                let unevaluated_results = iterable_items
                    .iter()
                    .map(|iterable_item| subst(&closed_body, &new_index, iterable_item, env))
                    .collect::<anyhow::Result<Vec<_>>>()?;
                Ok(Expr::List(unevaluated_results, meta.clone()))
            }
            _ => {
                let new_iterable = Box::pin(beta_reduce(env, iterable, eval_final_llm_fn)).await?;
                let new_body = Box::pin(beta_reduce(env, body, eval_final_llm_fn)).await?;
                Ok(Expr::ForLoop {
                    item: item.clone(),
                    iterable: Arc::new(new_iterable),
                    body: Arc::new(new_body),
                    meta: meta.clone(),
                })
            }
        },
        _ => panic!("Tried to beta reduce a {}", expr.dump_str()), // Err(anyhow::anyhow!("Not an application: {:?}", expr)),
    }
}

async fn eval_args(
    env: &EvalEnv<'_>,
    args: &Vec<Expr<(internal_baml_core::ast::Span, Option<TypeIR>)>>,
) -> anyhow::Result<Vec<BamlValue>> {
    let mut evaluated_args: Vec<BamlValue> = Vec::new();
    for arg in args {
        let val = eval_to_value(env, arg).await?;
        evaluated_args.push(val.unwrap().clone().value());
    }
    Ok(evaluated_args)
}

fn resolve_env_variable<'a>(
    env: &EvalEnv<'a>,
    var_name: &str,
) -> anyhow::Result<BamlValueWithMeta<()>> {
    match env.env_vars.get(var_name) {
        Some(value) => Ok(BamlValueWithMeta::String(value.clone(), ())),
        None => Err(anyhow!("Environment variable '{var_name}' not found")),
    }
}

pub async fn eval_to_value_or_llm_call<'a>(
    env: &EvalEnv<'a>,
    expr: &Expr<ExprMetadata>,
) -> anyhow::Result<ExprEvalResult> {
    let mut current_expr = expr.clone();

    for steps in 0..MAX_STEPS {
        match current_expr {
            Expr::App {
                ref func,
                ref args,
                ref meta,
                ref type_args,
            } => match (func.as_ref(), args.as_ref()) {
                (Expr::LLMFunction(name, arg_names, _), Expr::ArgsTuple(args, _)) => {
                    let mut evaluated_args: Vec<(String, BamlValue)> = Vec::new();
                    for (arg_name, arg) in arg_names.iter().zip(args) {
                        let val = eval_to_value(env, arg).await;
                        evaluated_args
                            .push((arg_name.clone(), val.unwrap().unwrap().clone().value()));
                    }
                    let res = ExprEvalResult::LLMCall {
                        name: name.clone(),
                        args: BamlMap::from_iter(evaluated_args.into_iter()),
                    };
                    return Ok(res);
                }
                _ => {
                    let res = Box::pin(beta_reduce(env, &current_expr, false)).await?;
                    current_expr = res;
                }
            },
            Expr::Atom(value) => {
                return Ok(ExprEvalResult::Value {
                    value: value.clone().map_meta(|_| ()),
                    field_type: TypeIR::Primitive(TypeValue::Null, TypeMeta::default()), // TODO: get the actual type
                });
            }
            Expr::List(items, meta) => {
                let mut new_items = Vec::new();
                for item in items {
                    let val = Box::pin(eval_to_value(env, &item))
                        .await?
                        .context("Evaluated value to None")?;
                    new_items.push(val);
                }
                let val = BamlValueWithMeta::List(new_items, ());
                return Ok(ExprEvalResult::Value {
                    value: val,
                    field_type: meta
                        .1
                        .clone()
                        .unwrap_or(TypeIR::Primitive(TypeValue::Null, TypeMeta::default())), // TODO: get the actual type
                });
            }
            Expr::Map(items, meta) => {
                let mut new_items = BamlMap::new();
                for (key, value) in items {
                    let val = Box::pin(eval_to_value(env, &value))
                        .await?
                        .context("Evaluated value to None")?;
                    new_items.insert(key.clone(), val);
                }
                let val = BamlValueWithMeta::Map(new_items, ());
                return Ok(ExprEvalResult::Value {
                    value: val,
                    field_type: meta
                        .1
                        .clone()
                        .unwrap_or(TypeIR::Primitive(TypeValue::Null, TypeMeta::default())), // TODO: get the actual type
                });
            }
            Expr::ClassConstructor {
                name,
                fields,
                spread,
                meta,
            } => {
                let mut new_fields = BamlMap::new();
                for (key, value) in fields {
                    let val = Box::pin(eval_to_value(env, &value))
                        .await?
                        .context("Evaluated value to None")?;
                    new_fields.insert(key.clone(), val);
                }
                let mut spread_fields = match spread {
                    Some(spread) => {
                        let res = Box::pin(eval_to_value(env, spread.as_ref())).await?;
                        match res {
                            Some(BamlValueWithMeta::Class(spread_class_name, spread_fields, _)) => {
                                if name != spread_class_name {
                                    return Err(anyhow!("Class constructor name mismatch"));
                                }
                                spread_fields.clone()
                            }
                            _ => {
                                return Err(anyhow!("Spread is not a class"));
                            }
                        }
                    }
                    None => BamlMap::new(),
                };
                spread_fields.extend(new_fields);
                let val = BamlValueWithMeta::Class(name.clone(), spread_fields, ());
                return Ok(ExprEvalResult::Value {
                    value: val,
                    field_type: TypeIR::class(name),
                });
            }
            Expr::LLMFunction(_, _, _) => {
                return Err(anyhow!("Bare LLM function found"));
            }
            Expr::Lambda(_, _, _) => {
                return Err(anyhow!("Bare lambda found: {}", expr.dump_str()));
            }
            Expr::Builtin(builtin, meta) => match builtin {
                Builtin::FetchValue => {
                    return Err(anyhow!(
                        "Bare builtin fetch_value found: {}",
                        expr.dump_str()
                    ));
                }
            },
            Expr::Let(var_name, value, body, meta) => {
                let res = beta_reduce(env, expr, false).await?;
                if res.temporary_same_state(expr) {
                    return Err(anyhow!("Failed to make progress"));
                }
                current_expr = res;
            }
            Expr::If(cond, then, else_, meta) => {
                let predicate = eval_to_value(env, cond.as_ref()).await?;
                match predicate {
                    Some(BamlValueWithMeta::Bool(predicate, _)) => {
                        if predicate {
                            current_expr = Arc::unwrap_or_clone(then.clone());
                        } else {
                            current_expr = else_
                                .as_ref()
                                .map(|e| Arc::unwrap_or_clone(e.clone()))
                                .unwrap_or(Expr::Atom(BamlValueWithMeta::Null(meta.clone())));
                        }
                    }
                    _ => todo!("Type error"),
                }
            }
            Expr::BoundVar(_, _) => {
                return Err(anyhow!("Bare bound variable found"));
            }
            Expr::FreeVar(_, _) => {
                return Err(anyhow!("Bare free variable found"));
            }
            Expr::ArgsTuple(_, _) => {
                return Err(anyhow!("Bare args tuple found"));
            }
            l @ Expr::ForLoop { .. } => {
                let res = Box::pin(beta_reduce(env, &l, false)).await?;
                current_expr = res;
            }
            Expr::ArrayAccess { base, index, meta } => {
                let base_val = eval_to_value(env, base.as_ref()).await?;
                let index_val = eval_to_value(env, index.as_ref()).await?;

                match (base_val, index_val) {
                    (
                        Some(BamlValueWithMeta::List(items, _)),
                        Some(BamlValueWithMeta::Int(idx, _)),
                    ) => {
                        if idx < 0 || idx as usize >= items.len() {
                            return Err(anyhow!("Array index out of bounds: {}", idx));
                        }
                        let val = items[idx as usize].clone();
                        return Ok(ExprEvalResult::Value {
                            value: val,
                            field_type: meta
                                .1
                                .clone()
                                .unwrap_or(TypeIR::Primitive(TypeValue::Null, TypeMeta::default())),
                        });
                    }
                    (Some(BamlValueWithMeta::Map(map, _)), Some(index_val)) => {
                        let key = match index_val {
                            BamlValueWithMeta::String(s, _) => s,
                            _ => return Err(anyhow!("Map index must be a string")),
                        };

                        match map.get(&key) {
                            Some(val) => {
                                return Ok(ExprEvalResult::Value {
                                    value: val.clone(),
                                    field_type: meta.1.clone().unwrap_or(TypeIR::Primitive(
                                        TypeValue::Null,
                                        TypeMeta::default(),
                                    )),
                                });
                            }
                            None => return Err(anyhow!("Map key not found: {}", key)),
                        }
                    }
                    _ => return Err(anyhow!("Invalid array/map access")),
                }
            }
            Expr::FieldAccess { base, field, meta } => {
                let base_val = eval_to_value(env, base.as_ref()).await?;

                match base_val {
                    Some(BamlValueWithMeta::Class(_, fields, _)) => match fields.get(&field) {
                        Some(val) => {
                            return Ok(ExprEvalResult::Value {
                                value: val.clone(),
                                field_type: meta.1.clone().unwrap_or(TypeIR::Primitive(
                                    TypeValue::Null,
                                    TypeMeta::default(),
                                )),
                            });
                        }
                        None => return Err(anyhow!("Field not found: {}", field)),
                    },
                    _ => return Err(anyhow!("Field access requires a class type")),
                }
            }
            Expr::BinaryOperation {
                left,
                operator,
                right,
                meta,
            } => {
                let left = eval_to_value(env, left.as_ref()).await?;
                let right = eval_to_value(env, right.as_ref()).await?;

                todo!("impl eval to value for binary operation");
            }

            Expr::UnaryOperation {
                expr,
                operator,
                meta,
            } => {
                let expr = eval_to_value(env, expr.as_ref()).await?;

                todo!("impl eval to value for unary operation");
            }
        }
    }
    Err(anyhow!("Max steps reached. {:?}", current_expr))
}

#[derive(Clone, Debug)]
pub enum ExprEvalResult {
    Value {
        value: BamlValueWithMeta<()>,
        field_type: TypeIR,
    },
    LLMCall {
        name: String,
        args: BamlMap<String, BamlValue>,
    },
}

/// Fully evaluate an expression to a value.
pub async fn eval_to_value<'a>(
    env: &EvalEnv<'a>,
    expr: &Expr<ExprMetadata>,
) -> anyhow::Result<Option<BamlValueWithMeta<()>>> {
    let mut current_expr = expr.clone();

    for steps in 0..MAX_STEPS {
        match current_expr {
            Expr::Atom(value) => return Ok(Some(value.clone().map_meta(|_| ()))),
            Expr::List(items, meta) => {
                let mut new_items = Vec::new();
                for item in items {
                    let val = Box::pin(eval_to_value(env, &item))
                        .await?
                        .context("Evaluated value to None")?;
                    new_items.push(val);
                }
                let val = BamlValueWithMeta::List(new_items, ());
                return Ok(Some(val));
            }
            Expr::Map(items, meta) => {
                let mut new_items = BamlMap::new();
                for (key, value) in items {
                    let val = Box::pin(eval_to_value(env, &value))
                        .await?
                        .context("Evaluated value to None")?;
                    new_items.insert(key.clone(), val);
                }
                return Ok(Some(BamlValueWithMeta::Map(new_items, ())));
            }
            Expr::ClassConstructor {
                name,
                fields,
                spread,
                meta,
            } => {
                let mut new_fields = BamlMap::new();
                for (key, value) in fields {
                    let val = Box::pin(eval_to_value(env, &value))
                        .await?
                        .context("Evaluated value to None")?;
                    new_fields.insert(key.clone(), val);
                }
                let mut spread_fields = match spread {
                    Some(spread) => {
                        let res = Box::pin(eval_to_value(env, spread.as_ref())).await?;
                        match res {
                            Some(BamlValueWithMeta::Class(spread_class_name, spread_fields, _)) => {
                                if name != spread_class_name {
                                    return Err(anyhow!("Class constructor name mismatch"));
                                }
                                spread_fields.clone()
                            }
                            _ => {
                                return Err(anyhow!("Spread is not a class"));
                            }
                        }
                    }
                    None => BamlMap::new(),
                };

                spread_fields.extend(new_fields);
                let val = BamlValueWithMeta::Class(name.clone(), spread_fields, ());
                return Ok(Some(val));
            }
            Expr::If(cond, then, else_, meta) => {
                let predicate = Box::pin(eval_to_value(env, cond.as_ref())).await?;
                match predicate {
                    Some(BamlValueWithMeta::Bool(predicate, _)) => {
                        if predicate {
                            current_expr = Arc::unwrap_or_clone(then.clone());
                        } else {
                            current_expr = else_
                                .as_ref()
                                .map(|e| Arc::unwrap_or_clone(e.clone()))
                                .unwrap_or(Expr::Atom(BamlValueWithMeta::Null(meta.clone())));
                        }
                    }
                    _ => todo!("Type error"),
                }
            }
            // Expr::ForLoop{ item, iterable, body, meta } => {
            //     match iterable.as_ref() {
            //         Expr::List(items, _ ) => {
            //             let mut results: Vec<BamlValueWithMeta<()>> = Vec::new();
            //             for i in items {
            //                 let result = Box::pin(eval_to_value(i))
            //             }
            //         }
            //     }
            // }
            other => {
                // let new_expr = step(env, &other).await?;
                let new_expr = Box::pin(beta_reduce(env, &other, true)).await?;

                if new_expr.temporary_same_state(expr) {
                    return Err(anyhow!("Failed to make progress."));
                }
                current_expr = new_expr;
            }
        }
    }
    Err(anyhow!("Max steps reached."))
}

#[cfg(test)]
mod tests {
    use baml_types::{BamlMap, BamlValue};
    use futures::channel::mpsc;
    use internal_baml_core::{
        ir::{repr::make_test_ir, IRHelper},
        FeatureFlags,
    };

    use super::*;
    use crate::{internal_baml_diagnostics::Span, BamlRuntime};

    // Make a testing runtime. It assumes the presence of
    // OPENAI_API_KEY environment variable.
    fn runtime(content: &str) -> BamlRuntime {
        let openai_api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY is not set.");
        BamlRuntime::from_file_content(
            ".",
            &HashMap::from([("main.baml", content)]),
            HashMap::from([("OPENAI_API_KEY", openai_api_key.as_str())]),
            FeatureFlags::new(),
        )
        .unwrap()
    }

    // #[tokio::test] // Uncomment to run.
    async fn test_eval_expr() {
        let rt = runtime(
            r##"
function MakePoem(length: int) -> string {
    client GPT4o
    prompt #"Write a poem {{ length }} lines long."#
}

function CombinePoems(poem1: string, poem2: string) -> string {
    client GPT4o
    prompt #"Combine the following two poems into one poem.

    Poem 1:
    {{ poem1 }}

    Poem 2:
    {{ poem2 }}
    "#
}

let poem = MakePoem(1);

let another = {
  let x = MakePoem(2);
  let y = MakePoem(3);
  CombinePoems(x,y)
};

function Pipeline() -> string {
    let x = MakePoem(4);
    let y = MakePoem(5);
    let a = MakePoem(6);
    let b = MakePoem(6);
    let xy = CombinePoems(x,y);
    let ab = CombinePoems(a,b);
    CombinePoems(xy, ab)
}

function Pyramid() -> string {
  CombinePoems( CombinePoems( MakePoem(10), MakePoem(10)), MakePoem(10))
}

let default_person = Person {
  name: "John Doe",
  age: 20,
  poem: "Never was there a man more plain."
};

class Person {
  name string
  age int
  poem string
}

function MakePerson() -> Person {
  Person { name: "Greg", poem: "Hello, world!", ..default_person }
}

function OuterPyramid() -> string {
  CombinePoems(poem, another)
}

function ExprList() -> string[] {
  [ MakePoem(10), MakePoem(2) ]
}

test TestPipeline() {
  functions [Pipeline]
  args { }
}

test TestPyramid() {
  functions [Pyramid]
  args { }
}

test OuterPyramid() {
  functions [OuterPyramid]
  args { }
}

client<llm> GPT4o {
  provider openai
  options {
    model gpt-4o
    api_key env.OPENAI_API_KEY
  }
}

test TestMakePoem() {
    functions [MakePoem]
    args { length 4 }
}

test TestExprList() {
  functions [ExprList]
  args { }
}

test TestMakePerson() {
  functions [MakePerson]
  args { }
}

function Echo(msg: string) -> string {
    client GPT4o
    prompt #"Please repeat the message back to me exactly as it is: {{ msg }}"#
}

function Quiz(msg: string) -> bool {
  client GPT4o
  prompt #"Is the following message true or false? {{ msg }}
  {{ ctx.output_format }}
  "#
}

function Go() -> string {
  if Quiz("The sky is green") { Echo("Hello") } else { Echo("World") }
}

test Go {
  functions [Go]
  args {}
}

class Poem {
  title string
  body string
}

function PoemAbout(topic: string) -> string {
  client GPT4o
  prompt #"Write a 10-word poem about {{ topic }}"#
}

function Poems() -> Poem[] {
  for (t in ["cats", "birds", "love", "rain"]) {
    Poem {
      title: t,
      body: PoemAbout(t)
    }
  }
}

test Poems {
  functions [Poems]
  args {}
}


        "##,
        );
        // dbg!(&rt.inner.ir.find_function("OuterPyramid").unwrap().item);
        let ctx = rt.create_ctx_manager(BamlValue::String("test".to_string()), None);

        let on_event = |res: FunctionResult| {
            eprintln!("on_event: {res:?}");
        };
        let f = rt.inner.ir.find_expr_fn("OuterPyramid").unwrap();
        dbg!(&f.item);
        let (res, _) = rt
            // .run_test("Second", "TestSecond", &ctx, Some(on_event))
            // .run_test("Go", "Go", &ctx, Some(on_event), None)
            .run_test(
                "Poems",
                "Poems",
                &ctx,
                Some(on_event),
                None,
                HashMap::new(),
                TripWire::new(None),
                None::<Box<dyn Fn()>>,
            )
            // .run_test("MakePerson", "TestMakePerson", &ctx, Some(on_event), None)
            // .run_test("CompareHaikus", "Test", &ctx, Some(on_event))
            // .run_test("LlmParseInt", "TestParse", &ctx, Some(on_event))
            .await;
        dbg!(res);
        assert!(false);
    }

    // #[tokio::test]
    async fn test_fn_stream() {
        let rt = runtime(
            r##"
function MakePoem(length: int) -> string {
    client GPT4o
    prompt #"Write a poem {{ length }} lines long."#
}

function CombinePoems(poem1: string, poem2: string) -> string {
    client GPT4o
    prompt #"Combine the following two poems into one poem.

    Poem 1:
    {{ poem1 }}

    Poem 2:
    {{ poem2 }}
    "#
}

let poem = MakePoem(1);

let another = {
  let x = MakePoem(2);
  let y = MakePoem(3);
  CombinePoems(x,y)
};

function Pipeline() -> string {
    let x = MakePoem(4);
    let y = MakePoem(5);
    let a = MakePoem(6);
    let b = MakePoem(7);
    let xy = CombinePoems(x,y);
    let ab = CombinePoems(a,b);
    CombinePoems(xy, ab)
}

function Pyramid() -> string {
  CombinePoems( CombinePoems( MakePoem(8), MakePoem(9)), MakePoem(10))
}

let default_person = Person {
  name: "John Doe",
  age: 20,
  poem: "Never was there a man more plain."
};

class Person {
  name string
  age int
  poem string
}

function MakePerson() -> Person {
  Person { name: "Greg", poem: "Hello, world!", ..default_person }
}

function OuterPyramid() -> string {
  CombinePoems(poem, another)
}

function ExprList() -> string[] {
  [ MakePoem(11), MakePoem(12) ]
}

test TestPipeline() {
  functions [Pipeline]
  args { }
}

test TestPyramid() {
  functions [Pyramid]
  args { }
}

test OuterPyramid() {
  functions [OuterPyramid]
  args { }
}

client<llm> GPT4o {
  provider openai
  options {
    model gpt-4o
    api_key env.OPENAI_API_KEY
  }
}

test TestMakePoem() {
    functions [MakePoem]
    args { length 4 }
}

test TestExprList() {
  functions [ExprList]
  args { }
}

test TestMakePerson() {
  functions [MakePerson]
  args { }
}
        "##,
        );
        // dbg!(&rt.inner.ir.find_function("OuterPyramid").unwrap().item);
        let ctx = rt.create_ctx_manager(BamlValue::String("test".to_string()), None);

        let on_event = |res: FunctionResult| {
            eprintln!("on_event: {res:?}");
        };
        let f = rt.inner.ir.find_expr_fn("OuterPyramid").unwrap();
        // dbg!(&f.item);
        let (res, _) = rt
            // .run_test("Second", "TestSecond", &ctx, Some(on_event))
            .run_test(
                "OuterPyramid",
                "OuterPyramid",
                &ctx,
                Some(on_event),
                None,
                HashMap::new(),
                TripWire::new(None),
                None::<Box<dyn Fn()>>,
            )
            // .run_test("MakePerson", "TestMakePerson", &ctx, Some(on_event), None)
            // .run_test("CompareHaikus", "Test", &ctx, Some(on_event))
            // .run_test("LlmParseInt", "TestParse", &ctx, Some(on_event))
            .await;
        dbg!(res);
        assert!(false);
    }

    // #[tokio::test]
    async fn test_fetch_value() {
        let rt = runtime(
            r##"
class Todo {
  id int
  todo string
  completed bool
  userId int
}

fn GetTodo() -> Todo {
  std::fetch_value<Todo>(std::Request {
    base_url: "https://dummyjson.com/todos/1",
    headers: {},
    query_params: {},
  })
}

fn UseFunction() -> string {
  let todo = GetTodo();
  LlmDescribeTodo(todo)
}

client<llm> GPT4o {
  provider openai
  options {
    model gpt-4o
    api_key env.OPENAI_API_KEY
  }
}

function LlmDescribeTodo(todo: Todo) -> string {
  client GPT4o
  prompt #"Describe the following todo in detail: {{ todo }}"#
}

test GetTodo() {
  functions [GetTodo]
  args { }
}

test UseFunction() {
  functions [UseFunction]
  args { }
}
        "##,
        );
        // dbg!(&rt.inner.ir.find_function("GetTodo").unwrap().item);
        let ctx = rt.create_ctx_manager(BamlValue::String("test".to_string()), None);

        let on_event = |res: FunctionResult| {
            eprintln!("on_event: {res:?}");
        };
        let f = rt.inner.ir.find_expr_fn("UseFunction").unwrap();
        // dbg!(&f.item);
        let (res, _) = rt
            .run_test(
                "UseFunction",
                "UseFunction",
                &ctx,
                Some(on_event),
                None,
                HashMap::from([(
                    "OPENAI_API_KEY".to_string(),
                    std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY is not set."),
                )]),
                TripWire::new(None),
                None::<Box<dyn Fn()>>,
            )
            .await;
        dbg!(res);
        assert!(false);
    }
}
