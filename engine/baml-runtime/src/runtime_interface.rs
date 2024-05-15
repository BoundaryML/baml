use anyhow::Result;
use baml_types::{BamlMap, BamlValue};
use indexmap::IndexMap;
use internal_baml_core::internal_baml_diagnostics::Diagnostics;
use internal_baml_core::ir::{repr::IntermediateRepr, FunctionWalker};
use internal_baml_jinja::RenderedPrompt;
use std::{collections::HashMap, sync::Arc};

use crate::tracing::TracingSpan;
use crate::{
    internal::{
        ir_features::IrFeatures,
        llm_client::{llm_provider::LLMProvider, retry_policy::CallablePolicy, LLMResponse},
    },
    runtime::InternalBamlRuntime,
    types::FunctionResultStream,
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

// This is a runtime that has full access (disk, network, etc) - feature full
pub trait RuntimeInterface {
    fn run_test(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContext,
    ) -> impl std::future::Future<Output = Result<TestResponse>>;

    fn call_function(
        &self,
        function_name: String,
        params: IndexMap<String, BamlValue>,
        ctx: &RuntimeContext,
    ) -> impl std::future::Future<Output = Result<FunctionResult>>;

    fn stream_function(
        &self,
        function_name: String,
        params: IndexMap<String, BamlValue>,
        ctx: &RuntimeContext,
    ) -> impl std::future::Future<Output = Result<FunctionResultStream>>;

    #[cfg(feature = "no_wasm")]
    fn generate_client(
        &self,
        client_type: &internal_baml_codegen::LanguageClientType,
        args: &internal_baml_codegen::GeneratorArgs,
    ) -> Result<internal_baml_codegen::GenerateOutput>;
}

//
// These are UNSTABLE, and should be considered as a work in progress
//

pub trait ExperimentalTracingInterface {
    fn start_span(
        &self,
        function_name: &str,
        ctx: &RuntimeContext,
        params: &BamlMap<String, BamlValue>,
    ) -> Option<TracingSpan>;

    #[allow(async_fn_in_trait)]
    async fn finish_function_span(
        &self,
        span: TracingSpan,
        result: &Result<FunctionResult>,
    ) -> Result<()>;

    fn flush(&self) -> Result<()>;
}

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
        params: &IndexMap<String, BamlValue>,
    ) -> Result<(RenderedPrompt, String)>;

    fn ir(&self) -> &IntermediateRepr;
}
