mod llm_client;
mod prompt_renderer;

use anyhow::Result;
use serde_json::json;
use std::{collections::HashMap, path::PathBuf};

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

pub struct BamlRuntime {
    ir: IntermediateRepr,
}

impl BamlRuntime {
    pub fn from_directory(directory: &PathBuf) -> Result<Self> {
        let glob_pattern = directory.join("**/*").to_string_lossy().to_string();
        let glob_pattern = if glob_pattern.starts_with(r"\\?\") {
            &glob_pattern[4..]
        } else {
            &glob_pattern
        };
        let entries = glob::glob(glob_pattern)?;

        let valid_extensions = ["baml", "json"];
        let src_files = entries
            .filter_map(|path| path.ok())
            .filter(|path| {
                path.is_file()
                    && path.extension().map_or(false, |ext| {
                        valid_extensions.contains(&ext.to_str().unwrap())
                    })
            })
            .collect::<Vec<_>>();

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
    ) -> Result<()> {
        let function = self.ir.find_function(function_name)?;
        self.ir.check_function_params(&function, params)?;

        let renderer = PromptRenderer::from_function(&function)?;
        let client = self.ir.find_client(renderer.client_name())?;
        let mut client = LLMProvider::from_ir(&client, &ctx)?;

        // Generate the prompt.
        let prompt = client.render_prompt(&renderer, &ctx, &json!(params))?;

        // Call the LLM.
        let response = client.call(&self.ir, &ctx, &prompt).await?;

        println!("{:?}", response);

        // Parse the output.
        let parsed = jsonish::from_str(
            response.content.as_str(),
            &self.ir,
            function.output(),
            &ctx.env,
        )?;

        println!("{:?}", parsed.0);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_call_function() -> Result<()> {
        let directory = PathBuf::from("/Users/vbv/repos/gloo-lang/integ-tests/baml_src");
        let runtime = BamlRuntime::from_directory(&directory).unwrap();

        let ctx = RuntimeContext::new().add_env("OPENAI_API_KEY".into(), "API_KEY".to_string());

        let mut params = HashMap::new();
        params.insert("input".into(), json!("\"Attention Is All You Need\" is a landmark[1][2] 2017 research paper by Google.[3] Authored by eight scientists, it was responsible for expanding 2014 attention mechanisms proposed by Bahdanau et. al. into a new deep learning architecture known as the transformer. The paper is considered by some to be a founding document for modern artificial intelligence, as transformers became the main architecture of large language models.[4][5] At the time, the focus of the research was on improving Seq2seq techniques for machine translation, but even in their paper the authors saw the potential for other tasks like question answering and for what is now called multimodal Generative AI.\n\nThe paper's title is a reference to the song \"All You Need Is Love\" by the Beatles.[6]\n\nAs of 2024, the paper has been cited more than 100,000 times.[7]"));

        runtime.call_function("ExtractNames", &params, ctx).await?;

        Ok(())
    }
}
