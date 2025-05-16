use anyhow::Context;
use futures::channel::mpsc;
use futures::stream::{self as stream, StreamExt};
use internal_baml_core::internal_baml_diagnostics::SerializedSpan;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::{BamlRuntime, FunctionResult};
use baml_types::expr::{Expr, ExprMetadata, Name, VarIndex};
use baml_types::{Arrow, FieldType, TypeValue};
use baml_types::{BamlMap, BamlValue, BamlValueWithMeta};
use internal_baml_core::ir::repr::IntermediateRepr;

const MAX_STEPS: usize = 1000;

pub struct EvalEnv<'a> {
    pub context: HashMap<Name, Expr<ExprMetadata>>,
    pub runtime: &'a BamlRuntime,
    pub expr_tx: Option<mpsc::UnboundedSender<Vec<SerializedSpan>>>,
    /// Evaluated top-level expressions.
    pub evaluated_cache: Arc<Mutex<HashMap<Name, Expr<ExprMetadata>>>>,
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
    env: &EvalEnv<'a>,
) -> anyhow::Result<Expr<ExprMetadata>> {
    let res: anyhow::Result<Expr<ExprMetadata>> = match expr {
        Expr::BoundVar(expr_var_name, _) => {
            if expr_var_name == var_name {
                Ok(val.clone())
            } else {
                Ok(expr.clone())
            }
        }
        Expr::FreeVar(name, _) => Ok(expr.clone()),
        Expr::Atom(_) => Ok(expr.clone()),
        Expr::App(f, x, meta) => {
            let f2 = subst(f, var_name, val, env)?;
            let x2 = subst(x, var_name, val, env)?;
            Ok(Expr::App(Arc::new(f2), Arc::new(x2), meta.clone()))
        }
        Expr::Lambda(params, body, meta) => Ok(Expr::Lambda(
            params.clone(),
            Arc::new(subst(body, var_name, val, env)?),
            meta.clone(),
        )),
        Expr::ArgsTuple(args, meta) => {
            let mut new_args = Vec::new();
            for arg in args {
                new_args.push(subst(arg, var_name, val, env)?);
            }
            Ok(Expr::ArgsTuple(new_args, meta.clone()))
        }
        Expr::LLMFunction(_, _, _) => Ok(expr.clone()),
        Expr::Let(name, value, body, meta) => {
            let new_value = subst(value, var_name, val, env)?;
            let new_body = subst(body, var_name, val, env)?;
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
                .map(|item| subst(item, var_name, val, env))
                .collect::<anyhow::Result<Vec<_>>>()?;
            Ok(Expr::List(new_items, meta.clone()))
        }
        Expr::Map(items, meta) => {
            let new_items = items
                .iter()
                .map(|(key, value)| {
                    let new_value = subst(value, var_name, val, env)?;
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
                    let new_value = subst(value, var_name, val, env)?;
                    Ok((key.clone(), new_value))
                })
                .collect::<anyhow::Result<BamlMap<_, _>>>()?;
            let new_spread = spread
                .as_ref()
                .map(|spread| {
                    subst(spread, var_name, val, env).map(|spread| Box::new(spread.clone()))
                })
                .transpose()?;
            Ok(Expr::ClassConstructor {
                name: name.clone(),
                fields: new_fields,
                spread: new_spread,
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
        Expr::App(f, x, meta) => match (f.as_ref(), x.as_ref()) {
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
                        subst(&acc, &param, &arg, env).as_ref().unwrap().clone()
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
                        subst(&acc, &param, &arg, env).as_ref().unwrap().clone()
                    });
                Box::pin(beta_reduce(env, &new_body, eval_final_llm_fn)).await
            }
            (Expr::LLMFunction(name, arg_names, _), Expr::ArgsTuple(args, _)) => {
                let mut evaluated_args: Vec<BamlValue> = Vec::new();
                for arg in args {
                    let val = eval_to_value(env, arg).await;
                    evaluated_args.push(val.unwrap().unwrap().clone().value());
                }

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
                    let res: anyhow::Result<FunctionResult> = env
                        .runtime
                        .call_function(name.clone(), &args_map, &ctx, None, None, None)
                        .await
                        .0;

                    if let Some(tx) = &env.expr_tx {
                        tx.unbounded_send(vec![]).unwrap();
                    }
                    let val = res?
                        .parsed()
                        .as_ref()
                        .ok_or(anyhow::anyhow!(
                            "Impossible case - empty value in parsed result."
                        ))?
                        .as_ref()
                        .map_err(|e| anyhow::anyhow!("{e}"))?
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
                    .context(format!("Variable not found: {:?}", name))?;
                let new_app = Expr::App(Arc::new(var_lookup.clone()), x.clone(), meta.clone());
                let res = Box::pin(beta_reduce(env, &new_app, eval_final_llm_fn)).await?;
                Ok(res)
            }
            _ => Err(anyhow::anyhow!("Not a function: {:?}", f)),
        },
        Expr::FreeVar(name, _) => {
            if let Some(cached) = env.evaluated_cache.lock().unwrap().get(name) {
                return Ok(cached.clone());
            }

            let var_lookup = env
                .context
                .get(name)
                .context(format!("Variable not found: {:?}", name))?;

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
        _ => panic!("Tried to beta reduce a {}", expr.dump_str()), // Err(anyhow::anyhow!("Not an application: {:?}", expr)),
    }
}

pub async fn eval_to_value_or_llm_call<'a>(
    env: &EvalEnv<'a>,
    expr: &Expr<ExprMetadata>,
) -> anyhow::Result<ExprEvalResult> {
    let mut current_expr = expr.clone();

    eprintln!("start eval_to_value_or_llm_call:\n{:?}", expr.dump_str());
    for steps in 0..MAX_STEPS {
        eprintln!(
            "loop eval_to_value_or_lm_call: n: {}, current_expr: {}",
            steps,
            current_expr.dump_str()
        );
        match current_expr {
            Expr::App(ref f, ref args, ref meta) => match (f.as_ref(), args.as_ref()) {
                (Expr::LLMFunction(name, arg_names, _), Expr::ArgsTuple(args, _)) => {
                    let mut evaluated_args: Vec<(String, BamlValue)> = Vec::new();
                    for (arg_name, arg) in arg_names.into_iter().zip(args) {
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
                    field_type: FieldType::Primitive(TypeValue::Null), // TODO: get the actual type
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
                        .unwrap_or(FieldType::Primitive(TypeValue::Null)), // TODO: get the actual type
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
                        .unwrap_or(FieldType::Primitive(TypeValue::Null)), // TODO: get the actual type
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
                                    return Err(anyhow::anyhow!("Class constructor name mismatch"));
                                }
                                spread_fields.clone()
                            }
                            _ => {
                                return Err(anyhow::anyhow!("Spread is not a class"));
                            }
                        }
                    }
                    None => BamlMap::new(),
                };
                spread_fields.extend(new_fields);
                let val = BamlValueWithMeta::Class(name.clone(), spread_fields, ());
                return Ok(ExprEvalResult::Value {
                    value: val,
                    field_type: FieldType::Class(name),
                });
            }
            Expr::LLMFunction(_, _, _) => {
                return Err(anyhow::anyhow!("Bare LLM function found"));
            }
            Expr::Lambda(_, _, _) => {
                return Err(anyhow::anyhow!("Bare lambda found: {}", expr.dump_str()));
            }
            Expr::Let(var_name, value, body, meta) => {
                let res = beta_reduce(env, &expr, false).await?;
                if res.temporary_same_state(expr) {
                    return Err(anyhow::anyhow!("Failed to make progress"));
                }
                current_expr = res;
            }
            Expr::BoundVar(_, _) => {
                return Err(anyhow::anyhow!("Bare bound variable found"));
            }
            Expr::FreeVar(_, _) => {
                return Err(anyhow::anyhow!("Bare free variable found"));
            }
            Expr::ArgsTuple(_, _) => {
                return Err(anyhow::anyhow!("Bare args tuple found"));
            }
        }
    }
    Err(anyhow::anyhow!("Max steps reached. {:?}", current_expr))
}

#[derive(Clone, Debug)]
pub enum ExprEvalResult {
    Value {
        value: BamlValueWithMeta<()>,
        field_type: FieldType,
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
                                    return Err(anyhow::anyhow!("Class constructor name mismatch"));
                                }
                                spread_fields.clone()
                            }
                            _ => {
                                return Err(anyhow::anyhow!("Spread is not a class"));
                            }
                        }
                    }
                    None => BamlMap::new(),
                };

                spread_fields.extend(new_fields);
                let val = BamlValueWithMeta::Class(name.clone(), spread_fields, ());
                return Ok(Some(val));
            }
            other => {
                // let new_expr = step(env, &other).await?;
                let new_expr = Box::pin(beta_reduce(env, &other, true)).await?;

                if new_expr.temporary_same_state(expr) {
                    return Err(anyhow::anyhow!("Failed to make progress."));
                }
                current_expr = new_expr;
            }
        }
    }
    Err(anyhow::anyhow!("Max steps reached."))
}

#[cfg(test)]
mod tests {
    use crate::internal_baml_diagnostics::Span;
    use baml_types::{BamlMap, BamlValue};
    use futures::channel::mpsc;
    use internal_baml_core::ir::repr::make_test_ir;
    use internal_baml_core::ir::IRHelper;

    use super::*;
    use crate::BamlRuntime;

    // Make a testing runtime. It assumes the presence of
    // OPENAI_API_KEY environment variable.
    fn runtime(content: &str) -> BamlRuntime {
        let openai_api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY is not set.");
        BamlRuntime::from_file_content(
            ".",
            &HashMap::from([("main.baml", content)]),
            HashMap::from([("OPENAI_API_KEY", openai_api_key.as_str())]),
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
    prompt #"Please repeat the message back to me, with three words of elaboration and a twist: {{ msg }}"#
  }
  
  function Go() -> string {
    Echo("Hello")
  }
  
  test Go {
    functions [Go]
    args {}
  }
        "##,
        );
        // dbg!(&rt.inner.ir.find_function("OuterPyramid").unwrap().item);
        let ctx = rt.create_ctx_manager(BamlValue::String("test".to_string()), None);

        let on_event = |res: FunctionResult| {
            eprintln!("on_event: {:?}", res);
        };
        let f = rt.inner.ir.find_expr_fn("OuterPyramid").unwrap();
        dbg!(&f.item);
        let (res, _) = rt
            // .run_test("Second", "TestSecond", &ctx, Some(on_event))
            .run_test("Go", "Go", &ctx, Some(on_event), None)
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
            eprintln!("on_event: {:?}", res);
        };
        let f = rt.inner.ir.find_expr_fn("OuterPyramid").unwrap();
        // dbg!(&f.item);
        let (res, _) = rt
            // .run_test("Second", "TestSecond", &ctx, Some(on_event))
            .run_test("OuterPyramid", "OuterPyramid", &ctx, Some(on_event), None)
            // .run_test("MakePerson", "TestMakePerson", &ctx, Some(on_event), None)
            // .run_test("CompareHaikus", "Test", &ctx, Some(on_event))
            // .run_test("LlmParseInt", "TestParse", &ctx, Some(on_event))
            .await;
        dbg!(res);
        assert!(false);
    }
}
