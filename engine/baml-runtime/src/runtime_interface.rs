use anyhow::Result;
use internal_baml_core::internal_baml_diagnostics::Diagnostics;
use internal_baml_core::ir::{repr::IntermediateRepr, FunctionWalker};
use internal_baml_jinja::RenderedPrompt;
use std::{collections::HashMap, sync::Arc};

use crate::{
    internal::{
        ir_features::IrFeatures,
        llm_client::{llm_provider::LLMProvider, retry_policy::CallablePolicy, LLMResponse},
    },
    runtime::InternalBamlRuntime,
    FunctionResult, RuntimeContext, TestResponse,
};

pub(crate) trait RuntimeConstructor {
    #[cfg(feature = "no_wasm")]
    fn from_directory(dir: &std::path::PathBuf) -> Result<InternalBamlRuntime>;

    fn from_file_content<T: AsRef<str>>(
        root_path: &str,
        files: &HashMap<T, T>,
    ) -> Result<InternalBamlRuntime>;
}

#[cfg(not(feature = "no_wasm"))]
type ResponseType<T> = Result<T, wasm_bindgen::JsValue>;
#[cfg(feature = "no_wasm")]
type ResponseType<T> = Result<T>;

// This is a runtime that has full access (disk, network, etc) - feature full
pub trait RuntimeInterface {
    fn run_test(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContext,
    ) -> impl std::future::Future<Output = ResponseType<TestResponse>>;

    fn call_function(
        &self,
        function_name: String,
        params: HashMap<String, serde_json::Value>,
        ctx: &RuntimeContext,
    ) -> impl std::future::Future<Output = ResponseType<FunctionResult>>;

    #[cfg(feature = "no_wasm")]
    fn generate_client(
        &self,
        client_type: &internal_baml_codegen::LanguageClientType,
        args: &internal_baml_codegen::GeneratorArgs,
    ) -> Result<()>;
}

//
// These are UNSTABLE, and should be considered as a work in progress
//

// Define your composite trait with a generic parameter that must implement all the required traits.
// This is a runtime that has no access to the disk or network
pub trait InternalRuntimeInterface {
    fn features(&self) -> IrFeatures;

    fn diagnostics(&self) -> &Diagnostics;

    fn get_client(
        &self,
        client_name: &str,
        ctx: &RuntimeContext,
    ) -> Result<(Arc<LLMProvider>, Option<CallablePolicy>)>;

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
        &self,
        function_name: &str,
        ctx: &RuntimeContext,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<(RenderedPrompt, String)>;

    fn ir(&self) -> &IntermediateRepr;
}
