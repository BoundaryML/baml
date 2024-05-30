use anyhow::Result;
use baml_types::{BamlMap, BamlValue};
use cfg_if::cfg_if;
use internal_baml_core::internal_baml_diagnostics::Diagnostics;
use internal_baml_core::ir::{repr::IntermediateRepr, FunctionWalker};
use internal_baml_jinja::RenderedPrompt;
use std::{collections::HashMap, sync::Arc};

use crate::internal::llm_client::llm_provider::LLMProvider;
use crate::internal::llm_client::orchestrator::{OrchestrationScope, OrchestratorNode};
use crate::tracing::{BamlTracer, TracingSpan};
use crate::type_builder::TypeBuilder;
use crate::RuntimeContextManager;
use crate::{
    internal::{ir_features::IrFeatures, llm_client::retry_policy::CallablePolicy},
    runtime::InternalBamlRuntime,
    types::FunctionResultStream,
    FunctionResult, RuntimeContext, TestResponse,
};

pub(crate) trait RuntimeConstructor {
    #[cfg(not(target_arch = "wasm32"))]
    fn from_directory(dir: &std::path::PathBuf) -> Result<InternalBamlRuntime>;

    fn from_file_content<T: AsRef<str>>(
        root_path: &str,
        files: &HashMap<T, T>,
    ) -> Result<InternalBamlRuntime>;
}

// This is a runtime that has full access (disk, network, etc) - feature full
pub trait RuntimeInterface {
    #[allow(async_fn_in_trait)]
    async fn call_function_impl(
        &self,
        function_name: String,
        params: &BamlMap<String, BamlValue>,
        ctx: RuntimeContext,
    ) -> Result<FunctionResult>;

    fn stream_function_impl(
        &self,
        function_name: String,
        params: &BamlMap<String, BamlValue>,
        tracer: Arc<BamlTracer>,
        ctx: RuntimeContext,
    ) -> Result<FunctionResultStream>;
}

//
// These are UNSTABLE, and should be considered as a work in progress
//

pub trait ExperimentalTracingInterface {
    fn start_span(
        &self,
        function_name: &str,
        params: &BamlMap<String, BamlValue>,
        ctx: &RuntimeContextManager,
    ) -> (Option<TracingSpan>, RuntimeContext);

    #[allow(async_fn_in_trait)]
    async fn finish_function_span(
        &self,
        span: TracingSpan,
        result: &Result<FunctionResult>,
        ctx: &RuntimeContextManager,
    ) -> Result<Option<uuid::Uuid>>;

    #[allow(async_fn_in_trait)]
    async fn finish_span(
        &self,
        span: TracingSpan,
        result: Option<BamlValue>,
        ctx: &RuntimeContextManager,
    ) -> Result<Option<uuid::Uuid>>;

    fn flush(&self) -> Result<()>;
}

pub trait InternalClientLookup<'a> {
    // Gets a top-level client/strategy by name
    fn get_llm_provider(
        &'a self,
        client_name: &str,
        ctx: &RuntimeContext,
    ) -> Result<Arc<LLMProvider>>;

    fn get_retry_policy(&self, policy_name: &str, ctx: &RuntimeContext) -> Result<CallablePolicy>;
}

// Define your composite trait with a generic parameter that must implement all the required traits.
// This is a runtime that has no access to the disk or network
pub trait InternalRuntimeInterface {
    fn features(&self) -> IrFeatures;

    fn diagnostics(&self) -> &Diagnostics;

    fn orchestration_graph(
        &self,
        client_name: &str,
        ctx: &RuntimeContext,
    ) -> Result<Vec<OrchestratorNode>>;

    fn get_function<'ir>(
        &'ir self,
        function_name: &str,
        ctx: &RuntimeContext,
    ) -> Result<FunctionWalker<'ir>>;

    fn render_prompt(
        &self,
        function_name: &str,
        ctx: &RuntimeContext,
        params: &BamlMap<String, BamlValue>,
        node_index: Option<usize>,
    ) -> Result<(RenderedPrompt, OrchestrationScope)>;

    fn ir(&self) -> &IntermediateRepr;

    fn get_test_params(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContext,
    ) -> Result<BamlMap<String, BamlValue>>;
}
