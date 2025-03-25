use anyhow::Context;
use futures::channel::mpsc;
use internal_baml_core::internal_baml_diagnostics::SerializedSpan;
use std::collections::HashMap;
use std::sync::Arc;

use crate::{BamlRuntime, FunctionResult};
use baml_types::expr::{Arrow, Expr, ExprType, Name};
use baml_types::{BamlMap, BamlValue, BamlValueWithMeta};
use internal_baml_core::ir::repr::ExprMetadata;
use internal_baml_core::ir::repr::IntermediateRepr;

pub struct EvalEnv<'a> {
    pub context: HashMap<Name, Expr<ExprMetadata, ()>>,
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
    expr: &Expr<ExprMetadata, ()>,
    var_name: &Name,
    val: &Expr<ExprMetadata, ()>,
    env: &EvalEnv<'a>,
) -> anyhow::Result<Expr<ExprMetadata, ()>> {
    // eprintln!(
    //     "SUBST2:\n[{} -> {}] in {:?}",
    //     var_name,
    //     val.dump_str(),
    //     expr
    // );
    let res: anyhow::Result<Expr<ExprMetadata, ()>> = match expr {
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
        Expr::Atom(_, _) => Ok(expr.clone()),
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
    };
    let res = res?;
    // eprintln!(
    //     "SUBST2:\n[{} -> {}] in {:?} ===> {:?}",
    //     var_name,
    //     val.dump_str(),
    //     expr,
    //     res
    // );
    Ok(res)
}

/// Perform a single beta reduction. Note that we ignore env.context
/// here. Only use env for the runtime.
async fn beta_reduce<'a>(
    env: &EvalEnv<'a>,
    expr: &Expr<ExprMetadata, ()>,
) -> anyhow::Result<Expr<ExprMetadata, ()>> {
    // eprintln!("BETA_REDUCE:\n{}\n", expr.dump_str());
    match expr {
        Expr::Atom(_, _) => Ok(expr.clone()),
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
                    // eprintln!("BETA_REDUCE_LAMBDA_RESULT2: {}\n", new_body.dump_str());
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
                    } else {
                        // TODO: Don't panic :)
                        panic!("tx is none");
                    }

                    let res: anyhow::Result<FunctionResult> = env
                        .runtime
                        .call_function(name.clone(), &args_map, &ctx, None, None, None)
                        .await
                        .0;
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

                    if let Some(tx) = &env.expr_tx {
                        tx.unbounded_send(vec![]).unwrap();
                    }
                    // eprintln!("BETA_REDUCE_LLM_RESULT: {:?}\n", val);
                    Ok(Expr::Atom(val, meta.clone()))
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
        _ => Err(anyhow::anyhow!("Not an application: {:?}", expr)),
    }
}

/// Fully evaluate an expression to a value.
pub async fn eval_to_value<'a>(
    env: &EvalEnv<'a>,
    expr: &Expr<ExprMetadata, ()>,
) -> anyhow::Result<Option<BamlValueWithMeta<()>>> {
    // eprintln!("called eval_to_value: {}", expr.dump_str());
    let max_steps = 1000;
    let mut current_expr = expr.clone();

    for steps in 0..max_steps {
        match current_expr {
            Expr::Atom(value, _) => return Ok(Some(value.clone())),
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
    fn runtime(content: &str) -> (BamlRuntime, mpsc::Receiver<Vec<Span>>) {
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
        let (rt, _) = runtime(
            r##"

        client<llm> GPT35 {
          provider baml-openai-chat
          options {
            model gpt-3.5-turbo
            api_key env.OPENAI_API_KEY
          }
        }

        function CountWords(text: string) -> int {
          client GPT35
          prompt #"
          "#
        }

        fn Second(x: string, y: string) -> int {
          let x1 = LlmParseInt(x);
          let y1 = LlmParseInt(y);
          let z1 = Double(y1);
          let a1 = Double(z1);
          a1
        }

        fn DoId(x: int, y: int) -> int {
          let z = Double(x);
          let a = Double(z);
          a
        }

        function Double(my_inp: int) -> int {
          client GPT35
          prompt #"
          Double the following integer:
          {{ my_inp }}

          {{ ctx.output_format}}
          "#
        }

        function LlmParseInt(inp: string) -> int {
          client GPT35
          prompt #"
            Parse the following text as an integer:
            {{ inp }}

            {{ ctx.output_format}}
          "#
        }

        test TestSecond {
          functions [Second]
          args {
            x "123"
            y "456"
          }
        }

        test TestParse {
          functions [LlmParseInt]
          args { inp "123"}
          @@assert({{ this == 124 }})
        }

        enum MyEnum {
          A
          B
          C
        }
        
        
        class Foo {
          my_foo Foo?
        }
        
        class A {
          i int
        }
        
        client<llm> GPT3 {
          provider openai
          options {
            model gpt-4o
            api_key env.OPENAI_API_KEY
          }
        }
        
        enum Color {
          RED
          GREEN
          BLUE
        }
        
        fn First(x: int, y: int) -> int {
          x
        }
        
        
        function DoIt(a: int) -> int {
          client GPT3
          prompt #"
            Just return {{ a }} times 100.
        
            {{ ctx.output_format }}
          "#
        }
        
        test FirstTest {
          functions [First]
          args {
            x 1
            y 2
          }
        }
        
        test FooTest {
          functions [DoIt]
          args {
            a 1
            c RED
            }
        }
        "##,
        );
        let ctx = rt.create_ctx_manager(BamlValue::String("test".to_string()), None);

        // let params = BamlMap::from([(
        //     "inp".to_string(),
        //     BamlValue::String("123".to_string().to_string()),
        // )]);
        // let res = rt
        //     .call_function("LlmParseInt".to_string(), &params, &ctx, None, None)
        //     .await
        //     .0
        //     .unwrap();
        // let res2 = res.parsed().as_ref().unwrap().as_ref().unwrap();
        // dbg!(res2);

        // dbg!(&rt.inner.ir.expr_fns);
        // let fns: Vec<_> = rt
        //     .inner
        //     .ir
        //     .walk_expr_fns()
        //     .into_iter()
        //     .map(|w| (w.item.elem.0.clone(), w.item.elem.1.dump_str()))
        //     .collect::<Vec<_>>();
        // fns.iter()
        //     .for_each(|(name, fn_str)| eprintln!("{}: {}", name, fn_str));

        // let params = BamlMap::from([(
        //     "x".to_string(),
        //     BamlValue::String("123".to_string().to_string()),
        // ), (
        //     "y".to_string(),
        //     BamlValue::String("456".to_string().to_string()),
        // )]);
        // let res3 = rt
        //     .call_function("Second".to_string(), &params, &ctx, None, None)
        //     .await
        //     .0.unwrap();
        // let res4 = res3.parsed().as_ref().unwrap().as_ref().unwrap();
        // dbg!(res4);

        // let params = BamlMap::from([(
        //     "x".to_string(),
        //     BamlValue::Int(888),
        // ), ("y".to_string(), BamlValue::Int(999))]);
        // let res3 = rt
        //     .call_function("DoId".to_string(), &params, &ctx, None, None)
        //     .await
        //     .0.unwrap();
        // let res4 = res3.parsed().as_ref().unwrap().as_ref().unwrap();
        // dbg!(res4);

        let on_event = |res: FunctionResult| {
            eprintln!("on_event: {:?}", res);
        };
        let (res, _) = rt
            // .run_test("Second", "TestSecond", &ctx, Some(on_event))
            .run_test("First", "FirstTest", &ctx, Some(on_event), None)
            // .run_test("CompareHaikus", "Test", &ctx, Some(on_event))
            // .run_test("LlmParseInt", "TestParse", &ctx, Some(on_event))
            .await;
        // dbg!(res);
        assert!(false);
    }

    #[tokio::test]
    async fn test_haikus() {
        let (rt, _) = runtime(
            r##"

               class TwoInts {
                int1 int
                int2 int
              }
              
              function AddThem(two_ints: TwoInts) -> int {
                client GPT3
                prompt #"
                  Add {{ two_ints.int1}} and {{ two_ints.int2 }} together.
                  {{ ctx.output_format }}
                "#
              }
              
              test AddThemTest {
                functions [AddThem]
                args {
                  two_ints {
                    int1 1
                    int2 2
                    }
                }
              }
              
              class BreakResult {
                two_ints TwoInts
                reason_interesting string @description("A reason why this split is interesting")
              }
              
              function BreakThem(inp: int) -> BreakResult {
                client GPT3
                prompt #"
                  Split {{ inp }} into two integers in any way you like.
                  {{ ctx.output_format }}
                "#
              }
              
              test BreakThemTest {
                functions [BreakThem]
                args {
                  inp 123
                }
              }
              
              fn Compose(two_ints: TwoInts) -> BreakResult {
                let z = AddThem(two_ints);
                BreakThem(z)
              }
              
              test ComposeTest {
                functions [Compose]
                args {
                  two_ints {
                    int1 1
                    int2 2
                  }
                }
              }       
        class Comparison {
          haiku1 string
          haiku1_score int
          haiku2 string
          haiku2_score int
          three_reasons string[]
        }
        
        function CompareHaikus(haiku1: string, haiku2: string) -> Comparison {
          client GPT3
          prompt #"
            Compare the following haikus:
            {{ haiku1 }}
            {{ haiku2 }}
            {{ ctx.output_format }}
          "#
        }
        
        fn HaikusForTopic(topic: string) -> Comparison {
          let haiku1 = Haiku35(topic);
          let haiku2 = Haiku4o(topic);
          CompareHaikus(haiku1, haiku2)
        }
        
        test HaikusForTopicTest {
          functions [HaikusForTopic]
          args {
            topic "The sky is blue"
          }
        }
        
        function Haiku35(topic: string) -> string {
          client GPT3
          prompt #"
            Produce a haiku about {{ topic }}"#
        }
        
        function Haiku4o(topic: string) -> string {
          client GPT4o
          prompt #"
            Produce a haiku about {{ topic }}"#
        }
        
        test Haiku35Test {
          functions [Haiku35]
          args {
            topic "The sky is blue"
          }
        }
        
        test Haiku4oTest {
          functions [Haiku4o]
          args {
            topic "The sky is blue"
          }
        }
        

        client<llm> GPT3 {
          provider openai
          options {
            model gpt-3.5-turbo
            api_key env.OPENAI_API_KEY
          }
        }
        
        client<llm> GPT4o {
          provider openai
          options {
            model gpt-4o
            api_key env.OPENAI_API_KEY
          }
        }
      "##,
        );
        eprintln!("ir: {:?}", rt.inner.ir);
        let ctx = rt.create_ctx_manager(BamlValue::String("test".to_string()), None);
        let on_event = |res: FunctionResult| {
            eprintln!("on_event: {:?}", res);
        };
        let (res, _) = rt
            // .run_test("Second", "TestSecond", &ctx, Some(on_event))
            .run_test("Compose", "ComposeTest", &ctx, Some(on_event), None)
            // .run_test("CompareHaikus", "Test", &ctx, Some(on_event))
            // .run_test("LlmParseInt", "TestParse", &ctx, Some(on_event))
            .await;
        dbg!(res);
        assert!(false);
    }

    #[tokio::test]
    async fn test_haikus_2() {
        let (rt, _) = runtime(
            r##"

class TwoInts {
  int1 int
  int2 int
}

function AddThem(two_ints: TwoInts) -> int {
  client GPT3
  prompt #"
    Add {{ two_ints.int1 }} and {{ two_ints.int2 }} together.
    {{ ctx.output_format }}
  "#
}

function BreakThem(inp: int) -> TwoInts {
  client GPT3
  prompt #"
    Split {{ inp }} into two integers in any way you like.
    {{ ctx.output_format }}
  "#
}

fn Third(x: int) -> string {
  x
}


fn Compose(two_ints: TwoInts) -> TwoInts {
  BreakThem( AddThem(two_ints) )
}


test ComposeTest {
  functions [Compose]
  args {
    two_ints {
      int1 23
      int2 12
    }
  }
}

class Comparison {
  haiku1 string
  haiku1_score int
  haiku2 string
  haiku2_score int
  three_reasons string[]
}

function CompareHaikus(haiku1: string, haiku2: string, n_reasons: int) -> Comparison {
  client GPT4o
  prompt #"
    Compare the following haikus:

    {{ haiku1 }}

    {{ haiku2 }}

    Give {{ n_reasons }} reasons why you chose the higher-rated haiku.
    {{ ctx.output_format }}
  "#
}

fn HaikusForTopic(topic: string) -> Comparison {
  let haiku1 = Haiku35(topic);
  let haiku2 = Haiku4o(topic);
  CompareHaikus(haiku1, haiku2)
}

test HaikusForTopicTest {
  functions [HaikusForTopic]
  args {
    topic "The most wonderful thing about Mexico is its men"
  }
}


let some_haiku = "The sky is blue, the grass is green, the sky is blue, the grass is green";

let better_haiku = {
  let topic = "Let's write GPU kernels in BAML";
  Haiku4o(topic)
};

fn UseTopLevelThings(n_reasons: int) -> Comparison {
  CompareHaikus(some_haiku, better_haiku, n_reasons)
}

test UseTopLevelThingsTest {
  functions [UseTopLevelThings]
  args {
    n_reasons 3
  }
}

function Haiku35(topic: string) -> string {
  client GPT3
  prompt #"
    Produce a haiku about {{ topic }}"#
}

function Haiku4o(topic: string) -> string {
  client GPT4o
  prompt #"
    Produce a haiku about {{ topic }}"#
}

test Haiku35Test {
  functions [Haiku35]
  args {
    topic "The sky is blue"
  }
}

test Haiku4oTest {
  functions [Haiku4o]
  args {
    topic "The sky is blue"
  }
}



client<llm> GPT3 {
  provider openai
  options {
    model gpt-3.5-turbo
    api_key env.OPENAI_API_KEY
  }
}


client<llm> GPT4o {
  provider openai
  options {
    model gpt-4o
    api_key env.OPENAI_API_KEY
  }
}
      "##,
        );
        eprintln!("ir: {:?}", rt.inner.ir);
        let ctx = rt.create_ctx_manager(BamlValue::String("test".to_string()), None);
        let on_event = |res: FunctionResult| {
            eprintln!("on_event: {:?}", res);
        };
        let (res, _) = rt
            // .run_test("Compose", "ComposeTest", &ctx, Some(on_event))
            .run_test(
                "UseTopLevelThings",
                "UseTopLevelThingsTest",
                &ctx,
                Some(on_event),
                None,
            )
            .await;
        dbg!(res);
    }
}
