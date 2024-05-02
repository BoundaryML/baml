use anyhow::Result;
use internal_baml_core::ir::{repr::IntermediateRepr, FunctionWalker};
use internal_baml_jinja::RenderedPrompt;
use std::{collections::HashMap, path::PathBuf};

use crate::{
    internal::{
        ir_features::IrFeatures,
        llm_client::{llm_provider::LLMProvider, retry_policy::CallablePolicy, LLMResponse},
    },
    runtime::InternalBamlRuntime,
    FunctionResult, RuntimeContext, TestResponse,
};

pub(crate) trait RuntimeConstructor {
    #[cfg(feature = "disk")]
    fn from_directory(dir: &PathBuf) -> Result<InternalBamlRuntime>;

    fn from_file_content(
        root_path: &str,
        files: &HashMap<String, String>,
    ) -> Result<InternalBamlRuntime>;
}

// This is a runtime that has full access (disk, network, etc) - feature full
pub trait RuntimeInterface {
    fn run_test(
        &mut self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContext,
    ) -> impl std::future::Future<Output = Result<TestResponse>> + Send;

    fn call_function(
        &self,
        function_name: String,
        params: HashMap<String, serde_json::Value>,
        ctx: &RuntimeContext,
    ) -> impl std::future::Future<Output = Result<FunctionResult>> + Send;
}

//
// These are UNSTABLE, and should be considered as a work in progress
//

// Define your composite trait with a generic parameter that must implement all the required traits.
// This is a runtime that has no access to the disk or network
pub trait InternalRuntimeInterface {
    fn features(&self) -> IrFeatures;

    fn get_client_mut(
        &mut self,
        client_name: &str,
        ctx: &RuntimeContext,
    ) -> Result<(&mut LLMProvider, Option<CallablePolicy>)>;

    fn get_client(
        &mut self,
        client_name: &str,
        ctx: &RuntimeContext,
    ) -> Result<&(LLMProvider, Option<CallablePolicy>)>;

    fn get_function<'ir>(
        &'ir self,
        function_name: &str,
        ctx: &RuntimeContext,
    ) -> Result<FunctionWalker<'ir>>;

    fn parse_response<'ir>(
        &'ir self,
        function: &FunctionWalker<'ir>,
        response: LLMResponse,
        ctx: &RuntimeContext,
    ) -> Result<FunctionResult>;

    fn render_prompt(
        &mut self,
        function_name: &str,
        ctx: &RuntimeContext,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<RenderedPrompt>;

    fn ir(&self) -> &IntermediateRepr;
}
