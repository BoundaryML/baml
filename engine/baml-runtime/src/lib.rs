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

pub mod tracing;
pub mod tracingv2;
pub mod type_builder;
mod types;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use eval_expr::ExprEvalResult;
use futures::channel::mpsc;
use internal_baml_core::ast::Span;
use internal_baml_core::ir::repr::initial_context;
use internal_baml_core::ir::repr::IntermediateRepr;
use jsonish::ResponseValueMeta;
use tokio::sync::Mutex;

use crate::internal::llm_client::LLMCompleteResponse;
use baml_types::expr::{Expr, ExprMetadata};
use baml_types::tracing::events::FunctionId;
use baml_types::tracing::events::HTTPBody;
use baml_types::tracing::events::HTTPRequest;
use baml_types::tracing::events::HttpRequestId;
use baml_types::BamlMap;
use baml_types::BamlValue;
use baml_types::BamlValueWithMeta;
use baml_types::Completion;
use baml_types::Constraint;
use cfg_if::cfg_if;
use client_registry::ClientRegistry;
use eval_expr::EvalEnv;
use futures::future::join;
use futures::future::join_all;
use indexmap::IndexMap;
use internal::llm_client::llm_provider::LLMProvider;
use internal::llm_client::orchestrator::OrchestrationScope;
use internal::llm_client::primitive::json_body;
use internal::llm_client::primitive::json_headers;
use internal::llm_client::primitive::JsonBodyInput;
use internal::llm_client::retry_policy::CallablePolicy;
use internal::prompt_renderer::PromptRenderer;
use internal_baml_core::configuration::CloudProject;
use internal_baml_core::configuration::CodegenGenerator;
use internal_baml_core::configuration::Generator;
use internal_baml_core::configuration::GeneratorOutputType;
use internal_baml_core::ir::FunctionWalker;
use internal_baml_core::ir::IRHelperExtended;
use internal_llm_client::AllowedRoleMetadata;
use internal_llm_client::ClientSpec;
use jsonish::ResponseBamlValue;
use on_log_event::LogEventCallbackSync;
use runtime::InternalBamlRuntime;
use runtime_interface::InternalClientLookup;
use serde_json::json;
use std::sync::OnceLock;
use tracingv2::storage::storage::Collector;
use tracingv2::storage::storage::BAML_TRACER;
use web_time::SystemTime;

use crate::internal::llm_client::LLMCompleteResponseMetadata;
#[cfg(not(target_arch = "wasm32"))]
pub use cli::RuntimeCliDefaults;
pub use runtime_context::BamlSrcReader;
use runtime_interface::ExperimentalTracingInterface;
use runtime_interface::RuntimeConstructor;
use runtime_interface::RuntimeInterface;
use tracing::{BamlTracer, TracingSpan};
use type_builder::TypeBuilder;
pub use types::*;
use web_time::Duration;

#[cfg(feature = "internal")]
pub use internal_baml_jinja::{ChatMessagePart, RenderedPrompt};
#[cfg(feature = "internal")]
pub use runtime_interface::InternalRuntimeInterface;

#[cfg(not(feature = "internal"))]
pub(crate) use internal_baml_jinja::{ChatMessagePart, RenderedPrompt};
#[cfg(not(feature = "internal"))]
pub(crate) use runtime_interface::InternalRuntimeInterface;

pub use internal_baml_core::internal_baml_diagnostics;
pub use internal_baml_core::internal_baml_diagnostics::Diagnostics as DiagnosticsError;
pub use internal_baml_core::internal_baml_diagnostics::SerializedSpan;
pub use internal_baml_core::ir::{
    ir_helpers::infer_type, scope_diagnostics, FieldType, IRHelper, TypeValue,
};

use crate::internal::llm_client::LLMResponse;
use crate::test_constraints::{evaluate_test_constraints, TestConstraintsResult};

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

#[derive(Clone)]
pub struct BamlRuntime {
    pub inner: InternalBamlRuntime,
    tracer: Arc<BamlTracer>,
    env_vars: HashMap<String, String>,
    #[cfg(not(target_arch = "wasm32"))]
    pub async_runtime: Arc<tokio::runtime::Runtime>,
}

impl BamlRuntime {
    pub fn env_vars(&self) -> &HashMap<String, String> {
        &self.env_vars
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn get_tokio_singleton() -> Result<Arc<tokio::runtime::Runtime>> {
        match TOKIO_SINGLETON.get_or_init(|| tokio::runtime::Runtime::new().map(Arc::new)) {
            Ok(t) => Ok(t.clone()),
            Err(e) => Err(e.into()),
        }
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

        Ok(BamlRuntime {
            inner: InternalBamlRuntime::from_directory(&path)?,
            tracer: BamlTracer::new(None, env_vars.into_iter())?.into(),
            env_vars: copy,
            #[cfg(not(target_arch = "wasm32"))]
            async_runtime: Self::get_tokio_singleton()?,
        })
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

        let inner = InternalBamlRuntime::from_file_content(root_path, files)?;

        Ok(BamlRuntime {
            inner,
            tracer: BamlTracer::new(None, env_vars.into_iter())?.into(),
            env_vars: copy,
            #[cfg(not(target_arch = "wasm32"))]
            async_runtime: Self::get_tokio_singleton()?,
        })
    }

    #[cfg(feature = "internal")]
    pub fn internal(&self) -> &impl InternalRuntimeInterface {
        &self.inner
    }

    pub fn create_ctx_manager(
        &self,
        language: BamlValue,
        // A callback that can be implemented in JS to read files that are referred in tests.
        baml_src_reader: BamlSrcReader,
    ) -> RuntimeContextManager {
        let ctx = RuntimeContextManager::new_from_env_vars(self.env_vars.clone(), baml_src_reader);
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
        let ctx = RuntimeContextManager::new_from_env_vars(self.env_vars.clone(), baml_src_reader);
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
            &self.inner.get_function(&function_name, &ctx)?,
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
        expr_tx: Option<mpsc::UnboundedSender<Vec<SerializedSpan>>>,
        collector: Option<Arc<Collector>>,
    ) -> (Result<TestResponse>, Option<uuid::Uuid>)
    where
        F: Fn(FunctionResult),
    {
        let span = self.tracer.start_span(test_name, ctx, &Default::default());

        let expr_fn = self.inner.ir().find_expr_fn(function_name);
        let is_expr_fn = expr_fn.is_ok();

        if let Some(span) = span.clone() {
            if let Some(collector) = collector.clone() {
                collector.track_function(FunctionId(span.clone().span_id.to_string()));
            }
        }

        let run_to_response = || async {
            let rctx_no_tb = ctx.create_ctx(None, None, span.clone().map(|s| s.span_id))?;
            let (params, constraints) =
                self.get_test_params_and_constraints(function_name, test_name, &rctx_no_tb, true)?;

            // Run the expression to either a value or a final LLM call.
            // (If it's not an expr fn, it'll be a final LLM call.)
            let expr_eval_result = expr_eval_result(
                self,
                ctx,
                expr_tx.clone(),
                collector.clone(),
                self.tracer.clone(),
                None,
                None,
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
                    .get_test_type_builder(&function_name, test_name, ctx)
                    .unwrap()
            };

            let rctx =
                ctx.create_ctx(type_builder.as_ref(), None, span.clone().map(|s| s.span_id))?;

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
                        function_span: None,
                        constraints_result: TestConstraintsResult::empty(),
                    });
                }
                ExprEvalResult::LLMCall { name, args } => (name, args),
            };

            let mut stream = self.inner.stream_function_impl(
                function_name.into(),
                &params,
                self.tracer.clone(),
                rctx,
                #[cfg(not(target_arch = "wasm32"))]
                self.async_runtime.clone(),
                // TODO: collectors here?
                vec![],
            )?;
            let (response_res, span_uuid) =
                stream.run(on_event, ctx, type_builder.as_ref(), None).await;
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
                function_span: span_uuid,
                constraints_result: test_constraints_result,
            })
        };

        let response = run_to_response().await;

        let mut target_id = None;
        if let Some(span) = span {
            #[cfg(not(target_arch = "wasm32"))]
            match self.tracer.finish_span(span, ctx, None) {
                Ok(id) => target_id = id,
                Err(e) => log::debug!("Error during logging: {}", e),
            }
            #[cfg(target_arch = "wasm32")]
            match self.tracer.finish_span(span, ctx, None).await {
                Ok(id) => target_id = id,
                Err(e) => log::debug!("Error during logging: {}", e),
            }
        }

        (response, target_id)
    }

    pub async fn run_test<F>(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContextManager,
        on_event: Option<F>,
        collector: Option<Arc<Collector>>,
    ) -> (Result<TestResponse>, Option<uuid::Uuid>)
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
    ) -> (Result<FunctionResult>, Option<uuid::Uuid>) {
        let fut = self.call_function(function_name, params, ctx, tb, cb, collectors);
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
    ) -> (Result<FunctionResult>, Option<uuid::Uuid>) {
        let res = Box::pin(self.call_function_with_expr_events(
            function_name,
            params,
            ctx,
            tb,
            cb,
            collectors,
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
        expr_tx: Option<mpsc::UnboundedSender<Vec<SerializedSpan>>>,
    ) -> (Result<FunctionResult>, Option<uuid::Uuid>) {
        log::trace!("Calling function: {}", function_name);
        let span = self.tracer.start_span(&function_name, ctx, params);

        if let Some(span) = span.clone() {
            if let Some(collectors) = collectors {
                for collector in collectors.iter() {
                    collector.track_function(FunctionId(span.clone().span_id.to_string()));
                }
            }
        }

        let fake_syntax_span = Span::fake();
        let response = match ctx.create_ctx(tb, cb, span.clone().map(|s| s.span_id)) {
            Ok(rctx) => {
                let is_expr_fn = self
                    .inner
                    .ir()
                    .expr_fns
                    .iter()
                    .find(|f| f.elem.name == function_name)
                    .is_some();
                if !is_expr_fn {
                    self.inner
                        .call_function_impl(function_name, params, rctx)
                        .await
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
                    };
                    let param_baml_values = params
                        .iter()
                        .map(|(k, v)| {
                            let arg_type = infer_type(v);
                            let baml_value_with_meta: BamlValueWithMeta<ExprMetadata> =
                                match arg_type {
                                    None => Ok::<_, anyhow::Error>(
                                        BamlValueWithMeta::with_const_meta(v, (Span::fake(), None)),
                                    ),
                                    Some(arg_type) => {
                                        let value_unit_meta: BamlValueWithMeta<()> =
                                            BamlValueWithMeta::with_const_meta(v, ());
                                        let baml_value = self
                                            .inner
                                            .ir()
                                            .distribute_type_with_meta(value_unit_meta, arg_type)?;
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
                    let fn_call_expr = Expr::App(
                        Arc::new(fn_expr),
                        Arc::new(params_expr),
                        (fake_syntax_span.clone(), Some(result_type.clone())),
                    );
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

        let mut target_id = None;
        if let Some(span) = span {
            #[cfg(not(target_arch = "wasm32"))]
            match self.tracer.finish_baml_span(span, ctx, &response) {
                Ok(id) => target_id = id,
                Err(e) => log::debug!("Error during logging: {}", e),
            }
            #[cfg(target_arch = "wasm32")]
            match self.tracer.finish_baml_span(span, ctx, &response).await {
                Ok(id) => target_id = id,
                Err(e) => log::debug!("Error during logging: {}", e),
            }
        }
        (response, target_id)
    }

    pub fn stream_function_with_expr_events(
        &self,
        function_name: String,
        params: &BamlMap<String, BamlValue>,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        collectors: Option<Vec<Arc<Collector>>>,
        expr_tx: Option<mpsc::UnboundedSender<Vec<SerializedSpan>>>,
    ) -> Result<FunctionResultStream> {
        self.inner.stream_function_impl(
            function_name,
            params,
            self.tracer.clone(),
            ctx.create_ctx(tb, cb, None)?,
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
    ) -> Result<FunctionResultStream> {
        self.stream_function_with_expr_events(function_name, params, ctx, tb, cb, collectors, None)
    }

    pub async fn build_request(
        &self,
        function_name: String,
        params: &BamlMap<String, BamlValue>,
        context_manager: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        stream: bool,
    ) -> Result<HTTPRequest> {
        let ctx = context_manager.create_ctx(tb, cb, None)?;

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
        Ok(HTTPRequest {
            id: HttpRequestId(uuid::Uuid::new_v4().to_string()),
            url: request.url().to_string(),
            method: request.method().to_string(),
            headers: json_headers(request.headers()),
            body: HTTPBody::new(
                request
                    .body()
                    .map(reqwest::Body::as_bytes)
                    .flatten()
                    .unwrap_or_default()
                    .into(),
            ),
        })
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
    ) -> Result<HTTPRequest> {
        let fut = self.build_request(function_name, params, context_manager, tb, cb, stream);
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
    ) -> Result<ResponseBamlValue> {
        let ctx = ctx.create_ctx(tb, cb, None)?;

        let renderer = PromptRenderer::from_function(
            &self.inner.get_function(&function_name, &ctx)?,
            self.inner.ir(),
            &ctx,
        )?;

        renderer.parse(&self.inner.ir(), &ctx, &llm_response, allow_partials)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn generate_client(
        &self,
        client_type: &GeneratorOutputType,
        args: &internal_baml_codegen::GeneratorArgs,
    ) -> Result<internal_baml_codegen::GenerateOutput> {
        use internal_baml_codegen::GenerateClient;

        client_type.generate_client(self.inner.ir(), args)
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
                generator
                    .output_type
                    .generate_client(self.inner.ir(), args)
                    .with_context(|| {
                        let err_msg = format!(
                            "Error while running generator defined at {}:{}:{}",
                            generator.span.file.path(),
                            generator.span.line_and_column().0 .0,
                            generator.span.line_and_column().0 .1
                        );
                        log::error!("{}", err_msg);
                        err_msg
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

impl ExperimentalTracingInterface for BamlRuntime {
    fn start_span(
        &self,
        function_name: &str,
        params: &BamlMap<String, BamlValue>,
        ctx: &RuntimeContextManager,
    ) -> Option<TracingSpan> {
        self.tracer.start_span(function_name, ctx, params)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn finish_function_span(
        &self,
        span: Option<TracingSpan>,
        result: &Result<FunctionResult>,
        ctx: &RuntimeContextManager,
    ) -> Result<Option<uuid::Uuid>> {
        if let Some(span) = span {
            self.tracer.finish_baml_span(span, ctx, result)
        } else {
            Ok(None)
        }
    }

    #[cfg(target_arch = "wasm32")]
    async fn finish_function_span(
        &self,
        span: Option<TracingSpan>,
        result: &Result<FunctionResult>,
        ctx: &RuntimeContextManager,
    ) -> Result<Option<uuid::Uuid>> {
        if let Some(span) = span {
            self.tracer.finish_baml_span(span, ctx, result).await
        } else {
            Ok(None)
        }
    }

    // For non-LLM calls -- used by FFI boundary like with @trace in python
    #[cfg(not(target_arch = "wasm32"))]
    fn finish_span(
        &self,
        span: Option<TracingSpan>,
        result: Option<BamlValue>,
        ctx: &RuntimeContextManager,
    ) -> Result<Option<uuid::Uuid>> {
        if let Some(span) = span {
            self.tracer.finish_span(span, ctx, result)
        } else {
            Ok(None)
        }
    }

    #[cfg(target_arch = "wasm32")]
    async fn finish_span(
        &self,
        span: Option<TracingSpan>,
        result: Option<BamlValue>,
        ctx: &RuntimeContextManager,
    ) -> Result<Option<uuid::Uuid>> {
        if let Some(span) = span {
            self.tracer.finish_span(span, ctx, result).await
        } else {
            Ok(None)
        }
    }

    fn flush(&self) -> Result<()> {
        self.tracer.flush()
    }

    fn drain_stats(&self) -> InnerTraceStats {
        self.tracer.drain_stats()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn set_log_event_callback(
        &self,
        log_event_callback: Option<LogEventCallbackSync>,
    ) -> Result<()> {
        self.tracer.set_log_event_callback(log_event_callback);
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
            let span = tracer.start_span(&function_name, mgr, params);

            if let Some(span) = span.clone() {
                if let Some(collector) = collector {
                    collector.track_function(FunctionId(span.clone().span_id.to_string()));
                }
            }
            let ctx = mgr.create_ctx(tb, cb, span.clone().map(|s| s.span_id))?;
            let env = EvalEnv {
                context: initial_context(ir),
                runtime,
                expr_tx: expr_tx.clone(),
                evaluated_cache: Arc::new(std::sync::Mutex::new(HashMap::new())),
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
            let fn_call_expr = Expr::App(
                Arc::new(expr_fn.elem().expr.clone()),
                Arc::new(params_expr),
                (fake_syntax_span.clone(), Some(result_type.clone())),
            );
            let res = eval_expr::eval_to_value_or_llm_call(&env, &fn_call_expr).await?;
            Ok(res)
        }
        Err(e) => Ok(ExprEvalResult::LLMCall {
            name: function_name.to_string(),
            args: params.clone(),
        }),
    }
}
