use baml_types::BamlValue;



use super::*;
use crate::runtime_interface::RuntimeConstructor;
use crate::types::TestStatus;
use crate::{FunctionResult, RuntimeContext, RuntimeInterface, TestResponse};

use std::path::PathBuf;

fn assert_passed(test: &TestResponse) {
    assert_eq!(test.status(), TestStatus::Pass);
}

fn assert_failed(test: &TestResponse) {
    assert_ne!(test.status(), TestStatus::Pass);
}

#[tokio::test]
async fn test_run_test() -> Result<()> {
    let directory = PathBuf::from("/Users/vbv/repos/gloo-lang/integ-tests/baml_src");
    let runtime = InternalBamlRuntime::from_directory(&directory).unwrap();

    let ctx = RuntimeContext::new().add_env("OPENAI_API_KEY".into(), "API_KEY".to_string());

    let res = runtime
        .run_test("ExtractNames", "pale_maroon", &ctx)
        .await?;

    assert_passed(&res);
    Ok(())
}

#[tokio::test]
async fn test_call_function() -> Result<FunctionResult> {
    let directory = PathBuf::from("/Users/aaronvillalpando/Projects/baml/integ-tests/baml_src");
    let runtime = InternalBamlRuntime::from_directory(&directory).unwrap();

    // to actually run this you need to replace the OPENAI_API_KEY value with the actual key
    let ctx = RuntimeContext::new().add_env("OPENAI_API_KEY".into(), "OPENAI_API_KEY".to_string());

    let mut params = baml_types::BamlMap::new();
    params.insert(
        "input".to_string(),
        BamlValue::String("Attention Is All You Need. Mark. Hello.".into()),
    );

    let res = runtime
        .call_function("ExtractNames".to_string(), params, &ctx)
        .await?;

    log::info!("Result: {}", res);

    Ok(res)
}
