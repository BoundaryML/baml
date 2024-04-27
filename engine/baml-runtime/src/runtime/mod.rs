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

pub struct FunctionResponse {
    llm_response: LLMResponse,
    parsed: Option<Result<(serde_json::Value, jsonish::DeserializerConditions)>>,
}

impl Termination for FunctionResponse {
    fn report(self) -> std::process::ExitCode {
        match self.parsed {
            Some(Ok((_, _))) => std::process::ExitCode::SUCCESS,
            _ => std::process::ExitCode::FAILURE,
        }
    }
}

impl BamlRuntime {
    pub fn from_directory(directory: &PathBuf) -> Result<Self> {
        static VALID_EXTENSIONS: [&str; 2] = ["baml", "json"];

        let src_files = walkdir::WalkDir::new(directory)
            .into_iter()
            .filter_map(|e| match e {
                Ok(e) => Some(e),
                Err(e) => {
                    log::warn!(
                        "Error while reading files from {:#}: {}",
                        directory.to_string_lossy(),
                        e
                    );
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
            .collect();

        Self::from_files(directory, src_files)
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

    pub async fn call_function(
        &self,
        function_name: &str,
        params: &HashMap<String, serde_json::Value>,
        ctx: RuntimeContext,
    ) -> Result<FunctionResponse> {
        let function = self.ir.find_function(function_name)?;
        self.ir.check_function_params(&function, params)?;

        let renderer = PromptRenderer::from_function(&function)?;
        let client = self.ir.find_client(renderer.client_name())?;
        let mut client = LLMProvider::from_ir(&client, &ctx)?;

        // Generate the prompt.
        let prompt = client.render_prompt(&renderer, &ctx, &json!(params))?;

        // Call the LLM.
        let response = client.call(&self.ir, &ctx, &prompt).await;

        match response.content() {
            None => {
                return Ok(FunctionResponse {
                    llm_response: response,
                    parsed: None,
                });
            }
            Some(content) => {
                let parsed = jsonish::from_str(content, &self.ir, function.output(), &ctx.env);

                Ok(FunctionResponse {
                    llm_response: response,
                    parsed: Some(parsed),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_call_function() -> Result<FunctionResponse> {
        let directory = PathBuf::from("/Users/vbv/repos/gloo-lang/integ-tests/baml_src");
        let runtime = BamlRuntime::from_directory(&directory).unwrap();

        let ctx = RuntimeContext::new().add_env("OPENAI_API_KEY".into(), "API_KEY".to_string());

        let mut params = HashMap::new();
        params.insert("input".into(), json!("\"Attention Is All You Need\" is a landmark[1][2] 2017 research paper by Google.[3] Authored by eight scientists, it was responsible for expanding 2014 attention mechanisms proposed by Bahdanau et. al. into a new deep learning architecture known as the transformer. The paper is considered by some to be a founding document for modern artificial intelligence, as transformers became the main architecture of large language models.[4][5] At the time, the focus of the research was on improving Seq2seq techniques for machine translation, but even in their paper the authors saw the potential for other tasks like question answering and for what is now called multimodal Generative AI.\n\nThe paper's title is a reference to the song \"All You Need Is Love\" by the Beatles.[6]\n\nAs of 2024, the paper has been cited more than 100,000 times.[7]"));

        let res = runtime.call_function("ExtractNames", &params, ctx).await?;

        println!("{:#?}", res.llm_response);

        if let Some(Ok((val, flags))) = &res.parsed {
            println!("{}", val);
            println!("{}", flags);
        }

        Ok(res)
    }
}
