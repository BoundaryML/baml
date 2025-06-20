// mod tests;

#[cfg(feature = "internal")]
pub mod internal;
#[cfg(not(feature = "internal"))]
pub(crate) mod internal;

#[cfg(not(target_arch = "wasm32"))]
pub mod cli;
pub mod client_registry;
pub mod errors;
pub mod eval_expr;
pub mod request;
pub mod runtime;
pub mod runtime_interface;
pub mod test_constraints;
#[cfg(not(target_arch = "wasm32"))]
pub mod test_executor;

mod runtime_methods;
pub mod tracing;
pub mod tracingv2;
pub mod type_builder;
mod types;

use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
};

use anyhow::{Context, Result};
use baml_ids::{FunctionCallId, HttpRequestId};
use baml_types::{
    expr::{Expr, ExprMetadata},
    tracing::events::{HTTPBody, HTTPRequest, TraceEvent},
    BamlMap, BamlValue, BamlValueWithMeta, Completion, Constraint,
};
use cfg_if::cfg_if;
#[cfg(not(target_arch = "wasm32"))]
pub use cli::RuntimeCliDefaults;
use client_registry::ClientRegistry;
use dashmap::DashMap;
use eval_expr::{EvalEnv, ExprEvalResult};
use futures::{
    channel::mpsc,
    future::{join, join_all},
};
use indexmap::IndexMap;
use internal::{
    llm_client::{
        llm_provider::LLMProvider,
        orchestrator::OrchestrationScope,
        primitive::{json_body, json_headers, JsonBodyInput},
        retry_policy::CallablePolicy,
    },
    prompt_renderer::PromptRenderer,
};
use internal_baml_core::{
    ast::Span,
    configuration::{CloudProject, CodegenGenerator, Generator, GeneratorOutputType},
    internal_baml_diagnostics::SerializedSpan,
    ir::{
        repr::{initial_context, IntermediateRepr},
        FunctionWalker, IRHelperExtended,
    },
};
pub use internal_baml_core::{
    internal_baml_diagnostics,
    internal_baml_diagnostics::Diagnostics as DiagnosticsError,
    ir::{ir_helpers::infer_type, scope_diagnostics, FieldType, IRHelper, TypeValue},
};
#[cfg(feature = "internal")]
pub use internal_baml_jinja::{ChatMessagePart, RenderedPrompt};
#[cfg(not(feature = "internal"))]
pub(crate) use internal_baml_jinja::{ChatMessagePart, RenderedPrompt};
use internal_llm_client::{AllowedRoleMetadata, ClientSpec};
use jsonish::{ResponseBamlValue, ResponseValueMeta};
use on_log_event::LogEventCallbackSync;
use runtime::InternalBamlRuntime;
pub use runtime_context::BamlSrcReader;
#[cfg(feature = "internal")]
pub use runtime_interface::InternalRuntimeInterface;
#[cfg(not(feature = "internal"))]
pub(crate) use runtime_interface::InternalRuntimeInterface;
use runtime_interface::{ExperimentalTracingInterface, InternalClientLookup, RuntimeConstructor};
pub(crate) use runtime_methods::prepare_function::PreparedFunctionArgs;
use serde_json::{self, json};
use tracing::{BamlTracer, TracingCall};
use tracingv2::{
    publisher::flush,
    storage::storage::{Collector, BAML_TRACER},
};
use type_builder::TypeBuilder;
pub use types::*;
use web_time::{Duration, SystemTime};

use crate::{
    errors::IntoBamlError,
    internal::llm_client::{LLMCompleteResponse, LLMCompleteResponseMetadata, LLMResponse},
    test_constraints::{evaluate_test_constraints, TestConstraintsResult},
};

#[cfg(not(target_arch = "wasm32"))]
static TOKIO_SINGLETON: OnceLock<std::io::Result<Arc<tokio::runtime::Runtime>>> = OnceLock::new();

static INIT: std::sync::Once = std::sync::Once::new();

// fn setup_crypto_provider() {
//     #[cfg(not(target_arch = "wasm32"))]
//     {
//         use rustls::crypto::CryptoProvider;
//         INIT.call_once(|| {
//             let provider = rustls::crypto::ring::default_provider();
//             CryptoProvider::install_default(provider).expect("failed to install CryptoProvider");
//         });
//     }
// }
pub struct BamlTracerWrapper {
    tracers: DashMap<String, Arc<BamlTracer>>,
}

impl BamlTracerWrapper {
    /// Helper to filter only the relevant env_vars (BOUNDARY_*) for use as a key and config.
    fn filter_relevant_env_vars(env_vars: &HashMap<String, String>) -> HashMap<String, String> {
        env_vars
            .iter()
            .filter(|(k, _)| k.starts_with("BOUNDARY_"))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Helper to deterministically hash only the relevant env_vars (BOUNDARY_*) for use as a key.
    fn env_vars_key(env_vars: &HashMap<String, String>) -> String {
        let filtered = Self::filter_relevant_env_vars(env_vars);
        let mut items: Vec<_> = filtered.iter().collect();
        items.sort_by(|a, b| a.0.cmp(b.0));
        serde_json::to_string(&items).unwrap()
    }

    /// Create a new BamlTracerWrapper and insert a tracer for the given env vars.
    pub fn new(env_vars: &HashMap<String, String>) -> Result<Self> {
        let tracers = DashMap::new();
        let filtered = Self::filter_relevant_env_vars(env_vars);
        let key = Self::env_vars_key(env_vars);
        let tracer = Arc::new(BamlTracer::new(None, filtered.clone().into_iter())?);
        tracers.insert(key, tracer);
        Ok(Self { tracers })
    }

    /// Get the tracer for the given env vars, creating a new one if the config changed.
    pub fn get_or_create_tracer(&self, env_vars: &HashMap<String, String>) -> Arc<BamlTracer> {
        let filtered = Self::filter_relevant_env_vars(env_vars);
        let key = Self::env_vars_key(env_vars);
        if let Some(existing) = self.tracers.get(&key) {
            if existing.config_matches_env_vars(&filtered) {
                let cloned = existing.clone();
                return cloned;
            }
        }
        // Config changed, clear all and insert new
        self.tracers.clear();
        let new_tracer = Arc::new(
            BamlTracer::new(None, filtered.clone().into_iter())
                .expect("Failed to create BamlTracer"),
        );
        self.tracers.insert(key, new_tracer.clone());
        new_tracer
    }

    /// Get the current tracer (the only one in the map, if any).
    pub fn get_tracer(&self) -> Arc<BamlTracer> {
        // Return the first tracer if any, else panic (should always have one)
        self.tracers.iter().next().expect("No tracer found").clone()
    }
}

#[derive(Clone)]
pub struct BamlRuntime {
    pub inner: Arc<InternalBamlRuntime>,
    pub tracer_wrapper: Arc<BamlTracerWrapper>,
    #[cfg(not(target_arch = "wasm32"))]
    pub async_runtime: Arc<tokio::runtime::Runtime>,
}

impl BamlRuntime {
    #[cfg(not(target_arch = "wasm32"))]
    fn get_tokio_singleton() -> Result<Arc<tokio::runtime::Runtime>> {
        match TOKIO_SINGLETON.get_or_init(|| tokio::runtime::Runtime::new().map(Arc::new)) {
            Ok(t) => Ok(t.clone()),
            Err(e) => Err(e.into()),
        }
    }

    fn new_runtime(inner: InternalBamlRuntime, env_vars: &HashMap<String, String>) -> Result<Self> {
        #[cfg(not(target_arch = "wasm32"))]
        let rt = Self::get_tokio_singleton()?;
        let inner = Arc::new(inner);

        let runtime = BamlRuntime {
            inner: inner.clone(),
            tracer_wrapper: Arc::new(BamlTracerWrapper::new(&env_vars)?),
            #[cfg(not(target_arch = "wasm32"))]
            async_runtime: rt.clone(),
        };

        tracingv2::publisher::start_publisher(
            Arc::new(
                (inner, env_vars.clone()).try_into().context(
                    "Internal error: Failed to create a event publisher for BAML runtime",
                )?,
            ),
            #[cfg(not(target_arch = "wasm32"))]
            rt.clone(),
        );

        Ok(runtime)
    }

    pub fn parse_baml_src_path(path: impl Into<PathBuf>) -> Result<PathBuf> {
        let mut path: PathBuf = path.into();

        if !path.exists() {
            anyhow::bail!(
                "Expected --from '{}' to be a baml_src/ directory, but it does not exist",
                path.display()
            );
        }

        if !path.is_dir() {
            anyhow::bail!(
                "Expected --from '{}' to be a baml_src/ directory, but it is not",
                path.display()
            );
        }

        if path.file_name() != Some(std::ffi::OsStr::new("baml_src")) {
            let contained = path.join("baml_src");

            if contained.exists() && contained.is_dir() {
                path = contained;
            } else {
                anyhow::bail!(
                    "Expected --from '{}' to be a baml_src/ directory, but it is not",
                    path.display()
                );
            }
        }

        Ok(path)
    }

    /// Load a runtime from a directory
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_directory<T: AsRef<str>>(
        path: &std::path::Path,
        env_vars: HashMap<T, T>,
    ) -> Result<Self> {
        // setup_crypto_provider();
        let path = Self::parse_baml_src_path(path)?;

        let copy = env_vars
            .iter()
            .map(|(k, v)| (k.as_ref().to_string(), v.as_ref().to_string()))
            .collect();
        baml_log::set_from_env(&copy)?;

        Self::new_runtime(InternalBamlRuntime::from_directory(&path)?, &copy)
    }

    pub fn from_file_content<T: AsRef<str> + std::fmt::Debug, U: AsRef<str>>(
        root_path: &str,
        files: &HashMap<T, T>,
        env_vars: HashMap<U, U>,
    ) -> Result<Self> {
        // setup_crypto_provider();
        let copy = env_vars
            .iter()
            .map(|(k, v)| (k.as_ref().to_string(), v.as_ref().to_string()))
            .collect();
        baml_log::set_from_env(&copy)?;

        Self::new_runtime(
            InternalBamlRuntime::from_file_content(root_path, files)?,
            &copy,
        )
    }

    #[cfg(feature = "internal")]
    pub fn internal(&self) -> &Arc<InternalBamlRuntime> {
        &self.inner
    }

    pub fn create_ctx_manager(
        &self,
        language: BamlValue,
        // A callback that can be implemented in JS to read files that are referred in tests.
        baml_src_reader: BamlSrcReader,
    ) -> RuntimeContextManager {
        let ctx = RuntimeContextManager::new(baml_src_reader);
        let tags: HashMap<String, BamlValue> = [("baml.language", language)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        ctx.upsert_tags(tags);
        ctx
    }

    // Another way of creating a context that uses some
    // helper functions to load AWS SSO profile and creds.
    // These functions are implemented in Node for example, and used by the vscode playground to make aws sso work.
    pub fn create_ctx_manager_for_wasm(
        &self,
        // This callback reads files that are added in tests
        baml_src_reader: BamlSrcReader,
    ) -> RuntimeContextManager {
        let ctx = RuntimeContextManager::new(baml_src_reader);
        let tags: HashMap<String, BamlValue> = [(
            "baml.language".to_string(),
            BamlValue::String("wasm".to_string()),
        )]
        .into_iter()
        .collect();
        ctx.upsert_tags(tags);
        ctx
    }
}

impl BamlRuntime {
    pub async fn render_prompt(
        &self,
        function_name: &str,
        ctx: &RuntimeContext,
        params: &BamlMap<String, BamlValue>,
        node_index: Option<usize>,
    ) -> Result<(RenderedPrompt, OrchestrationScope, AllowedRoleMetadata)> {
        self.inner
            .render_prompt(function_name, ctx, params, node_index)
            .await
    }

    pub fn llm_provider_from_function(
        &self,
        function_name: &str,
        ctx: &RuntimeContext,
    ) -> Result<Arc<LLMProvider>> {
        let renderer = PromptRenderer::from_function(
            &self.inner.get_function(&function_name)?,
            self.inner.ir(),
            &ctx,
        )?;

        self.inner.get_llm_provider(renderer.client_spec(), ctx)
    }

    pub fn get_test_params_and_constraints(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContext,
        strict: bool,
    ) -> Result<(BamlMap<String, BamlValue>, Vec<Constraint>)> {
        let params = self
            .inner
            .get_test_params(function_name, test_name, ctx, strict)?;
        let constraints = self
            .inner
            .get_test_constraints(function_name, test_name, ctx)
            .unwrap_or(vec![]); // TODO: Fix this.
                                // .get_test_constraints(function_name, test_name, ctx)?;
        Ok((params, constraints))
    }

    pub fn get_test_params(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContext,
        strict: bool,
    ) -> Result<BamlMap<String, BamlValue>> {
        self.inner
            .get_test_params(function_name, test_name, ctx, strict)
    }

    pub async fn run_test_with_expr_events<F>(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContextManager,
        on_event: Option<F>,
        expr_tx: Option<mpsc::UnboundedSender<Vec<internal_baml_diagnostics::SerializedSpan>>>,
        collector: Option<Arc<Collector>>,
        env_vars: HashMap<String, String>,
    ) -> (Result<TestResponse>, FunctionCallId)
    where
        F: Fn(FunctionResult),
    {
        baml_log::set_from_env(&env_vars).unwrap();

        let call = self
            .tracer_wrapper
            .get_or_create_tracer(&env_vars)
            .start_call(
                test_name,
                ctx,
                &Default::default(),
                true,
                true, // tests always stream which is why there's an on_event
                collector.as_ref().map(|c| vec![c.clone()]),
            );

        let expr_fn = self.inner.ir().find_expr_fn(function_name);
        let is_expr_fn = expr_fn.is_ok();

        let run_to_response = || async {
            // acceptable clone, just used for testing
            let rctx_no_tb =
                ctx.create_ctx(None, None, env_vars.clone(), call.new_call_id_stack.clone())?;
            let (params, constraints) =
                self.get_test_params_and_constraints(function_name, test_name, &rctx_no_tb, true)?;

            // Run the expression to either a value or a final LLM call.
            // (If it's not an expr fn, it'll be a final LLM call.)
            let expr_eval_result = expr_eval_result(
                self,
                ctx,
                expr_tx.clone(),
                collector.clone(),
                self.tracer_wrapper.get_or_create_tracer(&env_vars),
                None,
                None,
                env_vars.clone(),
                function_name,
                &params,
            )
            .await?;

            // If the expression evaluates to an LLM call, shadow the old function_name and params (of
            // the test function) with the new function_name and params (of the LLM call).
            let (function_name, params): (String, BamlMap<String, BamlValue>) =
                match &expr_eval_result {
                    ExprEvalResult::Value { value, field_type } => {
                        (function_name.to_string(), params.clone())
                    }
                    ExprEvalResult::LLMCall { name, args } => (name.to_string(), args.clone()),
                };

            let type_builder = if is_expr_fn {
                None
            } else {
                self.inner
                    .get_test_type_builder(&function_name, test_name)
                    .unwrap()
            };

            let rctx = ctx.create_ctx(
                type_builder.as_ref(),
                None,
                env_vars.clone(),
                call.new_call_id_stack.clone(),
            )?;

            let (function_name, params) = match expr_eval_result {
                ExprEvalResult::Value { value, field_type } => {
                    let fake_syntax_span = Span::fake();
                    return Ok(TestResponse {
                        // TODO: Factor out fake response data.
                        function_response: FunctionResult::new(
                            OrchestrationScope { scope: vec![] },
                            LLMResponse::Success(LLMCompleteResponse {
                                client: "openai".to_string(),
                                model: "gpt-3.5-turbo".to_string(),
                                prompt: RenderedPrompt::Completion(
                                    "Sample raw response".to_string(),
                                ),
                                request_options: BamlMap::new(),
                                content: "Sample raw response".to_string(),
                                start_time: SystemTime::now(),
                                latency: Duration::from_millis(2025),
                                metadata: LLMCompleteResponseMetadata {
                                    baml_is_complete: true,
                                    finish_reason: Some("stop".to_string()),
                                    prompt_tokens: Some(50),
                                    output_tokens: Some(50),
                                    total_tokens: Some(100),
                                },
                            }),
                            // TODO: Run checks and asserts.
                            Some(Ok(ResponseBamlValue(value.map_meta(|_| {
                                ResponseValueMeta(
                                    vec![],
                                    vec![],
                                    Completion::default(),
                                    field_type.clone(),
                                )
                            })))),
                        ),
                        function_call: call.curr_call_id(),
                        constraints_result: TestConstraintsResult::empty(),
                    });
                }
                ExprEvalResult::LLMCall { name, args } => (name, args),
            };

            let mut stream = self.inner.stream_function_impl(
                function_name.into(),
                &params,
                self.tracer_wrapper.get_or_create_tracer(&env_vars),
                rctx,
                #[cfg(not(target_arch = "wasm32"))]
                self.async_runtime.clone(),
                // TODO: collectors here?
                vec![],
            )?;
            let (response_res, call_uuid) = stream
                .run(on_event, ctx, type_builder.as_ref(), None, env_vars.clone())
                .await;
            let res = response_res?;
            let (_, llm_resp, val) = res
                .event_chain()
                .iter()
                .last()
                .context("Expected non-empty event chain")?;
            if let Some(expr_tx) = expr_tx {
                expr_tx.unbounded_send(vec![]).unwrap();
            }
            let complete_resp = match llm_resp {
                LLMResponse::Success(complete_llm_response) => Ok(complete_llm_response),
                LLMResponse::InternalFailure(e) => Err(anyhow::anyhow!("{}", e)),
                LLMResponse::UserFailure(e) => Err(anyhow::anyhow!("{}", e)),
                LLMResponse::LLMFailure(e) => Err(anyhow::anyhow!(
                    "{} {}\n\nRequest options: {}",
                    e.code.to_string(),
                    e.message,
                    serde_json::to_string(&e.request_options).unwrap_or_default()
                )),
            }?;
            let test_constraints_result = if constraints.is_empty() {
                TestConstraintsResult::empty()
            } else {
                match val {
                    Some(Ok(value)) => {
                        let value_with_constraints = value.0.map_meta(|m| m.1.clone());
                        evaluate_test_constraints(
                            &params,
                            &value_with_constraints,
                            &complete_resp,
                            constraints,
                        )
                    }
                    _ => TestConstraintsResult::empty(),
                }
            };

            Ok(TestResponse {
                function_response: res,
                function_call: call_uuid,
                constraints_result: test_constraints_result,
            })
        };

        let response = run_to_response().await;

        let call_id = call.curr_call_id();
        {
            #[cfg(not(target_arch = "wasm32"))]
            match self
                .tracer_wrapper
                .get_or_create_tracer(&env_vars)
                .finish_call(call, ctx, None)
            {
                Ok(id) => {}
                Err(e) => baml_log::error!("Error during logging: {}", e),
            }
            #[cfg(target_arch = "wasm32")]
            match self
                .tracer_wrapper
                .get_or_create_tracer(&env_vars)
                .finish_call(call, ctx, None)
                .await
            {
                Ok(id) => {}
                Err(e) => log::error!("Error during logging: {}", e),
            }
        }

        (response, call_id)
    }

    pub async fn run_test<F>(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContextManager,
        on_event: Option<F>,
        collector: Option<Arc<Collector>>,
        env_vars: HashMap<String, String>,
    ) -> (Result<TestResponse>, FunctionCallId)
    where
        F: Fn(FunctionResult),
    {
        let res = self
            .run_test_with_expr_events::<F>(
                function_name,
                test_name,
                ctx,
                on_event,
                None,
                collector,
                env_vars,
            )
            .await;
        res
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn call_function_sync(
        &self,
        function_name: String,
        params: &BamlMap<String, BamlValue>,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        collectors: Option<Vec<Arc<Collector>>>,
        env_vars: HashMap<String, String>,
    ) -> (Result<FunctionResult>, FunctionCallId) {
        let fut = self.call_function(function_name, params, ctx, tb, cb, collectors, env_vars);
        self.async_runtime.block_on(fut)
    }

    pub async fn call_function(
        &self,
        function_name: String,
        params: &BamlMap<String, BamlValue>,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        collectors: Option<Vec<Arc<Collector>>>,
        env_vars: HashMap<String, String>,
    ) -> (Result<FunctionResult>, FunctionCallId) {
        let res = Box::pin(self.call_function_with_expr_events(
            function_name,
            params,
            ctx,
            tb,
            cb,
            collectors,
            env_vars,
            None,
        ))
        .await;
        res
    }

    pub async fn call_function_with_expr_events(
        &self,
        function_name: String,
        params: &BamlMap<String, BamlValue>,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        collectors: Option<Vec<Arc<Collector>>>,
        env_vars: HashMap<String, String>,
        expr_tx: Option<mpsc::UnboundedSender<Vec<internal_baml_diagnostics::SerializedSpan>>>,
    ) -> (Result<FunctionResult>, FunctionCallId) {
        // baml_log::info!("env vars: {:#?}", env_vars.clone());
        baml_log::set_from_env(&env_vars).unwrap();

        log::trace!("Calling function: {}", function_name);
        log::debug!("collectors: {:#?}", &collectors);

        let call = self
            .tracer_wrapper
            .get_or_create_tracer(&env_vars)
            .start_call(&function_name, ctx, params, true, false, collectors);
        let curr_call_id = call.curr_call_id();

        let fake_syntax_span = Span::fake();
        let response =
            match ctx.create_ctx(tb, cb, env_vars.clone(), call.new_call_id_stack.clone()) {
                Ok(rctx) => {
                    let is_expr_fn = self
                        .inner
                        .ir()
                        .expr_fns
                        .iter()
                        .find(|f| f.elem.name == function_name)
                        .is_some();
                    if !is_expr_fn {
                        let call_id_stack = rctx.call_id_stack.clone();
                        // TODO: is this the right naming?
                        let prepared_func =
                            match self.inner.prepare_function(function_name.clone(), params) {
                                Ok(prepared_func) => prepared_func,
                                Err(e) => {
                                    return (e.into(), curr_call_id);
                                }
                            };

                        // Call (CANNOT RETURN HERE until trace event is finished)
                        let result = self.inner.call_function_impl(prepared_func, rctx).await;
                        // eprintln!("result: {:?}", result);
                        // Trace event
                        let trace_event = TraceEvent::new_function_end(
                            call_id_stack.clone(),
                            match &result {
                                Ok(result) => match result.result_with_constraints_content() {
                                    Ok(value) => Ok(value.0.map_meta(|f| f.3.clone())),
                                    Err(e) => Err((&e).into_baml_error()), // None => Err(baml_types::tracing::errors::BamlError::Base {
                                                                           //     message: format!(
                                                                           //         "No parsed result found for function: {}",
                                                                           //         function_name
                                                                           //     )
                                                                           //     .into(),
                                                                           // }),
                                },
                                Err(e) => Err(e.into_baml_error()),
                            },
                        );
                        BAML_TRACER.lock().unwrap().put(Arc::new(trace_event));

                        result
                    } else {
                        // TODO: This code path is ugly. Calling a function heavily assumes that the
                        // function is an LLM function. Find a way to make function-calling API more
                        // hospitable to Expression Fns, or create new APIs for calling Expr Fns.
                        let expr_fn = &self
                            .inner
                            .ir()
                            .expr_fns
                            .iter()
                            .find(|f| f.elem.name == function_name)
                            .expect("We checked earlier that this function is an expr_fn")
                            .elem;
                        let fn_expr = expr_fn.expr.clone();
                        let context = initial_context(&self.inner.ir());
                        let env = EvalEnv {
                            context,
                            runtime: self,
                            expr_tx: expr_tx.clone(),
                            evaluated_cache: Arc::new(std::sync::Mutex::new(HashMap::new())),
                            env_vars: env_vars.clone(),
                        };
                        let param_baml_values = params
                            .iter()
                            .map(|(k, v)| {
                                let arg_type = infer_type(v);
                                let baml_value_with_meta: BamlValueWithMeta<ExprMetadata> =
                                    match arg_type {
                                        None => Ok::<_, anyhow::Error>(
                                            BamlValueWithMeta::with_const_meta(
                                                v,
                                                (Span::fake(), None),
                                            ),
                                        ),
                                        Some(arg_type) => {
                                            let value_unit_meta: BamlValueWithMeta<()> =
                                                BamlValueWithMeta::with_const_meta(v, ());
                                            let baml_value =
                                                self.inner.ir().distribute_type_with_meta(
                                                    value_unit_meta,
                                                    arg_type,
                                                )?;
                                            let baml_value_with_meta =
                                                baml_value.map_meta_owned(|(_, field_type)| {
                                                    (Span::fake(), Some(field_type))
                                                });

                                            Ok(baml_value_with_meta)
                                        }
                                    }?;
                                Ok(Expr::Atom(baml_value_with_meta))
                            })
                            .collect::<Result<_>>()
                            .unwrap_or(vec![]); //TODO: Is it acceptable to swallow errors here?

                        let params_expr: Expr<ExprMetadata> =
                            Expr::ArgsTuple(param_baml_values, (fake_syntax_span.clone(), None));
                        let result_type = expr_fn.output.clone();
                        let fn_call_expr = Expr::App {
                            func: Arc::new(fn_expr),
                            args: Arc::new(params_expr),
                            meta: (fake_syntax_span.clone(), Some(result_type.clone())),
                            type_args: vec![],
                        };
                        let res = eval_expr::eval_to_value(&env, &fn_call_expr)
                            .await
                            .map(|v| {
                                v.map(|v| {
                                    ResponseBamlValue(v.map_meta(|_| {
                                        ResponseValueMeta(
                                            vec![],
                                            vec![],
                                            Completion::default(),
                                            result_type.clone(),
                                        )
                                    }))
                                })
                            })
                            .transpose();
                        // TODO: this is a placeholder since this is an expr fn, and there was no llm call. So this is a dummy.
                        let llm_response = LLMResponse::Success(LLMCompleteResponse {
                            client: "openai".to_string(),
                            model: "gpt-3.5-turbo".to_string(),
                            prompt: RenderedPrompt::Completion("Sample raw response".to_string()),
                            request_options: BamlMap::new(),
                            content: "Sample raw response".to_string(),
                            start_time: SystemTime::now(),
                            latency: Duration::from_millis(2025),
                            metadata: LLMCompleteResponseMetadata {
                                baml_is_complete: true,
                                finish_reason: Some("stop".to_string()),
                                prompt_tokens: Some(50),
                                output_tokens: Some(50),
                                total_tokens: Some(100),
                            },
                        });
                        Ok(FunctionResult::new(
                            OrchestrationScope { scope: vec![] },
                            llm_response,
                            res,
                        ))
                    }
                }
                Err(e) => Err(e),
            };

        #[cfg(not(target_arch = "wasm32"))]
        match self
            .tracer_wrapper
            .get_or_create_tracer(&env_vars)
            .finish_baml_call(call, ctx, &response)
        {
            Ok(id) => {}
            Err(e) => baml_log::error!("Error during logging: {}", e),
        }
        #[cfg(target_arch = "wasm32")]
        match self
            .tracer_wrapper
            .get_or_create_tracer(&env_vars)
            .finish_baml_call(call, ctx, &response)
            .await
        {
            Ok(id) => {}
            Err(e) => log::error!("Error during logging: {}", e),
        }

        (response, curr_call_id)
    }

    pub fn stream_function_with_expr_events(
        &self,
        function_name: String,
        params: &BamlMap<String, BamlValue>,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        collectors: Option<Vec<Arc<Collector>>>,
        env_vars: HashMap<String, String>,
        expr_tx: Option<mpsc::UnboundedSender<Vec<SerializedSpan>>>,
    ) -> Result<FunctionResultStream> {
        baml_log::set_from_env(&env_vars).unwrap();
        self.inner.stream_function_impl(
            function_name,
            params,
            self.tracer_wrapper.get_or_create_tracer(&env_vars),
            ctx.create_ctx(tb, cb, env_vars, ctx.call_id_stack(true)?)?,
            #[cfg(not(target_arch = "wasm32"))]
            self.async_runtime.clone(),
            collectors.unwrap_or_else(|| vec![]),
        )
    }

    pub fn stream_function(
        &self,
        function_name: String,
        params: &BamlMap<String, BamlValue>,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        collectors: Option<Vec<Arc<Collector>>>,
        env_vars: HashMap<String, String>,
    ) -> Result<FunctionResultStream> {
        self.stream_function_with_expr_events(
            function_name,
            params,
            ctx,
            tb,
            cb,
            collectors,
            env_vars,
            None,
        )
    }

    pub async fn build_request(
        &self,
        function_name: String,
        params: &BamlMap<String, BamlValue>,
        context_manager: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        env_vars: HashMap<String, String>,
        stream: bool,
    ) -> Result<HTTPRequest> {
        baml_log::set_from_env(&env_vars).unwrap();
        let ctx = context_manager.create_ctx(tb, cb, env_vars, vec![])?;

        let provider = self.llm_provider_from_function(&function_name, &ctx)?;

        let prompt = self
            .render_prompt(&function_name, &ctx, &params, None)
            .await
            .map(|(prompt, ..)| prompt)?;

        let request = match prompt {
            RenderedPrompt::Chat(chat) => provider
                .build_request(either::Either::Right(&chat), true, stream, &ctx, self)
                .await?
                .build()?,

            RenderedPrompt::Completion(completion) => provider
                .build_request(either::Either::Left(&completion), true, stream, &ctx, self)
                .await?
                .build()?,
        };

        // TODO: Too much work to get the requeset body, we're building a serde
        // map and then serialize it into bytes and then parse it back again
        // into a map. We can extract the initial map directly if we refactor
        // the `build_request` method.
        //
        // Would also be nice if RequestBuilder had getters so we didn't have to
        // call .build()? above.
        Ok(HTTPRequest::new(
            HttpRequestId::new(),
            request.url().to_string(),
            request.method().to_string(),
            json_headers(request.headers()),
            HTTPBody::new(
                request
                    .body()
                    .map(reqwest::Body::as_bytes)
                    .flatten()
                    .unwrap_or_default()
                    .into(),
            ),
        ))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn build_request_sync(
        &self,
        function_name: String,
        params: &BamlMap<String, BamlValue>,
        context_manager: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        stream: bool,
        env_vars: HashMap<String, String>,
    ) -> Result<HTTPRequest> {
        let fut = self.build_request(
            function_name,
            params,
            context_manager,
            tb,
            cb,
            env_vars,
            stream,
        );
        self.async_runtime.block_on(fut)
    }

    // TODO: Should this have an async version? Parse in a different thread and
    // allow the async runtime to schedule other futures? Do it only if the
    // input is very large?
    pub fn parse_llm_response(
        &self,
        function_name: String,
        llm_response: String,
        allow_partials: bool,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        env_vars: HashMap<String, String>,
    ) -> Result<ResponseBamlValue> {
        baml_log::set_from_env(&env_vars).unwrap();
        let ctx = ctx.create_ctx(tb, cb, env_vars, vec![])?;

        let renderer = PromptRenderer::from_function(
            &self.inner.get_function(&function_name)?,
            self.inner.ir(),
            &ctx,
        )?;

        renderer.parse(&self.inner.ir(), &ctx, &llm_response, allow_partials)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn generate_client(
        &self,
        client_type: &GeneratorOutputType,
        args: &generators_lib::GeneratorArgs,
    ) -> Result<internal_baml_codegen::GenerateOutput> {
        generators_lib::generate_sdk(self.inner.ir.clone(), args)?;

        todo!()
    }
}

// Interfaces for generators
impl BamlRuntime {
    pub fn function_names(&self) -> impl Iterator<Item = &str> {
        self.inner.ir().function_names()
    }

    /// Determine the file containing the generators.
    pub fn generator_path(&self) -> Option<PathBuf> {
        let path_counts: HashMap<&PathBuf, u32> = self
            .inner
            .ir()
            .configuration()
            .generators
            .iter()
            .map(|generator| match generator {
                Generator::BoundaryCloud(generator) => generator.span.file.path_buf(),
                Generator::Codegen(generator) => generator.span.file.path_buf(),
            })
            .fold(HashMap::new(), |mut acc, path| {
                *acc.entry(path).or_default() += 1;
                acc
            });

        path_counts
            .into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(path, _)| path.clone())
    }

    pub fn cloud_projects(&self) -> Vec<&CloudProject> {
        self.inner
            .ir()
            .configuration()
            .generators
            .iter()
            .filter_map(|generator| match generator {
                Generator::BoundaryCloud(generator) => Some(generator),
                Generator::Codegen(_) => None,
            })
            .collect()
    }

    pub fn codegen_generators(&self) -> impl Iterator<Item = &CodegenGenerator> {
        self.inner
            .ir()
            .configuration()
            .generators
            .iter()
            .filter_map(|generator| match generator {
                Generator::Codegen(generator) => Some(generator),
                Generator::BoundaryCloud(_) => None,
            })
    }

    pub fn run_codegen(
        &self,
        input_files: &IndexMap<PathBuf, String>,
        no_version_check: bool,
    ) -> Result<Vec<internal_baml_codegen::GenerateOutput>> {
        use internal_baml_codegen::GenerateClient;

        let client_types: Vec<(&CodegenGenerator, internal_baml_codegen::GeneratorArgs)> = self
            .codegen_generators()
            .map(|generator| {
                Ok((
                    generator,
                    internal_baml_codegen::GeneratorArgs::new(
                        generator.output_dir(),
                        generator.baml_src.clone(),
                        input_files.iter(),
                        generator.version.clone(),
                        no_version_check,
                        generator.default_client_mode(),
                        generator.on_generate.clone(),
                        generator.output_type,
                        generator.client_package_name.clone(),
                        generator.module_format,
                    )?,
                ))
            })
            .collect::<Result<_>>()
            .context("Internal error: failed to collect generators")?;

        // VSCode / WASM can't run "on_generate", so if any generator specifies on_generate,
        // we disable codegen. (This can be super surprising behavior to someone, but we'll cross
        // that bridge when we get there)
        cfg_if! {
            if #[cfg(target_arch = "wasm32")] {
                if client_types
                    .iter()
                    .any(|(g, _)| !g.on_generate.is_empty())
                {
                    // We could also return an error here, but what really matters is whether we message
                    // the user about it in VSCode. IMO the user shouldn't get a message about "vscode not
                    // generating code, run 'baml-cli dev' to generate code" because that's surprising
                    //
                    // We _could_ do something like "show that message the first time the user tries to
                    // codegen for rest/openapi", but that's overengineered, I think
                    return Ok(vec![]);
                }
            }
        }

        client_types
            .iter()
            .map(|(generator, args)| {
                let files = generators_lib::generate_sdk(self.inner.ir.clone(), args)?;
                Ok(internal_baml_codegen::GenerateOutput {
                    client_type: generator.output_type,
                    output_dir_shorthand: generator.output_dir(),
                    output_dir_full: generator.output_dir(),
                    files,
                })
            })
            .collect()
    }
}

impl<'a> InternalClientLookup<'a> for BamlRuntime {
    fn get_llm_provider(
        &'a self,
        client_spec: &ClientSpec,
        ctx: &RuntimeContext,
    ) -> Result<Arc<LLMProvider>> {
        self.inner.get_llm_provider(client_spec, ctx)
    }

    fn get_retry_policy(&self, policy_name: &str, ctx: &RuntimeContext) -> Result<CallablePolicy> {
        self.inner.get_retry_policy(policy_name, ctx)
    }
}

// These are used by Python and TS etc to trace Py/TS functions. Not baml ones.
impl ExperimentalTracingInterface for BamlRuntime {
    fn start_call(
        &self,
        function_name: &str,
        params: &BamlMap<String, BamlValue>,
        ctx: &RuntimeContextManager,
        env_vars: &HashMap<String, String>,
    ) -> TracingCall {
        self.tracer_wrapper
            .get_or_create_tracer(env_vars)
            .start_call(function_name, ctx, params, false, false, None)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn finish_function_call(
        &self,
        call: TracingCall,
        result: &Result<FunctionResult>,
        ctx: &RuntimeContextManager,
        env_vars: &HashMap<String, String>,
    ) -> Result<uuid::Uuid> {
        self.tracer_wrapper
            .get_or_create_tracer(env_vars)
            .finish_baml_call(call, ctx, result)
            .map(|r| r.0)
    }

    #[cfg(target_arch = "wasm32")]
    async fn finish_function_call(
        &self,
        call: TracingCall,
        result: &Result<FunctionResult>,
        ctx: &RuntimeContextManager,
        env_vars: &HashMap<String, String>,
    ) -> Result<uuid::Uuid> {
        self.tracer_wrapper
            .get_or_create_tracer(env_vars)
            .finish_baml_call(call, ctx, result)
            .await
            .map(|r| r.0)
    }

    // For non-LLM calls -- used by FFI boundary like with @trace in python
    #[cfg(not(target_arch = "wasm32"))]
    fn finish_call(
        &self,
        call: TracingCall,
        result: Option<BamlValue>,
        ctx: &RuntimeContextManager,
        env_vars: &HashMap<String, String>,
    ) -> Result<uuid::Uuid> {
        self.tracer_wrapper
            .get_or_create_tracer(env_vars)
            .finish_call(call, ctx, result)
    }

    #[cfg(target_arch = "wasm32")]
    async fn finish_call(
        &self,
        call: TracingCall,
        result: Option<BamlValue>,
        ctx: &RuntimeContextManager,
        env_vars: &HashMap<String, String>,
    ) -> Result<uuid::Uuid> {
        self.tracer_wrapper
            .get_or_create_tracer(env_vars)
            .finish_call(call, ctx, result)
            .await
    }

    fn flush(&self) -> Result<()> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Err(e) = self.async_runtime.block_on(flush()) {
                baml_log::debug!("Failed to flush: {}", e);
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = wasm_bindgen_futures::spawn_local(async move {
                if let Err(e) = flush().await {
                    baml_log::error!("Failed to flush: {}", e);
                }
            });
        }
        self.tracer_wrapper.get_tracer().flush()
    }

    fn drain_stats(&self) -> InnerTraceStats {
        self.tracer_wrapper.get_tracer().drain_stats()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn set_log_event_callback(
        &self,
        log_event_callback: Option<LogEventCallbackSync>,
    ) -> Result<()> {
        self.tracer_wrapper
            .get_tracer()
            .set_log_event_callback(log_event_callback);
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn baml_src_files(dir: &std::path::PathBuf) -> Result<Vec<PathBuf>> {
    static VALID_EXTENSIONS: [&str; 1] = ["baml"];

    log::trace!("Reading files from {:#}", dir.to_string_lossy());

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

    Ok(src_files)
}

/// The function name requested by the user may be an expression function or an LLM function.
///
/// If it's an LLM function, just return the function name and params.
///
/// If it's an expression function, determine whether it evaluates to an LLM function,
/// and if so, return that function and its params. Not all expr functios evaluate to
/// a (single) LLM function. So in those cases, just return the final value. (for example,
/// some expression functions compute a list of values that are each the result of an LLM
/// function - this can't be streamed, it can only be returned as a whole list).
async fn expr_eval_result(
    runtime: &BamlRuntime,
    mgr: &RuntimeContextManager,
    expr_tx: Option<mpsc::UnboundedSender<Vec<SerializedSpan>>>,
    collector: Option<Arc<Collector>>,
    tracer: Arc<BamlTracer>,
    tb: Option<&TypeBuilder>,
    cb: Option<&ClientRegistry>,
    env_vars: HashMap<String, String>,
    function_name: &str,
    params: &BamlMap<String, BamlValue>,
) -> Result<ExprEvalResult> {
    let fake_syntax_span = Span::fake();
    let ir = runtime.inner.ir();
    let is_expr_fn = ir.find_expr_fn(function_name).is_ok();
    let maybe_expr_f = ir.find_expr_fn(function_name);
    match maybe_expr_f {
        Ok(expr_fn) => {
            log::trace!("Calling function: {}", function_name);
            let collectors = collector.as_ref().map(|c| vec![c.clone()]);
            let call = tracer.start_call(&function_name, mgr, params, true, false, collectors);

            let ctx = mgr.create_ctx(tb, cb, env_vars.clone(), call.new_call_id_stack.clone())?;
            let env = EvalEnv {
                context: initial_context(ir),
                runtime,
                expr_tx: expr_tx.clone(),
                evaluated_cache: Arc::new(std::sync::Mutex::new(HashMap::new())),
                env_vars,
            };

            let param_baml_values = params
                .iter()
                .map(|(k, v)| {
                    let arg_type = infer_type(v);
                    let baml_value_with_meta: BamlValueWithMeta<ExprMetadata> = match arg_type {
                        None => Ok::<_, anyhow::Error>(BamlValueWithMeta::with_const_meta(
                            v,
                            (Span::fake(), None),
                        )),
                        Some(arg_type) => {
                            let value_unit_meta: BamlValueWithMeta<()> =
                                BamlValueWithMeta::with_const_meta(v, ());
                            let baml_value = runtime
                                .inner
                                .ir()
                                .distribute_type_with_meta(value_unit_meta, arg_type)?;
                            let baml_value_with_meta = baml_value
                                .map_meta_owned(|(_, field_type)| (Span::fake(), Some(field_type)));

                            Ok(baml_value_with_meta)
                        }
                    }?;
                    Ok(Expr::Atom(baml_value_with_meta))
                })
                .collect::<Result<_>>()
                .unwrap_or(vec![]); //TODO: Is it acceptable to swallow errors here?
            let params_expr: Expr<ExprMetadata> =
                Expr::ArgsTuple(param_baml_values, (fake_syntax_span.clone(), None));
            let result_type = expr_fn.elem().output.clone();
            let fn_call_expr = Expr::App {
                func: Arc::new(expr_fn.elem().expr.clone()),
                type_args: vec![],
                args: Arc::new(params_expr),
                meta: (fake_syntax_span.clone(), Some(result_type.clone())),
            };
            let res = eval_expr::eval_to_value_or_llm_call(&env, &fn_call_expr).await?;
            Ok(res)
        }
        Err(e) => Ok(ExprEvalResult::LLMCall {
            name: function_name.to_string(),
            args: params.clone(),
        }),
    }
}
