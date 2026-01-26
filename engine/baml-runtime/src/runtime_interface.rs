use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use baml_types::{BamlMap, BamlValue, Constraint};
use internal_baml_core::{
    internal_baml_diagnostics::Diagnostics,
    ir::{repr::IntermediateRepr, ExprFunctionWalker, FunctionWalker},
};
use internal_baml_jinja::RenderedPrompt;
use internal_llm_client::{AllowedRoleMetadata, ClientSpec};

use crate::{
    control_flow::ControlFlowVisualization,
    internal::{
        ir_features::IrFeatures,
        llm_client::{
            llm_provider::LLMProvider,
            orchestrator::{OrchestrationScope, OrchestratorNode},
            retry_policy::CallablePolicy,
        },
    },
    tracing::{BamlTracer, TracingCall},
    tracingv2::storage::storage::Collector,
    type_builder::TypeBuilder,
    types::{on_log_event::LogEventCallbackSync, FunctionResultStream},
    FunctionResult, RenderCurlSettings, RuntimeContext, RuntimeContextManager,
};

pub(crate) trait RuntimeConstructor: Sized {
    #[cfg(not(target_arch = "wasm32"))]
    fn from_directory(
        dir: &std::path::Path,
        feature_flags: internal_baml_core::feature_flags::FeatureFlags,
    ) -> Result<Self>;

    fn from_file_content<T: AsRef<str>>(
        root_path: &str,
        files: &HashMap<T, T>,
        feature_flags: internal_baml_core::feature_flags::FeatureFlags,
    ) -> Result<Self>;
}

// These are UNSTABLE, and should be considered as a work in progress
pub trait ExperimentalTracingInterface {
    fn start_call(
        &self,
        function_name: &str,
        params: &BamlMap<String, BamlValue>,
        ctx: &RuntimeContextManager,
        // TODO: return TracinsSpan in canary, but in sam branch its' tracingCall
        env_vars: &HashMap<String, String>,
    ) -> TracingCall;

    #[cfg(target_arch = "wasm32")]
    #[allow(async_fn_in_trait)]
    async fn finish_function_call(
        &self,
        call: TracingCall,
        result: &Result<FunctionResult>,
        ctx: &RuntimeContextManager,
        env_vars: &HashMap<String, String>,
    ) -> Result<uuid::Uuid>;

    #[cfg(not(target_arch = "wasm32"))]
    fn finish_function_call(
        &self,
        call: TracingCall,
        result: &Result<FunctionResult>,
        ctx: &RuntimeContextManager,
        env_vars: &HashMap<String, String>,
    ) -> Result<uuid::Uuid>;

    #[cfg(target_arch = "wasm32")]
    #[allow(async_fn_in_trait)]
    async fn finish_call(
        &self,
        call: TracingCall,
        result: Option<BamlValue>,
        ctx: &RuntimeContextManager,

        env_vars: &HashMap<String, String>,
    ) -> Result<uuid::Uuid>;

    #[cfg(not(target_arch = "wasm32"))]
    fn finish_call(
        &self,
        call: TracingCall,
        result: Option<BamlValue>,
        ctx: &RuntimeContextManager,
        env_vars: &HashMap<String, String>,
    ) -> Result<uuid::Uuid>;

    fn flush(&self) -> Result<()>;
    fn drain_stats(&self) -> crate::InnerTraceStats;

    #[cfg(not(target_arch = "wasm32"))]
    fn set_log_event_callback(&self, callback: Option<LogEventCallbackSync>) -> Result<()>;
}

pub trait InternalClientLookup<'a> {
    // Gets a top-level client/strategy by name
    fn get_llm_provider(
        &'a self,
        client_spec: &ClientSpec,
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
        client_name: &ClientSpec,
        ctx: &RuntimeContext,
    ) -> Result<Vec<OrchestratorNode>>;

    /// send a mermaid graph of the function graph
    fn function_graph(&self, function_name: &str, ctx: &RuntimeContext) -> Result<String>;

    /// build the structured control-flow visualization produced from HIR
    fn function_graph_v2(
        &self,
        function_name: &str,
        ctx: &RuntimeContext,
    ) -> Result<ControlFlowVisualization>;

    fn get_function<'ir>(&'ir self, function_name: &str) -> Result<FunctionWalker<'ir>>;
    fn get_expr_function<'ir>(
        &'ir self,
        function_name: &str,
        ctx: &RuntimeContext,
    ) -> Result<ExprFunctionWalker<'ir>>;

    #[allow(async_fn_in_trait)]
    async fn render_prompt(
        &self,
        function_name: &str,
        ctx: &RuntimeContext,
        params: &BamlMap<String, BamlValue>,
        node_index: Option<usize>,
    ) -> Result<(RenderedPrompt, OrchestrationScope, AllowedRoleMetadata)>;

    #[allow(async_fn_in_trait)]
    async fn render_raw_curl(
        &self,
        function_name: &str,
        ctx: &RuntimeContext,
        prompt: &[internal_baml_jinja::RenderedChatMessage],
        render_settings: RenderCurlSettings,
        node_index: Option<usize>,
    ) -> Result<String>;

    fn ir(&self) -> &IntermediateRepr;

    fn get_test_params(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContext,
        strict: bool,
    ) -> Result<BamlMap<String, BamlValue>>;

    fn get_test_constraints(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContext,
    ) -> Result<Vec<Constraint>>;

    fn get_test_type_builder(
        &self,
        function_name: &str,
        test_name: &str,
    ) -> Result<Option<TypeBuilder>>;
}
