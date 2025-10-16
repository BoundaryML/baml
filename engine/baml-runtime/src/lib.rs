// mod tests;

#[cfg(feature = "internal")]
pub mod internal;
#[cfg(not(feature = "internal"))]
pub(crate) mod internal;

#[cfg(not(target_arch = "wasm32"))]
pub mod cli;
pub mod client_registry;
pub mod errors;
pub mod request;
pub mod runtime;
pub mod runtime_interface;
pub mod test_constraints;
#[cfg(not(target_arch = "wasm32"))]
pub mod test_executor;

pub mod async_interpreter_runtime;
pub mod async_vm_runtime;
mod redaction;
mod runtime_methods;
pub mod tracing;
pub mod tracingv2;
pub mod type_builder;
mod types;

// Conditional runtime selection based on the "interpreter" feature flag
use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
};

use anyhow::{Context, Result};
#[cfg(feature = "interpreter")]
use async_interpreter_runtime::BamlAsyncInterpreterRuntime as CoreRuntime;
#[cfg(not(feature = "interpreter"))]
use async_vm_runtime::BamlAsyncVmRuntime as CoreRuntime;
use baml_ids::{FunctionCallId, HttpRequestId};
use baml_types::{
    expr::{Expr, ExprMetadata},
    tracing::events::{ClientDetails, HTTPBody, HTTPRequest, TraceEvent},
    BamlMap, BamlValue, BamlValueWithMeta, Completion, Constraint,
};
use cfg_if::cfg_if;
#[cfg(not(target_arch = "wasm32"))]
pub use cli::RuntimeCliDefaults;
use client_registry::{ClientProperty, ClientRegistry};
use futures::{
    channel::mpsc,
    future::{join, join_all},
};
use generators_lib::{
    version_check::{self, GeneratorType, VersionCheckMode},
    GenerateOutput, GeneratorArgs,
};
use indexmap::IndexMap;
use internal::{
    llm_client::{
        llm_provider::LLMProvider,
        orchestrator::{IterOrchestrator, OrchestrationScope},
        primitive::{json_body, json_headers, JsonBodyInput, LLMPrimitiveProvider},
        retry_policy::CallablePolicy,
        traits::{WithClientProperties, WithPrompt, WithRenderRawCurl},
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
    internal_baml_parser_database::ParserDatabase,
    ir::{ir_helpers::infer_type, scope_diagnostics, IRHelper, TypeIR, TypeValue},
};
#[cfg(feature = "internal")]
pub use internal_baml_jinja::{ChatMessagePart, RenderedPrompt};
#[cfg(not(feature = "internal"))]
pub(crate) use internal_baml_jinja::{ChatMessagePart, RenderedPrompt};
use internal_llm_client::{AllowedRoleMetadata, ClientSpec};
use jsonish::{ResponseBamlValue, ResponseValueMeta};
use on_log_event::LogEventCallbackSync;
pub use runtime_context::BamlSrcReader;
#[cfg(feature = "internal")]
pub use runtime_interface::InternalRuntimeInterface;
#[cfg(not(feature = "internal"))]
pub(crate) use runtime_interface::InternalRuntimeInterface;
use runtime_interface::{ExperimentalTracingInterface, RuntimeConstructor};
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
        let filtered = Self::filter_relevant_env_vars(env_vars);
        let key = Self::env_vars_key(env_vars);
        let tracer = Arc::new(BamlTracer::new(None, filtered.clone().into_iter())?);

        #[cfg(target_arch = "wasm32")]
        {
            let tracers = Arc::new(Mutex::new(HashMap::new()));
            tracers.lock().unwrap().insert(key, tracer);
            Ok(Self { tracers })
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let tracers = DashMap::new();
            tracers.insert(key, tracer);
            Ok(Self { tracers })
        }
    }

    /// Get the tracer for the given env vars, creating a new one if the config changed.
    pub fn get_or_create_tracer(&self, env_vars: &HashMap<String, String>) -> Arc<BamlTracer> {
        let filtered = Self::filter_relevant_env_vars(env_vars);
        let key = Self::env_vars_key(env_vars);

        #[cfg(target_arch = "wasm32")]
        {
            let mut tracers = self.tracers.lock().unwrap();
            if let Some(existing) = tracers.get(&key) {
                if existing.config_matches_env_vars(&filtered) {
                    return existing.clone();
                }
            }
            // Config changed, clear all and insert new
            tracers.clear();
            let new_tracer = Arc::new(
                BamlTracer::new(None, filtered.clone().into_iter())
                    .expect("Failed to create BamlTracer"),
            );
            tracers.insert(key, new_tracer.clone());
            new_tracer
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
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
    }

    /// Get the current tracer (the only one in the map, if any).
    pub fn get_tracer(&self) -> Arc<BamlTracer> {
        #[cfg(target_arch = "wasm32")]
        {
            let tracers = self.tracers.lock().unwrap();
            tracers.values().next().expect("No tracer found").clone()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.tracers.iter().next().expect("No tracer found").clone()
        }
    }
}

cfg_if::cfg_if!(
    if #[cfg(target_arch = "wasm32")] {
        type DashMap<K, V> = std::sync::Arc<std::sync::Mutex<std::collections::HashMap<K, V>>>;
    } else {
        use dashmap::DashMap;
    }
);

#[derive(Clone)]
pub struct BamlRuntime {
    // Core IR and parsing (formerly InternalBamlRuntime)
    pub ir: Arc<IntermediateRepr>,
    pub db: ParserDatabase,
    pub diagnostics: DiagnosticsError,
    clients: DashMap<String, runtime::CachedClient>,
    retry_policies: DashMap<String, CallablePolicy>,
    source_files: Vec<internal_baml_core::internal_baml_diagnostics::SourceFile>,

    // Runtime infrastructure
    pub tracer_wrapper: Arc<BamlTracerWrapper>,
    #[cfg(not(target_arch = "wasm32"))]
    pub async_runtime: Arc<tokio::runtime::Runtime>,
}

pub struct TripWire {
    trip_wire: Option<stream_cancel::Tripwire>,

    on_drop: Option<Box<dyn Fn() + 'static + Send + Sync>>,
}

impl TripWire {
    pub fn new(trip_wire: Option<stream_cancel::Tripwire>) -> Arc<Self> {
        Arc::new(Self {
            trip_wire,
            on_drop: None,
        })
    }

    pub fn new_with_on_drop(
        trip_wire: Option<stream_cancel::Tripwire>,
        on_drop: Box<dyn Fn() + 'static + Send + Sync>,
    ) -> Arc<Self> {
        Arc::new(Self {
            trip_wire,
            on_drop: Some(on_drop),
        })
    }

    fn trip_wire(&self) -> Option<stream_cancel::Tripwire> {
        self.trip_wire.clone()
    }
}

impl Drop for TripWire {
    fn drop(&mut self) {
        if let Some(on_drop) = self.on_drop.take() {
            on_drop();
        }
    }
}

impl BamlRuntime {
    #[cfg(not(target_arch = "wasm32"))]
    fn get_tokio_singleton() -> Result<Arc<tokio::runtime::Runtime>> {
        match TOKIO_SINGLETON.get_or_init(|| tokio::runtime::Runtime::new().map(Arc::new)) {
            Ok(t) => Ok(t.clone()),
            Err(e) => Err(e.into()),
        }
    }

    fn new_runtime(
        ir: Arc<IntermediateRepr>,
        db: ParserDatabase,
        diagnostics: DiagnosticsError,
        source_files: Vec<internal_baml_core::internal_baml_diagnostics::SourceFile>,
        env_vars: &HashMap<String, String>,
    ) -> Result<Self> {
        #[cfg(not(target_arch = "wasm32"))]
        let rt = Self::get_tokio_singleton()?;

        let runtime_clone = BamlRuntime {
            ir: ir.clone(),
            db: db.clone(),
            diagnostics: diagnostics.clone(),
            clients: Default::default(),
            retry_policies: Default::default(),
            source_files: source_files.clone(),
            tracer_wrapper: Arc::new(BamlTracerWrapper::new(env_vars)?),
            #[cfg(not(target_arch = "wasm32"))]
            async_runtime: rt.clone(),
        };

        let runtime = BamlRuntime {
            ir: ir.clone(),
            db,
            diagnostics,
            clients: Default::default(),
            retry_policies: Default::default(),
            source_files,
            tracer_wrapper: Arc::new(BamlTracerWrapper::new(env_vars)?),
            #[cfg(not(target_arch = "wasm32"))]
            async_runtime: rt.clone(),
        };

        tracingv2::publisher::start_publisher(
            Arc::new(
                (Arc::new(runtime_clone), env_vars.clone())
                    .try_into()
                    .context(
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
        feature_flags: internal_baml_core::feature_flags::FeatureFlags,
    ) -> Result<Self> {
        // setup_crypto_provider();
        let path = Self::parse_baml_src_path(path)?;

        let copy = env_vars
            .iter()
            .map(|(k, v)| (k.as_ref().to_string(), v.as_ref().to_string()))
            .collect();
        baml_log::set_from_env(&copy)?;

        let files = baml_src_files(&path)?;
        let contents: Vec<internal_baml_core::internal_baml_diagnostics::SourceFile> = files
            .iter()
            .map(|path| match std::fs::read_to_string(path) {
                Ok(contents) => Ok(
                    internal_baml_core::internal_baml_diagnostics::SourceFile::from((
                        path.clone(),
                        contents,
                    )),
                ),
                Err(e) => Err(e),
            })
            .filter_map(|res| res.ok())
            .collect();
        let mut schema = internal_baml_core::validate(&path, contents.clone(), feature_flags);
        schema.diagnostics.to_result()?;

        let ir = IntermediateRepr::from_parser_database(&schema.db, schema.configuration)?;

        Self::new_runtime(Arc::new(ir), schema.db, schema.diagnostics, contents, &copy)
    }

    pub fn from_file_content<T: AsRef<str> + std::fmt::Debug, U: AsRef<str>>(
        root_path: &str,
        files: &HashMap<T, T>,
        env_vars: HashMap<U, U>,
        feature_flags: internal_baml_core::feature_flags::FeatureFlags,
    ) -> Result<Self> {
        // setup_crypto_provider();
        let copy = env_vars
            .iter()
            .map(|(k, v)| (k.as_ref().to_string(), v.as_ref().to_string()))
            .collect();
        baml_log::set_from_env(&copy)?;

        let contents = files
            .iter()
            .map(|(path, contents)| {
                Ok(
                    internal_baml_core::internal_baml_diagnostics::SourceFile::from((
                        PathBuf::from(path.as_ref()),
                        contents.as_ref().to_string(),
                    )),
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let mut schema = internal_baml_core::validate(
            &PathBuf::from(root_path),
            contents.clone(),
            feature_flags,
        );
        schema.diagnostics.to_result()?;

        let ir = IntermediateRepr::from_parser_database(&schema.db, schema.configuration)?;
        ir.validate_test_args(&mut schema.diagnostics);
        schema.diagnostics.to_result()?;

        Self::new_runtime(Arc::new(ir), schema.db, schema.diagnostics, contents, &copy)
    }

    pub fn create_ctx_manager(
        &self,
        language: BamlValue,
        // A callback that can be implemented in JS to read files that are referred in tests.
        baml_src_reader: BamlSrcReader,
    ) -> RuntimeContextManager {
        let ctx = RuntimeContextManager::new(baml_src_reader);
        let tags: HashMap<String, BamlValue> = [
            ("baml.language", language),
            (
                "baml.runtime",
                BamlValue::String(env!("CARGO_PKG_VERSION").to_string()),
            ),
        ]
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
        let tags: HashMap<String, BamlValue> = [
            (
                "baml.language".to_string(),
                BamlValue::String("wasm".to_string()),
            ),
            (
                "baml.runtime".to_string(),
                BamlValue::String(env!("CARGO_PKG_VERSION").to_string()),
            ),
        ]
        .into_iter()
        .collect();
        ctx.upsert_tags(tags);
        ctx
    }
}

impl BamlRuntime {
    pub(crate) async fn render_prompt_impl(
        &self,
        function_name: &str,
        ctx: &RuntimeContext,
        params: &BamlMap<String, BamlValue>,
        node_index: Option<usize>,
    ) -> Result<(RenderedPrompt, OrchestrationScope, AllowedRoleMetadata)> {
        let func = self.get_function(function_name)?;
        let function_params = func.inputs();
        let baml_args = self.ir().check_function_params(
            function_params,
            params,
            internal_baml_core::ir::ArgCoercer {
                span_path: None,
                allow_implicit_cast_to_string: false,
            },
        )?;

        let renderer = PromptRenderer::from_function(&func, self.ir(), ctx)?;

        let client_spec = renderer.client_spec();
        let client = self.get_llm_provider_impl(client_spec, ctx)?;
        let mut selected =
            client.iter_orchestrator(&mut Default::default(), Default::default(), ctx, self)?;
        let node_index = node_index.unwrap_or(0);

        if node_index >= selected.len() {
            return Err(anyhow::anyhow!(
                "Execution Node out of bounds (render prompt): {} >= {} for client {}",
                node_index,
                selected.len(),
                client_spec,
            ));
        }

        let baml_args =
            BamlValue::Map(baml_args.into_iter().map(|(k, v)| (k, v.value())).collect());
        let node = selected.swap_remove(node_index);
        node.provider
            .render_prompt(self.ir(), &renderer, ctx, &baml_args)
            .await
            .map(|prompt| (prompt, node.scope, node.provider.allowed_metadata().clone()))
    }

    pub fn llm_provider_from_function(
        &self,
        function_name: &str,
        ctx: &RuntimeContext,
    ) -> Result<Arc<LLMProvider>> {
        let renderer =
            PromptRenderer::from_function(&self.get_function(function_name)?, self.ir(), ctx)?;

        self.get_llm_provider_impl(renderer.client_spec(), ctx)
    }

    pub fn get_test_params_and_constraints(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContext,
        strict: bool,
    ) -> Result<(BamlMap<String, BamlValue>, Vec<Constraint>)> {
        let params = self.get_test_params_impl(function_name, test_name, ctx, strict)?;
        let constraints = self
            .get_test_constraints_impl(function_name, test_name, ctx)
            .unwrap_or_default(); // TODO: Fix this.
                                  // .get_test_constraints_impl(function_name, test_name, ctx)?;
        Ok((params, constraints))
    }

    pub(crate) fn get_test_params_impl(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContext,
        strict: bool,
    ) -> Result<BamlMap<String, BamlValue>> {
        log::info!("get_test_params: {function_name} {test_name}");
        let maybe_test_and_params = self.get_function(function_name).and_then(|func| {
            let test = self.ir().find_test(&func, test_name)?;
            let test_case_params = test.test_case_params(&ctx.eval_ctx(strict))?;
            let inputs = func.inputs().clone();
            let span = test.span();
            Ok((test_case_params, inputs, span.cloned()))
        });
        let maybe_expr_test_and_params =
            self.get_expr_function(function_name, ctx).and_then(|func| {
                let test = self.ir().find_expr_fn_test(&func, test_name)?;
                let test_case_params = test.test_case_params(&ctx.eval_ctx(strict))?;
                let inputs = func.inputs().clone();
                let span = test.span();
                Ok((test_case_params, inputs, span.cloned()))
            });

        let maybe_params = maybe_test_and_params.or(maybe_expr_test_and_params);

        let eval_ctx = ctx.eval_ctx(strict);

        match maybe_params {
            Ok((params, function_params, span)) => {
                let mut errors = Vec::new();
                let params = params
                    .into_iter()
                    .map(|(k, v)| match v {
                        Ok(v) => (k, v),
                        Err(e) => {
                            errors.push(e);
                            (k, BamlValue::Null)
                        }
                    })
                    .collect::<BamlMap<_, _>>();

                if !errors.is_empty() {
                    return Err(anyhow::anyhow!(
                        "Unable to resolve test params: {:?}",
                        errors
                    ));
                }

                self.ir()
                    .check_function_params(
                        &function_params,
                        &params,
                        internal_baml_core::ir::ArgCoercer {
                            span_path: span.map(|s| s.file.path_buf().clone()),
                            allow_implicit_cast_to_string: true,
                        },
                    )
                    .map(|bv| bv.into_iter().map(|(k, v)| (k, v.value())).collect())
            }
            Err(e) => Err(anyhow::anyhow!("Unable to resolve test params: {:?}", e)),
        }
    }

    pub(crate) fn get_test_constraints_impl(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContext,
    ) -> Result<Vec<Constraint>> {
        if let Ok(func) = self.get_function(function_name) {
            let walker = self.ir().find_test(&func, test_name)?;
            return Ok(walker.item.1.elem.constraints.clone());
        }

        let expr_fn = self.get_expr_function(function_name, ctx)?;
        let test = self.ir().find_expr_fn_test(&expr_fn, test_name)?;
        Ok(test.item.1.elem.constraints.clone())
    }

    pub(crate) fn get_test_type_builder_impl(
        &self,
        function_name: &str,
        test_name: &str,
    ) -> Result<Option<TypeBuilder>> {
        if let Ok(func) = self.get_function(function_name) {
            let test = self.ir().find_test(&func, test_name)?;

            if test.type_builder_contents().is_empty() {
                return Ok(None);
            }

            let type_builder = TypeBuilder::new();
            type_builder.add_entries(test.type_builder_contents());
            type_builder
                .recursive_type_aliases()
                .lock()
                .unwrap()
                .extend(test.type_builder_recursive_aliases().iter().cloned());
            type_builder
                .recursive_classes()
                .lock()
                .unwrap()
                .extend(test.type_builder_recursive_classes().iter().cloned());

            return Ok(Some(type_builder));
        }

        let expr_fn = self.ir().find_expr_fn(function_name)?;
        let test = expr_fn.find_test(test_name).ok_or_else(|| {
            anyhow::anyhow!(
                "Test '{}' not found for expr function '{}'",
                test_name,
                function_name
            )
        })?;

        if test.item.1.elem.type_builder.entries.is_empty() {
            return Ok(None);
        }

        let type_builder = TypeBuilder::new();
        type_builder.add_entries(&test.item.1.elem.type_builder.entries);
        type_builder
            .recursive_type_aliases()
            .lock()
            .unwrap()
            .extend(
                test.item
                    .1
                    .elem
                    .type_builder
                    .recursive_aliases
                    .iter()
                    .cloned(),
            );
        type_builder.recursive_classes().lock().unwrap().extend(
            test.item
                .1
                .elem
                .type_builder
                .recursive_classes
                .iter()
                .cloned(),
        );

        Ok(Some(type_builder))
    }

    pub(crate) async fn render_raw_curl_impl(
        &self,
        function_name: &str,
        ctx: &RuntimeContext,
        prompt: &[internal_baml_jinja::RenderedChatMessage],
        render_settings: RenderCurlSettings,
        node_index: Option<usize>,
    ) -> Result<String> {
        let func = self.get_function(function_name)?;
        let renderer = PromptRenderer::from_function(&func, self.ir(), ctx)?;

        let client_spec = renderer.client_spec();
        let client = self.get_llm_provider_impl(client_spec, ctx)?;
        let mut selected =
            client.iter_orchestrator(&mut Default::default(), Default::default(), ctx, self)?;

        let node_index = node_index.unwrap_or(0);

        if node_index >= selected.len() {
            return Err(anyhow::anyhow!(
                "Execution Node out of bounds (raw curl): {} >= {} for client {}",
                node_index,
                selected.len(),
                client_spec,
            ));
        }

        let node = selected.swap_remove(node_index);
        node.provider
            .render_raw_curl(ctx, prompt, render_settings)
            .await
    }

    pub async fn run_test_with_expr_events<F, G>(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContextManager,
        on_event: Option<F>,
        expr_tx: Option<mpsc::UnboundedSender<Vec<internal_baml_diagnostics::SerializedSpan>>>,
        collector: Option<Arc<Collector>>,
        env_vars: HashMap<String, String>,
        tags: Option<HashMap<String, String>>,
        cancel_tripwire: Arc<TripWire>,
        on_tick: Option<G>,
    ) -> (Result<TestResponse>, FunctionCallId)
    where
        F: Fn(FunctionResult),
        G: Fn(),
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
                tags.as_ref(),
            );

        let expr_fn = self.ir().find_expr_fn(function_name);
        let is_expr_fn = expr_fn.is_ok();

        // If it's an expr function, use the simpler expr execution path
        if is_expr_fn {
            return self
                .run_expr_test(function_name, test_name, ctx, env_vars)
                .await;
        }

        let run_to_response = || async {
            // acceptable clone, just used for testing
            let rctx_no_tb =
                ctx.create_ctx(None, None, env_vars.clone(), call.new_call_id_stack.clone())?;
            let (params, constraints) =
                self.get_test_params_and_constraints(function_name, test_name, &rctx_no_tb, true)?;

            let type_builder = self
                .get_test_type_builder_impl(function_name, test_name)
                .unwrap();

            let rctx = ctx.create_ctx(
                type_builder.as_ref(),
                None,
                env_vars.clone(),
                call.new_call_id_stack.clone(),
            )?;

            let mut stream = self.stream_function_impl(
                function_name.to_string(),
                &params,
                self.tracer_wrapper.get_or_create_tracer(&env_vars),
                rctx,
                #[cfg(not(target_arch = "wasm32"))]
                self.async_runtime.clone(),
                // TODO: collectors here?
                vec![],
                None, // tags
                cancel_tripwire,
            )?;
            let (response_res, call_uuid) = stream
                .run(
                    None::<fn()>,
                    on_event,
                    ctx,
                    type_builder.as_ref(),
                    None,
                    env_vars.clone(),
                )
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
                LLMResponse::Cancelled(e) => Err(anyhow::anyhow!("Cancelled: {}", e)),
                LLMResponse::LLMFailure(e) => Err(anyhow::anyhow!({
                    let scrubbed_opts =
                        crate::redaction::scrub_baml_options(&e.request_options, &env_vars, false);
                    format!(
                        "{} {}\n\nRequest options: {}",
                        e.code,
                        e.message,
                        serde_json::to_string(&scrubbed_opts).unwrap_or_default()
                    )
                })),
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
                            complete_resp,
                            constraints,
                        )
                    }
                    _ => TestConstraintsResult::empty(),
                }
            };

            Ok(TestResponse {
                function_response: Some(res),
                expr_function_response: None,
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
                Err(e) => baml_log::error!("Error during logging: {e}"),
            }
            #[cfg(target_arch = "wasm32")]
            match self
                .tracer_wrapper
                .get_or_create_tracer(&env_vars)
                .finish_call(call, ctx, None)
                .await
            {
                Ok(id) => {}
                Err(e) => log::error!("Error during logging: {e}"),
            }
        }

        (response, call_id)
    }

    pub async fn run_test<F, G>(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContextManager,
        on_event: Option<F>,
        collector: Option<Arc<Collector>>,
        env_vars: HashMap<String, String>,
        tags: Option<HashMap<String, String>>,
        cancel_tripwire: Arc<TripWire>,
        on_tick: Option<G>,
    ) -> (Result<TestResponse>, FunctionCallId)
    where
        F: Fn(FunctionResult),
        G: Fn(),
    {
        let res = self
            .run_test_with_expr_events::<F, G>(
                function_name,
                test_name,
                ctx,
                on_event,
                None,
                collector,
                env_vars,
                tags,
                cancel_tripwire,
                on_tick,
            )
            .await;
        res
    }

    /// Run an expr function test - simpler path that doesn't involve LLM infrastructure
    pub async fn run_expr_test(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContextManager,
        env_vars: HashMap<String, String>,
    ) -> (Result<TestResponse>, FunctionCallId) {
        // Get test parameters
        let rctx = ctx.create_ctx_with_default();
        let params = match self.get_test_params(function_name, test_name, &rctx, true) {
            Ok(params) => params,
            Err(e) => return (Err(e), FunctionCallId::new()),
        };

        // Get constraints for the test
        let constraints = match self.get_test_constraints_impl(function_name, test_name, &rctx) {
            Ok(c) => c,
            Err(e) => return (Err(e), FunctionCallId::new()),
        };

        // Create a call ID for tracing
        let call_id = self
            .tracer_wrapper
            .get_or_create_tracer(&env_vars)
            .start_call(
                function_name,
                ctx,
                &params,
                true,
                false,
                None, // collectors
                None, // tags
            )
            .curr_call_id();

        // For expr functions, we need to use a runtime that can handle them
        // The AsyncInterpreterRuntime or AsyncVMRuntime would work, but since
        // we're in the basic BamlRuntime (LLM-only), we cannot call expr functions
        // directly. Instead, we need to return an error or use the interpreter directly.
        let interpreter_runtime = match CoreRuntime::try_from(self.clone()) {
            Ok(runtime) => runtime,
            Err(e) => return (Err(e), call_id),
        };

        let (result, _) = interpreter_runtime
            .call_function(
                function_name.to_string(),
                &params,
                ctx,
                None, // tb
                None, // cb
                None, // collectors
                env_vars.clone(),
                None, // tags
                TripWire::new(None),
                None::<fn(baml_compiler::watch::WatchNotification)>, // watch_handler
            )
            .await;

        // For expr functions, extract the parsed value directly
        let expr_response = match result {
            Ok(func_result) => {
                // Extract the parsed value from the FunctionResult
                func_result.parsed().as_ref().map(|r| match r {
                    Ok(val) => Ok(val.clone()),
                    Err(e) => Err(anyhow::anyhow!("{}", e)),
                })
            }
            Err(e) => Some(Err(e)),
        };

        // Evaluate constraints if any
        let constraints_result = if constraints.is_empty() {
            TestConstraintsResult::empty()
        } else {
            match &expr_response {
                Some(Ok(val)) => {
                    let value_with_constraints = val.0.map_meta(|m| m.1.clone());
                    evaluate_test_constraints(
                        &params,
                        &value_with_constraints,
                        &LLMCompleteResponse {
                            client: "expr_function".to_string(),
                            model: "expr_function".to_string(),
                            prompt: RenderedPrompt::Chat(vec![]),
                            request_options: BamlMap::new(),
                            start_time: web_time::SystemTime::now(),
                            latency: web_time::Duration::from_millis(0),
                            content: String::new(),
                            metadata: LLMCompleteResponseMetadata {
                                baml_is_complete: true,
                                finish_reason: None,
                                prompt_tokens: None,
                                output_tokens: None,
                                total_tokens: None,
                                cached_input_tokens: None,
                            },
                        },
                        constraints,
                    )
                }
                _ => TestConstraintsResult::empty(),
            }
        };

        let test_response = TestResponse {
            function_response: None,
            expr_function_response: expr_response,
            function_call: call_id.clone(),
            constraints_result,
        };

        (Ok(test_response), call_id)
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
        tags: Option<HashMap<String, String>>,
        cancel_tripwire: Arc<TripWire>,
    ) -> (Result<FunctionResult>, FunctionCallId) {
        let fut = self.call_function(
            function_name,
            params,
            ctx,
            tb,
            cb,
            collectors,
            tags,
            env_vars,
            cancel_tripwire,
        );
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
        tags: Option<HashMap<String, String>>,
        env_vars: HashMap<String, String>,
        cancel_tripwire: Arc<TripWire>,
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
            cancel_tripwire,
            tags,
        ))
        .await;
        res
    }

    /// TODO: this is a placeholder since expr fns are not LLM calls. So this is a dummy.
    pub fn dummy_llm_placeholder_for_expr_fn() -> LLMResponse {
        LLMResponse::Success(LLMCompleteResponse {
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
                cached_input_tokens: None,
            },
        })
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
        cancel_tripwire: Arc<TripWire>,
        tags: Option<HashMap<String, String>>,
    ) -> (Result<FunctionResult>, FunctionCallId) {
        // baml_log::info!("env vars: {:#?}", env_vars.clone());
        baml_log::set_from_env(&env_vars).unwrap();

        log::trace!("Calling function: {function_name}");
        log::debug!("collectors: {:#?}", &collectors);

        let call = self
            .tracer_wrapper
            .get_or_create_tracer(&env_vars)
            .start_call(
                &function_name,
                ctx,
                params,
                true,
                false,
                collectors,
                tags.as_ref(),
            );
        let curr_call_id = call.curr_call_id();

        let fake_syntax_span = Span::fake();
        let response =
            match ctx.create_ctx(tb, cb, env_vars.clone(), call.new_call_id_stack.clone()) {
                Ok(rctx) => {
                    let call_id_stack = rctx.call_id_stack.clone();
                    // TODO: is this the right naming?
                    let prepared_func = match self.prepare_function(function_name.clone(), params) {
                        Ok(prepared_func) => prepared_func,
                        Err(e) => {
                            let err_anyhow = e.into_error();
                            let trace_event = TraceEvent::new_function_end(
                                call_id_stack.clone(),
                                Err((&err_anyhow).to_baml_error()),
                            );
                            BAML_TRACER.lock().unwrap().put(Arc::new(trace_event));
                            return (Err(err_anyhow), curr_call_id);
                        }
                    };

                    // Call (CANNOT RETURN HERE until trace event is finished)
                    let result = self
                        .call_function_impl(prepared_func, rctx, cancel_tripwire)
                        .await;
                    // Trace event
                    let trace_event = TraceEvent::new_function_end(
                        call_id_stack.clone(),
                        match &result {
                            Ok(result) => match result.result_with_constraints_content() {
                                Ok(value) => {
                                    Ok(value.0.map_meta(|f| f.3.to_non_streaming_type(self.ir())))
                                }
                                Err(e) => Err((&e).to_baml_error()),
                            },
                            Err(e) => Err(e.to_baml_error()),
                        },
                    );
                    BAML_TRACER.lock().unwrap().put(Arc::new(trace_event));

                    result
                }
                Err(e) => {
                    let trace_event = TraceEvent::new_function_end(
                        call.new_call_id_stack.clone(),
                        Err((&e).to_baml_error()),
                    );
                    BAML_TRACER.lock().unwrap().put(Arc::new(trace_event));
                    Err(e)
                }
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
            Err(e) => log::error!("Error during logging: {e}"),
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
        tags: Option<HashMap<String, String>>,
        expr_tx: Option<mpsc::UnboundedSender<Vec<SerializedSpan>>>,
        cancel_tripwire: Arc<TripWire>,
    ) -> Result<FunctionResultStream> {
        baml_log::set_from_env(&env_vars).unwrap();
        self.stream_function_impl(
            function_name,
            params,
            self.tracer_wrapper.get_or_create_tracer(&env_vars),
            ctx.create_ctx(tb, cb, env_vars, ctx.call_id_stack(true)?)?,
            #[cfg(not(target_arch = "wasm32"))]
            self.async_runtime.clone(),
            collectors.unwrap_or_default(),
            tags,
            cancel_tripwire,
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
        tags: Option<HashMap<String, String>>,
        cancel_tripwire: Arc<TripWire>,
    ) -> Result<FunctionResultStream> {
        self.stream_function_with_expr_events(
            function_name,
            params,
            ctx,
            tb,
            cb,
            collectors,
            env_vars,
            tags,
            None,
            cancel_tripwire,
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
        let mut ctx = context_manager.create_ctx(tb, cb, env_vars, vec![])?;

        // Called from modular API.
        ctx.set_modular_api(true);

        let provider = self.llm_provider_from_function(&function_name, &ctx)?;

        let prompt = self
            .render_prompt(&function_name, &ctx, params, None)
            .await
            .map(|(prompt, ..)| prompt)?;

        let mut request_id = HttpRequestId::new();

        if let RenderedPrompt::Chat(chat) = &prompt {
            if let LLMProvider::Primitive(primitive) = provider.as_ref() {
                if let internal::llm_client::primitive::LLMPrimitiveProvider::Aws(aws_client) =
                    primitive.as_ref()
                {
                    return aws_client
                        .build_modular_http_request(&ctx, chat, stream, request_id)
                        .await;
                }
            }
        }

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
            std::mem::take(&mut request_id),
            request.url().to_string(),
            request.method().to_string(),
            json_headers(request.headers()),
            HTTPBody::new(
                request
                    .body()
                    .and_then(reqwest::Body::as_bytes)
                    .unwrap_or_default()
                    .into(),
            ),
            ClientDetails {
                name: "unknown".to_string(),
                provider: "unknown".to_string(),
                options: IndexMap::new(),
            },
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

        let renderer =
            PromptRenderer::from_function(&self.get_function(&function_name)?, self.ir(), &ctx)?;

        renderer.parse(self.ir(), &ctx, &llm_response, allow_partials)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn generate_client(
        &self,
        client_type: &GeneratorOutputType,
        args: &GeneratorArgs,
        generator_type: GeneratorType,
    ) -> Result<GenerateOutput> {
        if !args.no_version_check {
            if let Some(error) = version_check::check_version(
                &args.version,
                env!("CARGO_PKG_VERSION"),
                generator_type,
                VersionCheckMode::Strict,
                *client_type,
                false,
            ) {
                return Err(anyhow::anyhow!(error.msg()));
            }
        }

        let files = generators_lib::generate_sdk(self.ir.clone(), args)?;
        Ok(GenerateOutput {
            client_type: *client_type,
            output_dir_shorthand: args.output_dir().to_path_buf(),
            output_dir_full: args.output_dir().to_path_buf(),
            files,
        })
    }
}

// Interfaces for generators
impl BamlRuntime {
    pub fn function_names(&self) -> impl Iterator<Item = &str> {
        self.ir().function_names()
    }

    /// Determine the file containing the generators.
    pub fn generator_path(&self) -> Option<PathBuf> {
        let path_counts: HashMap<&PathBuf, u32> = self
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
        self.ir()
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
        self.ir()
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
        generator_type: GeneratorType,
    ) -> Result<Vec<GenerateOutput>> {
        let client_types: Vec<(&CodegenGenerator, GeneratorArgs)> = self
            .codegen_generators()
            .map(|generator| {
                Ok((
                    generator,
                    GeneratorArgs::new(
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
                if !args.no_version_check {
                    if let Some(error) = version_check::check_version(
                        &args.version,
                        env!("CARGO_PKG_VERSION"),
                        generator_type,
                        VersionCheckMode::Strict,
                        generator.output_type,
                        false,
                    ) {
                        return Err(anyhow::anyhow!(error.msg()));
                    }
                }

                let files = generators_lib::generate_sdk(self.ir.clone(), args)?;
                Ok(GenerateOutput {
                    client_type: generator.output_type,
                    output_dir_shorthand: generator.output_dir(),
                    output_dir_full: generator.output_dir(),
                    files,
                })
            })
            .collect()
    }
}

impl<'a> runtime_interface::InternalClientLookup<'a> for BamlRuntime {
    fn get_llm_provider(
        &'a self,
        client_spec: &ClientSpec,
        ctx: &RuntimeContext,
    ) -> Result<Arc<LLMProvider>> {
        self.get_llm_provider_impl(client_spec, ctx)
    }

    fn get_retry_policy(&self, policy_name: &str, ctx: &RuntimeContext) -> Result<CallablePolicy> {
        self.get_retry_policy_impl(policy_name, ctx)
    }
}

impl BamlRuntime {
    pub(crate) fn ir(&self) -> &IntermediateRepr {
        use std::ops::Deref;
        self.ir.deref()
    }

    pub(crate) fn get_llm_provider_impl(
        &self,
        client_spec: &ClientSpec,
        ctx: &RuntimeContext,
    ) -> Result<Arc<LLMProvider>> {
        match client_spec {
            ClientSpec::Shorthand(provider, model) => {
                let client_property = ClientProperty::from_shorthand(provider, model);
                let llm_primitive_provider =
                    LLMPrimitiveProvider::try_from((&client_property, ctx))
                        .context(format!("Failed to parse client: {provider}/{model}"))?;

                Ok(Arc::new(LLMProvider::Primitive(Arc::new(
                    llm_primitive_provider,
                ))))
            }
            ClientSpec::Named(client_name) => {
                if let Some(client) = ctx
                    .client_overrides
                    .as_ref()
                    .and_then(|(_, c)| c.get(client_name))
                {
                    return Ok(client.clone());
                }

                #[cfg(target_arch = "wasm32")]
                let mut clients = self.clients.lock().unwrap();
                #[cfg(not(target_arch = "wasm32"))]
                let clients = &self.clients;

                if clients.contains_key(client_name) {
                    #[allow(clippy::map_clone)]
                    let client = clients.get(client_name).map(|c| c.clone()).unwrap();
                    if !client.has_env_vars_changed(ctx.env_vars()) {
                        return Ok(client.provider.clone());
                    } else {
                        clients.remove(client_name);
                    }
                }

                let walker = self
                    .ir()
                    .find_client(client_name)
                    .context(format!("Could not find client with name: {client_name}"))?;
                let new_client = LLMProvider::try_from((&walker, ctx)).map(Arc::new)?;

                let mut required_env_vars = HashMap::new();
                let fail_on_missing_required_env_vars = !ctx.is_modular_api()
                    && !matches!(
                        walker.item.elem.provider,
                        internal_llm_client::ClientProvider::AwsBedrock
                            | internal_llm_client::ClientProvider::Vertex
                    );

                for key in walker.required_env_vars() {
                    if let Some(value) = ctx.env_vars().get(&key) {
                        if fail_on_missing_required_env_vars && value.trim().is_empty() {
                            baml_log::warn!(
                                "Required environment variable '{key}' for client '{client_name}' is set but is empty: {key}='{value}'"
                            );
                        }
                        required_env_vars.insert(key, value.to_owned());
                    } else if fail_on_missing_required_env_vars {
                        anyhow::bail!(
                            "LLM client '{client_name}' requires environment variable '{key}' to be set but it is not"
                        );
                    }
                }

                clients.insert(
                    client_name.into(),
                    runtime::CachedClient::new(new_client.clone(), required_env_vars),
                );

                Ok(new_client)
            }
        }
    }

    pub(crate) fn get_retry_policy_impl(
        &self,
        policy_name: &str,
        _ctx: &RuntimeContext,
    ) -> Result<CallablePolicy> {
        #[cfg(target_arch = "wasm32")]
        let mut retry_policies = self.retry_policies.lock().unwrap();
        #[cfg(not(target_arch = "wasm32"))]
        let retry_policies = &self.retry_policies;

        let inserter = || {
            self.ir()
                .walk_retry_policies()
                .find(|walker| walker.name() == policy_name)
                .ok_or_else(|| {
                    anyhow::anyhow!("Could not find retry policy with name: {}", policy_name)
                })
                .map(CallablePolicy::from)
        };

        #[cfg(target_arch = "wasm32")]
        {
            if let Some(policy_ref) = retry_policies.get(policy_name) {
                return Ok(policy_ref.clone());
            }
            let new_policy = inserter()?;
            retry_policies.insert(policy_name.into(), new_policy.clone());
            Ok(new_policy)
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let policy_ref = retry_policies
                .entry(policy_name.into())
                .or_try_insert_with(inserter)?;
            Ok(policy_ref.value().clone())
        }
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
            .start_call(function_name, ctx, params, false, false, None, None)
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
                log::error!("Failed to flush: {}", e);
                baml_log::debug!("Failed to flush: {}", e);
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            wasm_bindgen_futures::spawn_local(async move {
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

impl InternalRuntimeInterface for BamlRuntime {
    fn diagnostics(&self) -> &DiagnosticsError {
        &self.diagnostics
    }

    fn orchestration_graph(
        &self,
        client_spec: &ClientSpec,
        ctx: &RuntimeContext,
    ) -> Result<Vec<internal::llm_client::orchestrator::OrchestratorNode>> {
        let client = self.get_llm_provider_impl(client_spec, ctx)?;
        client.iter_orchestrator(&mut Default::default(), Default::default(), ctx, self)
    }

    fn function_graph(&self, _function_name: &str, _ctx: &RuntimeContext) -> Result<String> {
        let ast = self.db.ast();
        let graph =
            internal_baml_core::ast::BamlVisDiagramGenerator::generate_headers_flowchart(ast);
        Ok(graph)
    }

    fn features(&self) -> internal::ir_features::IrFeatures {
        internal::ir_features::WithInternal::features(self)
    }

    async fn render_prompt(
        &self,
        function_name: &str,
        ctx: &RuntimeContext,
        params: &BamlMap<String, BamlValue>,
        node_index: Option<usize>,
    ) -> Result<(RenderedPrompt, OrchestrationScope, AllowedRoleMetadata)> {
        self.render_prompt_impl(function_name, ctx, params, node_index)
            .await
    }

    async fn render_raw_curl(
        &self,
        function_name: &str,
        ctx: &RuntimeContext,
        prompt: &[internal_baml_jinja::RenderedChatMessage],
        render_settings: RenderCurlSettings,
        node_index: Option<usize>,
    ) -> Result<String> {
        self.render_raw_curl_impl(function_name, ctx, prompt, render_settings, node_index)
            .await
    }

    fn get_function<'ir>(
        &'ir self,
        function_name: &str,
    ) -> Result<internal_baml_core::ir::FunctionWalker<'ir>> {
        let walker = self.ir().find_function(function_name)?;
        Ok(walker)
    }

    fn get_expr_function<'ir>(
        &'ir self,
        function_name: &str,
        _ctx: &RuntimeContext,
    ) -> Result<internal_baml_core::ir::ExprFunctionWalker<'ir>> {
        let walker = self.ir().find_expr_fn(function_name)?;
        Ok(walker)
    }

    fn ir(&self) -> &IntermediateRepr {
        use std::ops::Deref;
        self.ir.deref()
    }

    fn get_test_params(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContext,
        strict: bool,
    ) -> Result<BamlMap<String, BamlValue>> {
        self.get_test_params_impl(function_name, test_name, ctx, strict)
    }

    fn get_test_constraints(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContext,
    ) -> Result<Vec<Constraint>> {
        self.get_test_constraints_impl(function_name, test_name, ctx)
    }

    fn get_test_type_builder(
        &self,
        function_name: &str,
        test_name: &str,
    ) -> Result<Option<TypeBuilder>> {
        self.get_test_type_builder_impl(function_name, test_name)
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
