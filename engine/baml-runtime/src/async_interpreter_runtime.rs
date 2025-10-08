//! Implementation of the async runtime using THIR interpreter.
//!
//! Unlike the VM-based runtime which requires an embedder loop for future scheduling,
//! this implementation directly calls the THIR interpreter with an LLM handler callback
//! that can execute LLM functions synchronously or asynchronously as needed.

use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use baml_compiler::{
    self, hir,
    thir::{interpret::interpret_thir, typecheck::typecheck},
};
use baml_ids::FunctionCallId;
use baml_types::{BamlMap, BamlValue, BamlValueWithMeta, Completion};
use internal_baml_core::ir::IRHelper;
use internal_baml_diagnostics::Diagnostics;
use jsonish::{ResponseBamlValue, ResponseValueMeta};

#[cfg(not(target_arch = "wasm32"))]
use crate::on_log_event::LogEventCallbackSync;
use crate::{
    client_registry::ClientRegistry,
    internal::llm_client::{orchestrator::OrchestrationScope, LLMResponse},
    runtime_interface::ExperimentalTracingInterface,
    tracing::TracingCall,
    tracingv2::storage::storage::Collector,
    type_builder::TypeBuilder,
    BamlRuntime as LlmRuntime, BamlSrcReader, BamlTracerWrapper, FunctionResult,
    FunctionResultStream, InnerTraceStats, InternalRuntimeInterface, RuntimeContextManager,
    TripWire,
};

/// Async THIR interpreter runtime.
///
/// This runtime uses the THIR interpreter directly with LLM function callbacks,
/// avoiding the complexity of the VM embedder loop. When the interpreter encounters
/// an LLM function call, it invokes the provided callback which handles the
/// actual LLM execution through the legacy runtime.
#[derive(Clone)]
pub struct BamlAsyncInterpreterRuntime {
    /// Async runtime to schedule futures.
    #[cfg(not(target_arch = "wasm32"))]
    async_runtime: Arc<tokio::runtime::Runtime>,

    /// Legacy Baml runtime for LLM function execution.
    llm_runtime: Arc<LlmRuntime>,

    /// Compiled THIR program.
    thir_program: baml_compiler::thir::THir<baml_compiler::thir::ExprMetadata>,
}

impl TryFrom<LlmRuntime> for BamlAsyncInterpreterRuntime {
    type Error = anyhow::Error;

    fn try_from(llm_runtime: LlmRuntime) -> Result<Self, Self::Error> {
        #[cfg(not(target_arch = "wasm32"))]
        let async_runtime = Arc::clone(&llm_runtime.async_runtime);

        // Stage 1: AST -> HIR
        let hir_program = hir::Hir::from_ast(&llm_runtime.db.ast);

        // Stage 2: HIR -> THIR (typecheck)
        let mut diagnostics = Diagnostics::new("dummy".into());
        let thir_program = typecheck(&hir_program, &mut diagnostics);

        if diagnostics.has_errors() {
            let errors = diagnostics.to_pretty_string();
            return Err(anyhow::anyhow!("Typecheck errors: {errors}"));
        }

        Ok(Self {
            llm_runtime: Arc::new(llm_runtime),
            thir_program,

            #[cfg(not(target_arch = "wasm32"))]
            async_runtime,
        })
    }
}

impl BamlAsyncInterpreterRuntime {
    pub fn internal(&self) -> &LlmRuntime {
        &self.llm_runtime
    }

    pub fn disassemble(&self, function_name: &str) {
        // The interpreter doesn't use bytecode, so we just print the THIR representation
        if let Some(expr_fn) = self
            .thir_program
            .expr_functions
            .iter()
            .find(|f| f.name == function_name)
        {
            println!("THIR for expression function '{}':", function_name);
            println!("{}", expr_fn.body.dump_str());
        } else {
            println!(
                "Function '{}' not found or is not an expression function",
                function_name
            );
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_directory<T: AsRef<str>>(
        path: &std::path::Path,
        env_vars: HashMap<T, T>,
    ) -> anyhow::Result<Self> {
        Self::try_from(LlmRuntime::from_directory(
            path,
            env_vars,
            internal_baml_core::FeatureFlags::new(),
        )?)
    }

    pub fn from_file_content<T: AsRef<str> + std::fmt::Debug, U: AsRef<str>>(
        root_path: &str,
        files: &HashMap<T, T>,
        env_vars: HashMap<U, U>,
    ) -> anyhow::Result<Self> {
        Self::from_file_content_with_features(
            root_path,
            files,
            env_vars,
            internal_baml_core::FeatureFlags::new(),
        )
    }

    pub fn from_file_content_with_features<T: AsRef<str> + std::fmt::Debug, U: AsRef<str>>(
        root_path: &str,
        files: &HashMap<T, T>,
        env_vars: HashMap<U, U>,
        feature_flags: internal_baml_core::FeatureFlags,
    ) -> anyhow::Result<Self> {
        Self::try_from(LlmRuntime::from_file_content(
            root_path,
            files,
            env_vars,
            feature_flags,
        )?)
    }

    pub fn create_ctx_manager(
        &self,
        language: BamlValue,
        baml_src_reader: BamlSrcReader,
    ) -> RuntimeContextManager {
        self.llm_runtime
            .create_ctx_manager(language, baml_src_reader)
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
        tags: Option<&HashMap<String, String>>,
        cancel_tripwire: Arc<TripWire>,
        emit_handler: Option<impl FnMut(baml_compiler::emit::EmitEvent) + Send + 'static>,
    ) -> (anyhow::Result<FunctionResult>, FunctionCallId) {
        // Check if this is an expression function
        let expr_fn = self
            .thir_program
            .expr_functions
            .iter()
            .find(|f| f.name == function_name);

        if expr_fn.is_none() {
            // If it's not an expression function, delegate to the LLM runtime
            return self
                .llm_runtime
                .call_function(
                    function_name,
                    params,
                    ctx,
                    tb,
                    cb,
                    collectors,
                    None, // TODO: tags?
                    env_vars,
                    cancel_tripwire,
                )
                .await;
        }

        let expr_fn = expr_fn.unwrap();
        let current_call_id = self
            .llm_runtime
            .tracer_wrapper
            .get_or_create_tracer(&env_vars)
            .start_call(
                &function_name,
                ctx,
                params,
                true,
                false,
                collectors.clone(),
                tags,
            )
            .curr_call_id();

        let output_type = expr_fn.return_type.clone();

        // Convert input parameters to BamlValueWithMeta for interpreter
        let extra_bindings = params
            .iter()
            .map(|(name, value)| {
                (
                    name.clone(),
                    baml_value_to_baml_value_with_meta(value.clone()),
                )
            })
            .collect::<BamlMap<_, _>>();

        // Create LLM function handler
        let llm_runtime_clone = Arc::clone(&self.llm_runtime);
        let ctx_clone = ctx.clone();
        let tb_clone = tb.cloned();
        let cb_clone = cb.cloned();
        let env_vars_clone = env_vars.clone();
        let cancel_tripwire_clone = cancel_tripwire.clone();

        let llm_handler = move |fn_name: String, args: Vec<BamlValue>| {
            let llm_runtime = Arc::clone(&llm_runtime_clone);
            let ctx = ctx_clone.clone();
            let tb = tb_clone.clone();
            let cb = cb_clone.clone();
            let env_vars = env_vars_clone.clone();
            let cancel_tripwire = cancel_tripwire_clone.clone();

            async move {
                // Find the LLM function to get parameter names
                let llm_fn = llm_runtime
                    .ir()
                    .find_function(&fn_name)
                    .map_err(|e| anyhow::anyhow!("LLM function not found: {}: {}", fn_name, e))?;

                // Convert args to parameter map
                let llm_params = args
                    .into_iter()
                    .zip(llm_fn.inputs().iter().map(|(name, _)| name.clone()))
                    .map(|(arg, param_name)| (param_name, arg))
                    .collect::<BamlMap<_, _>>();

                // Call the LLM function
                let (result, _call_id) = llm_runtime
                    .call_function(
                        fn_name,
                        &llm_params,
                        &ctx,
                        tb.as_ref(),
                        cb.as_ref(),
                        None, // TODO: collectors not supported yet
                        tags.cloned(),
                        env_vars,
                        cancel_tripwire,
                    )
                    .await;

                // Convert result to BamlValueWithMeta
                match result {
                    Ok(function_result) => {
                        let baml_value = function_result
                            .parsed()
                            .as_ref()
                            .unwrap()
                            .as_ref()
                            .unwrap()
                            .clone()
                            .0
                            .value();

                        Ok(baml_value_to_baml_value_with_meta(baml_value))
                    }
                    Err(e) => Err(e),
                }
            }
        };

        // Choose execution strategy based on function body structure
        let function_expr =
            if expr_fn.body.statements.len() == 1 && expr_fn.body.trailing_expr.is_none() {
                // Special case: Single expression statement (e.g., if-else as the only thing in function)
                // This happens when BAML functions are just a single expression
                match &expr_fn.body.statements[0] {
                    baml_compiler::thir::Statement::Expression { expr, .. } => expr.clone(),
                    _ => {
                        // Single non-expression statement - execute as block
                        baml_compiler::thir::Expr::Block(
                            Box::new(expr_fn.body.clone()),
                            (
                                internal_baml_diagnostics::Span::fake(),
                                Some(output_type.clone()),
                            ),
                        )
                    }
                }
            } else if !expr_fn.body.statements.is_empty() {
                // Function has multiple statements or statements + trailing expr
                // Execute the entire function body as a block to ensure all statements run
                baml_compiler::thir::Expr::Block(
                    Box::new(expr_fn.body.clone()),
                    (
                        internal_baml_diagnostics::Span::fake(),
                        Some(output_type.clone()),
                    ),
                )
            } else if let Some(trailing_expr) = &expr_fn.body.trailing_expr {
                // Function has no statements, only a trailing expression
                // Execute the trailing expression directly
                trailing_expr.clone()
            } else {
                // Function has no statements and no trailing expression - return null
                baml_compiler::thir::Expr::Value(BamlValueWithMeta::Null((
                    internal_baml_diagnostics::Span::fake(),
                    Some(output_type.clone()),
                )))
            };

        // Create the emit event handler - either use the provided one or create a no-op
        let mut emit_event_handler: Box<dyn FnMut(baml_compiler::emit::EmitEvent) + Send> =
            if let Some(handler) = emit_handler {
                Box::new(handler)
            } else {
                Box::new(|_event| {})
            };

        // Execute the interpreter
        let result = interpret_thir(
            function_name.clone(),
            self.thir_program.clone(),
            function_expr,
            llm_handler,
            emit_event_handler,
            extra_bindings,
            env_vars,
        )
        .await;

        let baml_value = match result {
            Ok(value_with_meta) => baml_value_with_meta_to_baml_value(value_with_meta),
            Err(e) => {
                let error_result = Err(e);
                return (error_result, current_call_id);
            }
        };

        let response_baml_value = ResponseBamlValue(BamlValueWithMeta::with_const_meta(
            &baml_value,
            ResponseValueMeta(vec![], vec![], Completion::default(), output_type),
        ));

        let final_result = Ok(FunctionResult::new(
            OrchestrationScope { scope: vec![] },
            LlmRuntime::dummy_llm_placeholder_for_expr_fn(),
            Some(Ok(response_baml_value)),
        ));

        (final_result, current_call_id)
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
        cancel_tripwire: Arc<TripWire>,
        tags: Option<&HashMap<String, String>>,
        emit_handler: Option<impl FnMut(baml_compiler::emit::EmitEvent) + Send + 'static>,
    ) -> (anyhow::Result<FunctionResult>, FunctionCallId) {
        self.async_runtime.block_on(self.call_function(
            function_name,
            params,
            ctx,
            tb,
            cb,
            collectors,
            env_vars,
            tags,
            cancel_tripwire,
            emit_handler,
        ))
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
        cancel_tripwire: Arc<TripWire>,
        tags: Option<&HashMap<String, String>>,
    ) -> anyhow::Result<FunctionResultStream> {
        self.llm_runtime.stream_function(
            function_name,
            params,
            ctx,
            tb,
            cb,
            collectors,
            env_vars,
            tags.cloned(),
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
    ) -> anyhow::Result<baml_types::tracing::events::HTTPRequest> {
        self.llm_runtime
            .build_request(
                function_name,
                params,
                context_manager,
                tb,
                cb,
                env_vars,
                stream,
            )
            .await
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
    ) -> anyhow::Result<baml_types::tracing::events::HTTPRequest> {
        self.llm_runtime.build_request_sync(
            function_name,
            params,
            context_manager,
            tb,
            cb,
            stream,
            env_vars,
        )
    }

    pub fn parse_llm_response(
        &self,
        function_name: String,
        llm_response: String,
        allow_partials: bool,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        env_vars: HashMap<String, String>,
    ) -> anyhow::Result<ResponseBamlValue> {
        self.llm_runtime.parse_llm_response(
            function_name,
            llm_response,
            allow_partials,
            ctx,
            tb,
            cb,
            env_vars,
        )
    }

    // WASM-specific method to create context manager with WASM-specific tags
    pub fn create_ctx_manager_for_wasm(
        &self,
        baml_src_reader: crate::BamlSrcReader,
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

    // Code generation methods
    pub fn run_codegen(
        &self,
        input_files: &indexmap::IndexMap<std::path::PathBuf, String>,
        no_version_check: bool,
        generator_type: generators_lib::version_check::GeneratorType,
    ) -> anyhow::Result<Vec<generators_lib::GenerateOutput>> {
        self.llm_runtime
            .run_codegen(input_files, no_version_check, generator_type)
    }

    pub fn codegen_generators(
        &self,
    ) -> impl Iterator<Item = &internal_baml_core::configuration::CodegenGenerator> {
        self.llm_runtime.codegen_generators()
    }

    // Test execution methods
    pub async fn run_test<F, G>(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContextManager,
        on_event: Option<F>,
        collector: Option<Arc<crate::tracingv2::storage::storage::Collector>>,
        env_vars: HashMap<String, String>,
        tags: Option<HashMap<String, String>>,
        cancel_tripwire: Arc<crate::TripWire>,
        on_tick: Option<G>,
    ) -> (Result<crate::TestResponse>, baml_ids::FunctionCallId)
    where
        F: Fn(crate::FunctionResult),
        G: Fn(),
    {
        self.llm_runtime
            .run_test(
                function_name,
                test_name,
                ctx,
                on_event,
                collector,
                env_vars,
                tags,
                cancel_tripwire,
                on_tick,
            )
            .await
    }

    pub async fn run_test_with_expr_events<F, G>(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &RuntimeContextManager,
        on_event: Option<F>,
        expr_tx: Option<
            futures::channel::mpsc::UnboundedSender<Vec<internal_baml_diagnostics::SerializedSpan>>,
        >,
        collector: Option<Arc<crate::tracingv2::storage::storage::Collector>>,
        env_vars: HashMap<String, String>,
        tags: Option<HashMap<String, String>>,
        cancel_tripwire: Arc<crate::TripWire>,
        on_tick: Option<G>,
    ) -> (Result<crate::TestResponse>, baml_ids::FunctionCallId)
    where
        F: Fn(crate::FunctionResult),
        G: Fn(),
    {
        self.llm_runtime
            .run_test_with_expr_events(
                function_name,
                test_name,
                ctx,
                on_event,
                expr_tx,
                collector,
                env_vars,
                tags,
                cancel_tripwire,
                on_tick,
            )
            .await
    }

    // Test parameter methods
    pub fn get_test_params(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &crate::runtime_context::RuntimeContext,
        strict: bool,
    ) -> Result<BamlMap<String, BamlValue>> {
        self.llm_runtime
            .get_test_params(function_name, test_name, ctx, strict)
    }

    pub fn get_test_type_builder(
        &self,
        function_name: &str,
        test_name: &str,
    ) -> Result<Option<TypeBuilder>> {
        self.llm_runtime
            .get_test_type_builder(function_name, test_name)
    }

    pub fn tracer_wrapper(&self) -> &Arc<BamlTracerWrapper> {
        &self.llm_runtime.tracer_wrapper
    }
}

impl ExperimentalTracingInterface for BamlAsyncInterpreterRuntime {
    fn start_call(
        &self,
        function_name: &str,
        params: &BamlMap<String, BamlValue>,
        ctx: &RuntimeContextManager,
        env_vars: &HashMap<String, String>,
    ) -> TracingCall {
        self.llm_runtime
            .start_call(function_name, params, ctx, env_vars)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn finish_function_call(
        &self,
        call: TracingCall,
        result: &anyhow::Result<FunctionResult>,
        ctx: &RuntimeContextManager,
        env_vars: &HashMap<String, String>,
    ) -> anyhow::Result<uuid::Uuid> {
        self.llm_runtime
            .finish_function_call(call, result, ctx, env_vars)
    }

    #[cfg(target_arch = "wasm32")]
    async fn finish_function_call(
        &self,
        call: TracingCall,
        result: &anyhow::Result<FunctionResult>,
        ctx: &RuntimeContextManager,
        env_vars: &HashMap<String, String>,
    ) -> anyhow::Result<uuid::Uuid> {
        self.llm_runtime
            .finish_function_call(call, result, ctx, env_vars)
            .await
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn finish_call(
        &self,
        call: TracingCall,
        result: Option<BamlValue>,
        ctx: &RuntimeContextManager,
        env_vars: &HashMap<String, String>,
    ) -> anyhow::Result<uuid::Uuid> {
        self.llm_runtime.finish_call(call, result, ctx, env_vars)
    }

    #[cfg(target_arch = "wasm32")]
    async fn finish_call(
        &self,
        call: TracingCall,
        result: Option<BamlValue>,
        ctx: &RuntimeContextManager,
        env_vars: &HashMap<String, String>,
    ) -> anyhow::Result<uuid::Uuid> {
        self.llm_runtime
            .finish_call(call, result, ctx, env_vars)
            .await
    }

    fn flush(&self) -> anyhow::Result<()> {
        self.llm_runtime.flush()
    }

    fn drain_stats(&self) -> InnerTraceStats {
        self.llm_runtime.drain_stats()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn set_log_event_callback(
        &self,
        log_event_callback: Option<LogEventCallbackSync>,
    ) -> anyhow::Result<()> {
        self.llm_runtime.set_log_event_callback(log_event_callback)
    }
}

/// Convert BamlValue to BamlValueWithMeta by adding fake metadata.
fn baml_value_to_baml_value_with_meta(
    value: BamlValue,
) -> BamlValueWithMeta<baml_compiler::thir::ExprMetadata> {
    let fake_meta = (internal_baml_diagnostics::Span::fake(), None);

    match value {
        BamlValue::String(s) => BamlValueWithMeta::String(s, fake_meta),
        BamlValue::Int(i) => BamlValueWithMeta::Int(i, fake_meta),
        BamlValue::Float(f) => BamlValueWithMeta::Float(f, fake_meta),
        BamlValue::Bool(b) => BamlValueWithMeta::Bool(b, fake_meta),
        BamlValue::List(l) => {
            let converted_list = l
                .into_iter()
                .map(baml_value_to_baml_value_with_meta)
                .collect();
            BamlValueWithMeta::List(converted_list, fake_meta)
        }
        BamlValue::Map(m) => {
            let converted_map = m
                .into_iter()
                .map(|(k, v)| (k, baml_value_to_baml_value_with_meta(v)))
                .collect();
            BamlValueWithMeta::Map(converted_map, fake_meta)
        }
        BamlValue::Media(m) => BamlValueWithMeta::Media(m, fake_meta),
        BamlValue::Enum(name, val) => BamlValueWithMeta::Enum(name, val, fake_meta),
        BamlValue::Class(name, fields) => {
            let converted_fields = fields
                .into_iter()
                .map(|(k, v)| (k, baml_value_to_baml_value_with_meta(v)))
                .collect();
            BamlValueWithMeta::Class(name, converted_fields, fake_meta)
        }
        BamlValue::Null => BamlValueWithMeta::Null(fake_meta),
    }
}

/// Convert BamlValueWithMeta to BamlValue by stripping metadata.
fn baml_value_with_meta_to_baml_value(
    value: BamlValueWithMeta<baml_compiler::thir::ExprMetadata>,
) -> BamlValue {
    match value {
        BamlValueWithMeta::String(s, _) => BamlValue::String(s),
        BamlValueWithMeta::Int(i, _) => BamlValue::Int(i),
        BamlValueWithMeta::Float(f, _) => BamlValue::Float(f),
        BamlValueWithMeta::Bool(b, _) => BamlValue::Bool(b),
        BamlValueWithMeta::List(l, _) => {
            let converted_list = l
                .into_iter()
                .map(baml_value_with_meta_to_baml_value)
                .collect();
            BamlValue::List(converted_list)
        }
        BamlValueWithMeta::Map(m, _) => {
            let converted_map = m
                .into_iter()
                .map(|(k, v)| (k, baml_value_with_meta_to_baml_value(v)))
                .collect();
            BamlValue::Map(converted_map)
        }
        BamlValueWithMeta::Media(m, _) => BamlValue::Media(m),
        BamlValueWithMeta::Enum(name, val, _) => BamlValue::Enum(name, val),
        BamlValueWithMeta::Class(name, fields, _) => {
            let converted_fields = fields
                .into_iter()
                .map(|(k, v)| (k, baml_value_with_meta_to_baml_value(v)))
                .collect();
            BamlValue::Class(name, converted_fields)
        }
        BamlValueWithMeta::Null(_) => BamlValue::Null,
    }
}

impl crate::runtime_interface::InternalRuntimeInterface for BamlAsyncInterpreterRuntime {
    fn features(&self) -> crate::internal::ir_features::IrFeatures {
        self.llm_runtime.features()
    }

    fn diagnostics(&self) -> &internal_baml_core::internal_baml_diagnostics::Diagnostics {
        self.llm_runtime.diagnostics()
    }

    fn orchestration_graph(
        &self,
        client_name: &internal_llm_client::ClientSpec,
        ctx: &crate::runtime_context::RuntimeContext,
    ) -> Result<Vec<crate::internal::llm_client::orchestrator::OrchestratorNode>> {
        self.llm_runtime.orchestration_graph(client_name, ctx)
    }

    fn function_graph(
        &self,
        function_name: &str,
        ctx: &crate::runtime_context::RuntimeContext,
    ) -> Result<String> {
        self.llm_runtime.function_graph(function_name, ctx)
    }

    fn get_function<'ir>(
        &'ir self,
        function_name: &str,
    ) -> Result<internal_baml_core::ir::FunctionWalker<'ir>> {
        self.llm_runtime.get_function(function_name)
    }

    fn get_expr_function<'ir>(
        &'ir self,
        function_name: &str,
        ctx: &crate::runtime_context::RuntimeContext,
    ) -> Result<internal_baml_core::ir::ExprFunctionWalker<'ir>> {
        self.llm_runtime.get_expr_function(function_name, ctx)
    }

    async fn render_prompt(
        &self,
        function_name: &str,
        ctx: &crate::runtime_context::RuntimeContext,
        params: &BamlMap<String, BamlValue>,
        node_index: Option<usize>,
    ) -> Result<(
        internal_baml_jinja::RenderedPrompt,
        crate::internal::llm_client::orchestrator::OrchestrationScope,
        internal_llm_client::AllowedRoleMetadata,
    )> {
        self.llm_runtime
            .render_prompt(function_name, ctx, params, node_index)
            .await
    }

    async fn render_raw_curl(
        &self,
        function_name: &str,
        ctx: &crate::runtime_context::RuntimeContext,
        prompt: &[internal_baml_jinja::RenderedChatMessage],
        render_settings: crate::RenderCurlSettings,
        node_index: Option<usize>,
    ) -> Result<String> {
        self.llm_runtime
            .render_raw_curl(function_name, ctx, prompt, render_settings, node_index)
            .await
    }

    fn ir(&self) -> &internal_baml_core::ir::repr::IntermediateRepr {
        self.llm_runtime.ir()
    }

    fn get_test_params(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &crate::runtime_context::RuntimeContext,
        strict: bool,
    ) -> Result<BamlMap<String, BamlValue>> {
        self.llm_runtime
            .get_test_params(function_name, test_name, ctx, strict)
    }

    fn get_test_constraints(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &crate::runtime_context::RuntimeContext,
    ) -> Result<Vec<baml_types::Constraint>> {
        self.llm_runtime
            .get_test_constraints(function_name, test_name, ctx)
    }

    fn get_test_type_builder(
        &self,
        function_name: &str,
        test_name: &str,
    ) -> Result<Option<TypeBuilder>> {
        self.llm_runtime
            .get_test_type_builder(function_name, test_name)
    }
}
