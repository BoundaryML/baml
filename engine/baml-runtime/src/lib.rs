pub(crate) mod internal;

mod runtime;
mod runtime_interface;
mod types;

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use internal::{
    ir_features::IrFeatures,
    llm_client::{
        llm_provider::LLMProvider,
        retry_policy::CallablePolicy,
        traits::{WithCallable, WithPrompt},
        LLMResponse, ModelFeatures,
    },
    prompt_renderer::PromptRenderer,
};
use internal_baml_core::ir::IRHelper;
use internal_baml_core::ir::{repr::IntermediateRepr, FunctionWalker};
use runtime::InternalBamlRuntime;

pub use runtime_interface::*;
pub use types::*;

pub struct BamlRuntime {
    inner: InternalBamlRuntime,
}

impl BamlRuntime {
    /// Load a runtime from a directory
    pub fn from_directory(path: &PathBuf) -> Result<Self> {
        Ok(BamlRuntime {
            inner: InternalBamlRuntime::from_directory(path)?,
        })
    }
}

impl RuntimeInterface for BamlRuntime {
    async fn run_test(
        &mut self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContext,
    ) -> Result<TestResponse> {
        let func = self.inner.get_function(function_name, ctx)?;
        let test = self.inner.ir().find_test(&func, test_name)?;

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
        self.inner.ir().check_function_params(&func, &params)?;

        let renderer = PromptRenderer::from_function(&func)?;
        let client_name = renderer.client_name().to_string();

        let (client, retry_policy) = self.inner.get_client_mut(&client_name, ctx)?;
        let prompt = client.render_prompt(&renderer, &ctx, &serde_json::json!(params))?;

        let response = client.call(retry_policy, ctx, &prompt).await;

        // We need to get the function again because self is borrowed mutably.
        let func = self.inner.get_function(function_name, ctx)?;
        let parsed = self.inner.parse_response(&func, response, ctx)?;
        Ok(TestResponse {
            function_response: Ok(parsed),
        })
    }

    async fn call_function(
        &mut self,
        function_name: String,
        params: HashMap<String, serde_json::Value>,
        ctx: &RuntimeContext,
    ) -> Result<crate::FunctionResult> {
        let func = self.inner.get_function(&function_name, ctx)?;
        self.inner.ir().check_function_params(&func, &params)?;

        let renderer = PromptRenderer::from_function(&func)?;
        let client_name = renderer.client_name().to_string();

        let (client, retry_policy) = self.inner.get_client_mut(&client_name, ctx)?;
        let prompt = client.render_prompt(&renderer, &ctx, &serde_json::json!(params))?;

        let response = client.call(retry_policy, ctx, &prompt).await;

        // We need to get the function again because self is borrowed mutably.
        let func = self.inner.get_function(&function_name, ctx)?;
        let parsed = self.inner.parse_response(&func, response, ctx)?;
        Ok(parsed)
    }
}

impl InternalRuntimeInterface for BamlRuntime {
    fn features(&self) -> IrFeatures {
        self.inner.features()
    }

    fn get_client_mut(
        &mut self,
        client_name: &str,
        ctx: &RuntimeContext,
    ) -> Result<(&mut LLMProvider, Option<CallablePolicy>)> {
        self.inner.get_client_mut(client_name, ctx)
    }

    fn get_function<'ir>(
        &'ir self,
        function_name: &str,
        ctx: &RuntimeContext,
    ) -> Result<FunctionWalker<'ir>> {
        self.inner.get_function(function_name, ctx)
    }

    fn parse_response<'ir>(
        &'ir self,
        function: &FunctionWalker<'ir>,
        response: LLMResponse,
        ctx: &RuntimeContext,
    ) -> Result<FunctionResult> {
        self.inner.parse_response(function, response, ctx)
    }

    fn ir(&self) -> &IntermediateRepr {
        self.inner.ir()
    }
}
