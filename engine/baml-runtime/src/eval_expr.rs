use anyhow::Context;
use futures::channel::mpsc;
use futures::stream::{self as stream, StreamExt};
use internal_baml_core::internal_baml_diagnostics::SerializedSpan;
use std::collections::HashMap;
use std::sync::Arc;

use crate::{BamlRuntime, FunctionResult};
use baml_types::expr::{Expr, ExprMetadata, Name};
use baml_types::Arrow;
use baml_types::{BamlMap, BamlValue, BamlValueWithMeta};
use internal_baml_core::ir::repr::IntermediateRepr;

pub struct EvalEnv<'a> {
    pub context: HashMap<Name, Expr<ExprMetadata>>,
    pub runtime: &'a BamlRuntime,
    pub expr_tx: Option<mpsc::UnboundedSender<Vec<SerializedSpan>>>,
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

fn subst2<'a>(
    expr: &Expr<ExprMetadata>,
    var_name: &Name,
    val: &Expr<ExprMetadata>,
    env: &EvalEnv<'a>,
) -> anyhow::Result<Expr<ExprMetadata>> {
    let res: anyhow::Result<Expr<ExprMetadata>> = match expr {
        Expr::Var(expr_var_name, _) => {
            if expr_var_name == var_name {
                Ok(val.clone())
            } else {
                if let Some(expr_fn) = env.context.get(expr_var_name) {
                    Ok(expr_fn.clone())
                } else {
                    Ok(expr.clone())
                }
            }
        }
        Expr::Atom(_) => Ok(expr.clone()),
        Expr::App(f, x, meta) => {
            let f2 = subst2(f, var_name, val, env)?;
            let x2 = subst2(x, var_name, val, env)?;
            Ok(Expr::App(Arc::new(f2), Arc::new(x2), meta.clone()))
        }
        Expr::Lambda(params, body, meta) => Ok(Expr::Lambda(
            params.clone(),
            Arc::new(subst2(body, var_name, val, env)?),
            meta.clone(),
        )),
        Expr::ArgsTuple(args, meta) => {
            let mut new_args = Vec::new();
            for arg in args {
                new_args.push(subst2(arg, var_name, val, env)?);
            }
            Ok(Expr::ArgsTuple(new_args, meta.clone()))
        }
        Expr::LLMFunction(_, _, _) => Ok(expr.clone()),
        Expr::Let(name, value, body, meta) => {
            if name == var_name {
                // Skip substitution if the let binding shadows the variable.
                Ok(expr.clone())
            } else {
                let new_value = subst2(value, var_name, val, env)?;
                let new_body = subst2(body, var_name, val, env)?;
                Ok(Expr::Let(
                    name.clone(),
                    Arc::new(new_value),
                    Arc::new(new_body),
                    meta.clone(),
                ))
            }
        }
        Expr::List(items, meta) => {
            let new_items = items
                .iter()
                .map(|item| subst2(item, var_name, val, env))
                .collect::<anyhow::Result<Vec<_>>>()?;
            Ok(Expr::List(new_items, meta.clone()))
        }
        Expr::Map(items, meta) => {
            let new_items = items
                .iter()
                .map(|(key, value)| {
                    let new_value = subst2(value, var_name, val, env)?;
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
                    let new_value = subst2(value, var_name, val, env)?;
                    Ok((key.clone(), new_value))
                })
                .collect::<anyhow::Result<BamlMap<_, _>>>()?;
            let new_spread = spread
                .as_ref()
                .map(|spread| {
                    subst2(spread, var_name, val, env).map(|spread| Box::new(spread.clone()))
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
) -> anyhow::Result<Expr<ExprMetadata>> {
    eprintln!("BETA_REDUCE:\n{}\n", expr.dump_str());

    match expr {
        Expr::Atom(_) => Ok(expr.clone()),
        Expr::Let(name, value, body, meta) => {
            // Rewrite the let binding as an application.
            // e.g. (let x = y in f) => (\x y => f)
            let lambda = Expr::Lambda(vec![name.clone()], body.clone(), meta.clone());
            let app = Expr::App(Arc::new(lambda), value.clone(), meta.clone());
            Box::pin(beta_reduce(env, &app)).await
        }
        Expr::App(f, x, meta) => {
            match (f.as_ref(), x.as_ref()) {
                (Expr::Lambda(params, body, _), Expr::ArgsTuple(args, _)) => {
                    // eprintln!("About to beta reduce lambda");
                    let pairs = params
                        .iter()
                        .cloned()
                        .zip(args.iter().cloned())
                        .collect::<Vec<_>>();
                    // dbg!(&pairs);
                    let new_body = pairs
                        .iter()
                        .fold(body.as_ref().clone(), |acc, (param, arg)| {
                            subst2(&acc, &param, &arg, env).as_ref().unwrap().clone()
                        });
                    // eprintln!("BETA_REDUCE_LAMBDA_RESULT1: {}\n", new_body.dump_str());
                    Box::pin(beta_reduce(env, &new_body)).await
                }
                (Expr::Lambda(params, body, _), arg) => {
                    if params.len() != 1 {
                        return Err(anyhow::anyhow!(
                            "Lambda takes exactly one argument: {:?}",
                            expr
                        ));
                    }
                    let new_body = subst2(body, &params[0], &arg, env)
                        .as_ref()
                        .unwrap()
                        .clone();
                    Box::pin(beta_reduce(env, &new_body)).await
                }
                (Expr::LLMFunction(name, arg_names, _), Expr::ArgsTuple(args, _)) => {
                    // dbg!(&args);
                    // let args: Vec<BamlValue> = args.clone().into_iter().map(|arg| arg.as_atom().unwrap().clone().value()).collect();
                    // let evaluated_args: Vec<BamlValue> = args.clone().into_iter().map(|arg| Box::pin(eval_to_value(env, arg).await)).collect::<anyhow::Result<Vec<_>>>()?;

                    let mut evaluated_args: Vec<BamlValue> = Vec::new();
                    for arg in args {
                        let val = eval_to_value(env, arg).await;
                        // eprintln!("BETA_REDUCE_LLM_ARG: {:?}", val);
                        evaluated_args.push(val.unwrap().unwrap().clone().value());
                    }

                    // let evaluated_args = args.clone().into_iter().map(|arg| Box::pin(eval_to_value(env, arg).await)).collect::<anyhow::Result<Vec<_>>>()?;
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
                    // if let Some(tx) = &env.expr_tx {
                    //     tx.unbounded_send(vec![]).unwrap();
                    // }
                    let res: anyhow::Result<FunctionResult> = env
                        .runtime
                        .call_function(name.clone(), &args_map, &ctx, None, None, None)
                        .await
                        .0;

                    if let Some(tx) = &env.expr_tx {
                        tx.unbounded_send(vec![]).unwrap();
                    }
                    let val = res
                        .unwrap()
                        .parsed()
                        .as_ref()
                        .unwrap()
                        .as_ref()
                        .unwrap()
                        .clone()
                        .0
                        .map_meta(|_| ());
                    Ok(Expr::Atom(val.map_meta(|_| meta.clone())))
                }
                (Expr::Var(name, _), _) => {
                    let var_lookup = env
                        .context
                        .get(name)
                        .context(format!("Variable not found: {:?}", name))?;
                    let new_app = Expr::App(Arc::new(var_lookup.clone()), x.clone(), meta.clone());
                    let res = Box::pin(beta_reduce(env, &new_app)).await?;
                    Ok(res)
                }
                _ => Err(anyhow::anyhow!("Not a function: {:?}", f)),
            }
        }
        Expr::Var(name, _) => {
            let var_lookup = env
                .context
                .get(name)
                .context(format!("Variable not found: {:?}", name))?;
            Ok(var_lookup.clone())
        }
        Expr::List(_, _) => Ok(expr.clone()),
        Expr::Map(_, _) => Ok(expr.clone()),
        Expr::ClassConstructor { .. } => Ok(expr.clone()),
        _ => Err(anyhow::anyhow!("Not an application: {:?}", expr)),
    }
}

/// Fully evaluate an expression to a value.
pub async fn eval_to_value<'a>(
    env: &EvalEnv<'a>,
    expr: &Expr<ExprMetadata>,
) -> anyhow::Result<Option<BamlValueWithMeta<()>>> {
    // eprintln!("called eval_to_value: {}", expr.dump_str());
    let max_steps = 1000;
    let mut current_expr = expr.clone();

    for steps in 0..max_steps {
        match current_expr {
            Expr::Atom(value) => return Ok(Some(value.clone().map_meta(|_| ()))),
            Expr::List(items, meta) => {
                let stream: Vec<Result<Option<BamlValueWithMeta<()>>, _>> = stream::iter(
                    items
                        .iter()
                        .map(|value| async move { eval_to_value(env, value).await }),
                )
                .buffered(4)
                .collect::<Vec<Result<_, _>>>()
                .await;
                let new_maybe_items = stream
                    .into_iter()
                    .collect::<Result<Vec<Option<BamlValueWithMeta<()>>>, _>>()?;
                let new_items = new_maybe_items
                    .into_iter()
                    .collect::<Option<Vec<BamlValueWithMeta<()>>>>()
                    .ok_or(anyhow::anyhow!("Failed to evaluate list"))?;
                let val = BamlValueWithMeta::List(new_items, ());
                return Ok(Some(val));
            }
            Expr::Map(items, meta) => {
                let stream: Vec<Result<(String, BamlValueWithMeta<()>), _>> =
                    stream::iter(items.iter().map(|(key, value)| async move {
                        let res = eval_to_value(env, value)
                            .await?
                            .ok_or(anyhow::anyhow!("Failed to evaluate map"))?;
                        Ok((key.clone(), res))
                    }))
                    .buffered(4)
                    .collect::<Vec<Result<(String, BamlValueWithMeta<()>), _>>>()
                    .await;
                let new_items = stream.into_iter().collect::<anyhow::Result<_>>()?;
                return Ok(Some(BamlValueWithMeta::Map(new_items, ())));
            }
            Expr::ClassConstructor {
                name,
                fields,
                spread,
                meta,
            } => {
                dbg!(&fields);
                let stream: Vec<Result<(String, BamlValueWithMeta<()>), anyhow::Error>> =
                    stream::iter(fields.iter().map(|(key, value)| async move {
                        let res = Box::pin(eval_to_value(env, value))
                            .await?
                            .ok_or(anyhow::anyhow!("Failed to evaluate class constructor"))?;
                        Ok((key.clone(), res))
                    }))
                    .buffered(2)
                    .collect::<Vec<Result<(String, BamlValueWithMeta<()>), _>>>()
                    .await;
                let fields = stream
                    .into_iter()
                    .collect::<Result<Vec<(String, BamlValueWithMeta<()>)>, anyhow::Error>>()?;
                dbg!(&fields);

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
                dbg!(&spread_fields);

                spread_fields.extend(fields);
                let val = BamlValueWithMeta::Class(name.clone(), spread_fields, ());
                return Ok(Some(val));
            }
            other => {
                // let new_expr = step(env, &other).await?;
                let new_expr = Box::pin(beta_reduce(env, &other)).await?;

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

    use super::*;
    use crate::BamlRuntime;

    // Make a testing runtime. It assumes the presence of
    // OPENAI_API_KEY environment variable.
    fn runtime(content: &str) -> BamlRuntime {
        let openai_api_key = std::env::var("OPENAI_API_KEY").unwrap();
        BamlRuntime::from_file_content(
            ".",
            &HashMap::from([("main.baml", content)]),
            HashMap::from([("OPENAI_API_KEY", openai_api_key.as_str())]),
        )
        .unwrap()
    }

    #[tokio::test]
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

let poem = MakePoem(10);

let another = {
  let x = MakePoem(10);
  let y = MakePoem(5);
  CombinePoems(x,y)
};

fn Pipeline() -> string {
    let x = MakePoem(6);
    let y = MakePoem(6);
    let a = MakePoem(6);
    let b = MakePoem(6);
    let xy = CombinePoems(x,y);
    let ab = CombinePoems(a,b);
    CombinePoems(xy, ab)
}

fn Pyramid() -> string {
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

fn MakePerson() -> Person {
  Person { name: "Greg", poem: "Hello, world!", ..default_person }
}

fn OuterPyramid() -> string {
  CombinePoems(poem, another)
}

fn ExprList() -> string[] {
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
        "##,
        );
        let ctx = rt.create_ctx_manager(BamlValue::String("test".to_string()), None);

        let on_event = |res: FunctionResult| {
            eprintln!("on_event: {:?}", res);
        };
        let (res, _) = rt
            // .run_test("Second", "TestSecond", &ctx, Some(on_event))
            // .run_test("OuterPyramid", "OuterPyramid", &ctx, Some(on_event), None)
            .run_test("MakePerson", "TestMakePerson", &ctx, Some(on_event), None)
            // .run_test("CompareHaikus", "Test", &ctx, Some(on_event))
            // .run_test("LlmParseInt", "TestParse", &ctx, Some(on_event))
            .await;
        dbg!(res);
        assert!(false);
    }
}
