pub mod internal;
mod llm_client;
mod prompt_renderer;

use anyhow::Result;
use serde_json::json;
use std::{collections::HashMap, path::PathBuf, process::Termination};

use internal_baml_core::{
    internal_baml_diagnostics::SourceFile,
    ir::{repr::IntermediateRepr, IRHelper},
    validate,
};

use crate::runtime::{
    llm_client::{LLMProvider, WithCallable, WithPrompt},
    prompt_renderer::PromptRenderer,
};
use crate::RuntimeContext;

use self::llm_client::LLMResponse;

pub struct BamlRuntime {
    ir: IntermediateRepr,
}

#[derive(Debug)]
pub struct FunctionResult {
    llm_response: LLMResponse,
    parsed: Option<Result<(serde_json::Value, jsonish::DeserializerConditions)>>,
}

impl FunctionResult {
    pub fn content(&self) -> Option<&str> {
        self.llm_response.content()
    }

    pub fn parsed(&self) -> Option<&serde_json::Value> {
        self.parsed
            .as_ref()
            .and_then(|res| res.as_ref().ok().map(|(val, _)| val))
    }
}

#[derive(Debug)]
pub struct TestResponse {
    function_response: Result<FunctionResult>,
}

#[derive(PartialEq, Eq, Debug)]
pub enum TestStatus<'a> {
    Pass,
    Fail(TestFailReason<'a>),
}

#[derive(Debug)]
pub enum TestFailReason<'a> {
    TestUnspecified(String),
    TestLLMFailure(&'a LLMResponse),
    TestParseFailure(&'a anyhow::Error),
}

impl PartialEq for TestFailReason<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::TestUnspecified(a), Self::TestUnspecified(b)) => a == b,
            (Self::TestLLMFailure(_), Self::TestLLMFailure(_)) => true,
            (Self::TestParseFailure(a), Self::TestParseFailure(b)) => {
                a.to_string() == b.to_string()
            }
            _ => false,
        }
    }
}

impl Eq for TestFailReason<'_> {}

impl TestResponse {
    pub fn status(&self) -> TestStatus {
        match &self.function_response {
            Ok(func_res) => {
                if let Some(parsed) = &func_res.parsed {
                    if parsed.is_ok() {
                        TestStatus::Pass
                    } else {
                        TestStatus::Fail(TestFailReason::TestParseFailure(
                            parsed.as_ref().unwrap_err(),
                        ))
                    }
                } else {
                    TestStatus::Fail(TestFailReason::TestLLMFailure(&func_res.llm_response))
                }
            }
            Err(e) => TestStatus::Fail(TestFailReason::TestUnspecified(e.to_string())),
        }
    }
}

impl Termination for FunctionResult {
    fn report(self) -> std::process::ExitCode {
        if self.parsed().is_some() {
            std::process::ExitCode::SUCCESS
        } else {
            std::process::ExitCode::FAILURE
        }
    }
}

impl Termination for TestResponse {
    fn report(self) -> std::process::ExitCode {
        if self.status() == TestStatus::Pass {
            std::process::ExitCode::SUCCESS
        } else {
            std::process::ExitCode::FAILURE
        }
    }
}

impl BamlRuntime {
    pub fn from_directory(dir: &PathBuf) -> Result<Self> {
        static VALID_EXTENSIONS: [&str; 2] = ["baml", "json"];

        log::info!("Reading files from {:#}", dir.to_string_lossy());

        if !dir.exists() {
            anyhow::bail!("{dir:#?} does not exist (expected a directory containing BAML files)",);
        }
        if dir.is_file() {
            return Err(anyhow::anyhow!(
                "{dir:#?} is a file, not a directory (expected a directory containing BAML files)",
            ));
        }
        if !dir.is_dir() {
            return Err(anyhow::anyhow!(
                "{dir:#?} is not a directory (expected a directory containing BAML files)",
            ));
        }

        let src_files = walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| match e {
                Ok(e) => Some(e),
                Err(e) => {
                    log::error!("Error while reading files from {dir:#?}: {e}");
                    None
                }
            })
            .filter(|e| e.file_type().is_file())
            .filter(|e| {
                let Some(ext) = e.path().extension() else {
                    return false;
                };
                let Some(ext) = ext.to_str() else {
                    return false;
                };
                VALID_EXTENSIONS.contains(&ext)
            })
            .map(|e| e.path().to_path_buf())
            .collect::<Vec<_>>();

        if !src_files
            .iter()
            .any(|f| f.extension() == Some("baml".as_ref()))
        {
            anyhow::bail!("no .baml files found in {dir:#?}");
        }

        Self::from_files(dir, src_files)
    }

    fn from_files(directory: &PathBuf, files: Vec<PathBuf>) -> Result<Self> {
        let contents = files
            .iter()
            .map(|path| match std::fs::read_to_string(path) {
                Ok(contents) => Ok(SourceFile::from((path.clone(), contents))),
                Err(e) => Err(e),
            })
            .filter_map(|res| res.ok())
            .collect();
        let mut schema = validate(directory, contents);
        schema.diagnostics.to_result()?;

        Ok(Self {
            ir: IntermediateRepr::from_parser_database(&schema.db)?,
        })
    }

    pub async fn run_test(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContext,
    ) -> Result<TestResponse> {
        let function = self.ir.find_function(function_name)?;
        let test = self.ir.find_test(&function, test_name)?;

        let params = match test.content().as_json(&ctx.env)? {
            serde_json::Value::Object(kv) => {
                let mut params = HashMap::new();
                for (k, v) in kv {
                    params.insert(k, v);
                }
                params
            }
            x => {
                return Ok(TestResponse {
                    function_response: Err(anyhow::anyhow!(
                        "Test content must be an object, found: {:?}",
                        x
                    )),
                })
            }
        };

        let func_response = self
            .call_function(function_name.to_string(), params, ctx)
            .await;

        Ok(TestResponse {
            function_response: func_response,
        })
    }

    pub async fn call_function(
        &self,
        function_name: String,
        params: HashMap<String, serde_json::Value>,
        ctx: &RuntimeContext,
    ) -> Result<FunctionResult> {
        let function = self.ir.find_function(function_name.as_str())?;
        self.ir.check_function_params(&function, &params)?;

        let renderer = PromptRenderer::from_function(&function)?;
        let client = self.ir.find_client(renderer.client_name())?;
        let mut client = LLMProvider::from_ir(&client, &ctx)?;

        // Generate the prompt.
        let prompt = client.render_prompt(&renderer, &ctx, &json!(params))?;

        // Call the LLM.
        let response = client.call(&self.ir, &ctx, &prompt).await;
        let parsed = response
            .content()
            .map(|content| jsonish::from_str(content, &self.ir, function.output(), &ctx.env));

        let result = FunctionResult {
            llm_response: response,
            parsed,
        };
        log::debug!("LLM call result:\n{result:#?}");
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn assert_passed(test: &TestResponse) {
        assert_eq!(test.status(), TestStatus::Pass);
    }

    fn assert_failed(test: &TestResponse) {
        assert_ne!(test.status(), TestStatus::Pass);
    }

    #[tokio::test]
    async fn test_run_test() -> Result<()> {
        let directory = PathBuf::from("/Users/vbv/repos/gloo-lang/integ-tests/baml_src");
        let runtime = BamlRuntime::from_directory(&directory).unwrap();

        let ctx = RuntimeContext::new().add_env("OPENAI_API_KEY".into(), "API_KEY".to_string());

        let res = runtime
            .run_test("ExtractNames", "pale_maroon", &ctx)
            .await?;

        assert_passed(&res);
        Ok(())
    }

    #[tokio::test]
    async fn test_call_function() -> Result<FunctionResult> {
        let directory = PathBuf::from("/Users/vbv/repos/gloo-lang/integ-tests/baml_src");
        let runtime = BamlRuntime::from_directory(&directory).unwrap();

        let ctx = RuntimeContext::new().add_env("OPENAI_API_KEY".into(), "API_KEY".to_string());

        let mut params = HashMap::new();
        params.insert("input".into(), json!("\"Attention Is All You Need\" is a landmark[1][2] 2017 research paper by Google.[3] Authored by eight scientists, it was responsible for expanding 2014 attention mechanisms proposed by Bahdanau et. al. into a new deep learning architecture known as the transformer. The paper is considered by some to be a founding document for modern artificial intelligence, as transformers became the main architecture of large language models.[4][5] At the time, the focus of the research was on improving Seq2seq techniques for machine translation, but even in their paper the authors saw the potential for other tasks like question answering and for what is now called multimodal Generative AI.\n\nThe paper's title is a reference to the song \"All You Need Is Love\" by the Beatles.[6]\n\nAs of 2024, the paper has been cited more than 100,000 times.[7]"));

        let res = runtime
            .call_function("ExtractNames".to_string(), params, &ctx)
            .await?;

        println!("{:#?}", res);

        Ok(res)
    }
}
