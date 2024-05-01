use serde_json::json;

use super::*;
use crate::runtime_interface::RuntimeConstructor;
use crate::types::TestStatus;
use crate::{FunctionResult, RuntimeContext, RuntimeInterface, TestResponse};
use std::collections::HashMap;
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
    let mut runtime = InternalBamlRuntime::from_directory(&directory).unwrap();

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
    let mut runtime = InternalBamlRuntime::from_directory(&directory).unwrap();

    // to actually run this you need to replace the OPENAI_API_KEY value with the actual key
    let ctx = RuntimeContext::new().add_env("OPENAI_API_KEY".into(), "OPENAI_API_KEY".to_string());

    let mut params = HashMap::new();
    params.insert(
        "input".to_string(),
        json!("Attention Is All You Need. Mark. Hello."),
    );
    // let mut params = BamlArgType::Map(IndexMap::from([(
    //     "input".to_string(),
    //     BamlArgType::String("\"Attention Is All You Need\" is a landmark[1][2] 2017 research paper by Google.[3] Authored by eight scientists, it was responsible for expanding 2014 attention mechanisms proposed by Bahdanau et. al. into a new deep learning architecture known as the transformer. The paper is considered by some to be a founding document for modern artificial intelligence, as transformers became the main architecture of large language models.[4][5] At the time, the focus of the research was on improving Seq2seq techniques for machine translation, but even in their paper the authors saw the potential for other tasks like question answering and for what is now called multimodal Generative AI.\n\nThe paper's title is a reference to the song \"All You Need Is Love\" by the Beatles.[6]\n\nAs of 2024, the paper has been cited more than 100,000 times.[7]".to_string()),
    // )]));

    let res = runtime
        .call_function("ExtractNames".to_string(), params, &ctx)
        .await?;

    println!("Result: {}", res);

    Ok(res)
}
