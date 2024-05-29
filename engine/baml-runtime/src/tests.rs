use baml_types::BamlValue;

use super::*;
use crate::runtime_interface::RuntimeConstructor;
use crate::types::TestStatus;
use crate::{
    FunctionResult, InternalRuntimeInterface, RuntimeContext, RuntimeInterface, TestResponse,
};

use std::path::PathBuf;

fn assert_passed(test: &TestResponse) {
    assert_eq!(test.status(), TestStatus::Pass);
}

fn assert_failed(test: &TestResponse) {
    assert_ne!(test.status(), TestStatus::Pass);
}

#[test_log::test]
fn test_graph_test() -> Result<()> {
    let directory = PathBuf::from("/Users/vbv/repos/gloo-lang/integ-tests/baml_src");
    let runtime = BamlRuntime::from_directory(
        &directory,
        [
            ("OPENAI_API_KEY", "API_KEY"),
            ("ANTHROPIC_API_KEY", "API_KEY"),
        ]
        .into_iter()
        .collect(),
    )?;

    let ctx = runtime.create_ctx_manager(BamlValue::String("none".to_string()));
    let ctx = ctx.create_ctx();
    let graph = runtime.inner.orchestration_graph("GPT4Turbo", &ctx)?;
    for node in graph.iter() {
        log::info!("Node: {:#}", node);
    }

    for name in [
        "GPT4Turbo",
        "Resilient_SimpleSyntax",
        "Lottery_SimpleSyntax",
    ]
    .iter()
    {
        log::info!("Graph: {}", name);
        let graph = runtime.inner.orchestration_graph(name, &ctx)?;
        for node in graph.iter() {
            log::info!("Node: {:#}", node);
        }
        log::info!("Graph: ----");
    }

    Ok(())
}

#[tokio::test]
async fn test_run_test() -> Result<()> {
    let directory = PathBuf::from("/Users/vbv/repos/gloo-lang/integ-tests/baml_src");
    // let ctx = RuntimeContext::new().add_env("OPENAI_API_KEY".into(), "API_KEY".to_string());
    let runtime = BamlRuntime::from_directory(
        &directory,
        [("OPENAI_API_KEY", "API_KEY")].into_iter().collect(),
    )
    .unwrap();
    let ctx = runtime.create_ctx_manager(BamlValue::String("none".to_string()));
    let (res, _) = runtime
        .run_test("ExtractNames", "pale_maroon", &ctx, Some(|_| {}))
        .await;
    let res = res?;

    assert_passed(&res);
    Ok(())
}

#[tokio::test]
async fn test_call_function() -> Result<FunctionResult> {
    let directory = PathBuf::from("/Users/aaronvillalpando/Projects/baml/integ-tests/baml_src");
    let runtime = BamlRuntime::from_directory(
        &directory,
        [("OPENAI_API_KEY", "OPENAI_API_KEY")].into_iter().collect(),
    )?;
    let ctx = runtime.create_ctx_manager(BamlValue::String("none".to_string()));

    let mut params = baml_types::BamlMap::new();
    params.insert(
        "input".to_string(),
        BamlValue::String("Attention Is All You Need. Mark. Hello.".into()),
    );

    let (res, _) = runtime
        .call_function("ExtractNames".to_string(), &params, &ctx)
        .await;
    let res = res?;

    log::info!("Result: {}", res);

    Ok(res)
}
