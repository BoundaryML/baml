//! Implementation of the new asynchronous runtime.
//!
//! The VM is completely standalone and doesn't care about futures, it just
//! delegates future scheduling to an "embedder". This is the embedder. The
//! async runtime acts both as a future scheduler and a VM driver.
//!
//! This architecture is inspired by Deno, which contains a Rust Tokio runtime
//! that wraps the V8 machine and schedules JS promises.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use baml_compiler::{self};
use baml_ids::FunctionCallId;
use baml_types::{tracing::events::HTTPRequest, BamlMap, BamlValue, BamlValueWithMeta, Completion};
use baml_vm::{BamlVmProgram, EvalStack, FunctionKind, ObjectIndex, Vm, VmExecState};
use internal_baml_core::ir::IRHelper;
use jsonish::{ResponseBamlValue, ResponseValueMeta};

#[cfg(not(target_arch = "wasm32"))]
use crate::on_log_event::LogEventCallbackSync;
use crate::{
    client_registry::ClientRegistry,
    internal::llm_client::{orchestrator::OrchestrationScope, LLMResponse},
    runtime::InternalBamlRuntime,
    runtime_interface::ExperimentalTracingInterface,
    tracing::TracingCall,
    tracingv2::storage::storage::Collector,
    type_builder::TypeBuilder,
    BamlRuntime as LlmRuntime, BamlSrcReader, FunctionResult, FunctionResultStream,
    InnerTraceStats, InternalRuntimeInterface, RuntimeContextManager,
};

/// Async VM runtime.
///
/// This is an async wrapper over our synchronous, single-threaded VM. The VM
/// yields control flow to this struct when futures have to be created and when
/// it's blocked awaiting a future. From there, the async runtime schedules
/// Tokio futures or awaits them, respectively. After that control flow goes
/// back to the VM and bytecode execution continues.
pub struct BamlAsyncVmRuntime {
    /// Async runtime to schedule futures.
    #[cfg(not(target_arch = "wasm32"))]
    async_runtime: Arc<tokio::runtime::Runtime>,

    /// Old Baml runtime.
    ///
    /// This now acts as some sort of "LLM function executor" or just LLM
    /// runtime for simplicity. Here it's only used to run LLM functions.
    llm_runtime: Arc<LlmRuntime>,

    // Compiler generated objects.
    program: BamlVmProgram,
}

impl TryFrom<LlmRuntime> for BamlAsyncVmRuntime {
    type Error = anyhow::Error;

    fn try_from(llm_runtime: LlmRuntime) -> Result<Self, Self::Error> {
        #[cfg(not(target_arch = "wasm32"))]
        let async_runtime = Arc::clone(&llm_runtime.async_runtime);

        let program = baml_compiler::compile(&llm_runtime.inner.db)?;

        Ok(Self {
            llm_runtime: Arc::new(llm_runtime),
            program,

            #[cfg(not(target_arch = "wasm32"))]
            async_runtime,
        })
    }
}

impl BamlAsyncVmRuntime {
    pub fn internal(&self) -> &Arc<InternalBamlRuntime> {
        self.llm_runtime.internal()
    }

    pub fn disassemble(&self, function_name: &str) {
        let Some(index) = self
            .program
            .resolved_function_names
            .get(function_name)
            .map(|(index, _)| *index)
        else {
            return println!("function not found: {function_name}");
        };

        let baml_vm::Object::Function(function) = &self.program.objects[index] else {
            return println!("not a function: {function_name}");
        };

        baml_vm::debug::disassemble(
            function,
            &EvalStack::default(),
            &self.program.objects,
            &self.program.globals,
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_directory<T: AsRef<str>>(
        path: &std::path::Path,
        env_vars: HashMap<T, T>,
    ) -> anyhow::Result<Self> {
        Self::try_from(LlmRuntime::from_directory(path, env_vars)?)
    }

    pub fn from_file_content<T: AsRef<str> + std::fmt::Debug, U: AsRef<str>>(
        root_path: &str,
        files: &HashMap<T, T>,
        env_vars: HashMap<U, U>,
    ) -> anyhow::Result<Self> {
        Self::try_from(LlmRuntime::from_file_content(root_path, files, env_vars)?)
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
    ) -> (anyhow::Result<FunctionResult>, FunctionCallId) {
        // TODO: Proper error handling. Refactor the API to return a Result.
        let (function_index, function_kind) = self
            .program
            .resolved_function_names
            .get(&function_name)
            .unwrap_or_else(|| {
                todo!("function '{function_name}' not found, add proper error handling for this")
            });

        // If we're not running an expression function, then just delegate the
        // call to the LLM runtime.
        if matches!(function_kind, FunctionKind::Llm) {
            return self
                .llm_runtime
                .call_function(function_name, params, ctx, tb, cb, collectors, env_vars)
                .await;
        }

        let current_call_id = self
            .llm_runtime
            .tracer_wrapper
            .get_or_create_tracer(&env_vars)
            .start_call(&function_name, ctx, params, true, false, collectors.clone())
            .curr_call_id();

        let expr_fn = self
            .llm_runtime
            .internal()
            .ir()
            .expr_fns
            .iter()
            .find(|f| f.elem.name == function_name)
            .unwrap_or_else(|| panic!("expr function not found: {function_name}"));

        let output_type = expr_fn.elem.output.clone();

        // Fun begins here. Drive the VM boy :)

        // First create an "execution context". Each function call needs  new
        // VM. Imagine this in Python:
        //
        // asyncio.gather(b.FnA(), b.FnB())
        //
        // Those function calls are not sharing the same VM obviously. So we
        // instantiate a new one for each function call.
        //
        // TODO: This is expensive for big programs, figure out how to share
        // compiler produced objects betweeen VMs. We know they are read only.
        let mut vm = Vm::new(self.program.clone());

        // TODO: We can't assume arg ordering here is correct, figure out why.
        let args = Vec::from_iter(expr_fn.elem.inputs().iter().map(|(name, _)| {
            let Some(param) = params.get(name) else {
                panic!("missing parameter: {name}");
            };

            let vm_value = try_vm_value_from_baml_value(&mut vm, param)
                .unwrap_or_else(|e| panic!("failed to convert baml arg to vm value: {e}"));

            vm_value
        }));

        vm.set_entry_point(*function_index, &args);

        let (futures_tx, mut futures_rx) = tokio::sync::mpsc::unbounded_channel::<(
            ObjectIndex,
            (anyhow::Result<FunctionResult>, FunctionCallId),
        )>();

        let result = 'mainloop: loop {
            match vm.exec() {
                Ok(VmExecState::Await(idx)) => {
                    let mut fulfilled = false;

                    // Fulfil completed futures without blocking, if any.
                    // TODO: Handle errors.
                    while let Ok((ready_idx, (result, call_id))) = futures_rx.try_recv() {
                        let vm_value = vm_value_from_function_result(&mut vm, result);

                        if let Err(e) = vm.fulfil_future(ready_idx, vm_value) {
                            break 'mainloop Err(e.into());
                        }

                        if ready_idx == idx {
                            fulfilled = true;
                        }
                    }

                    // Now we do have to wait until the future is ready. Let
                    // Tokio take care of it.
                    while !fulfilled {
                        // TODO: Handle errors.
                        let (ready_idx, (result, call_id)) = futures_rx
                            .recv()
                            .await
                            .expect("failed to receive result from channel");

                        let vm_value = vm_value_from_function_result(&mut vm, result);

                        if let Err(e) = vm.fulfil_future(ready_idx, vm_value) {
                            break 'mainloop Err(e.into());
                        }

                        // After this one we don't have to wait for more futures
                        // even if they are running in the background, because
                        // the VM has not "awaited" them yet.
                        if ready_idx == idx {
                            fulfilled = true;
                        }
                    }
                }

                Ok(VmExecState::ScheduleFuture(idx)) => {
                    let pending_future = match vm.pending_future(idx) {
                        Ok(f) => f,
                        Err(e) => break Err(e.into()),
                    };

                    let llm_fn = self
                        .llm_runtime
                        .internal()
                        .ir()
                        .find_function(&pending_future.llm_function)
                        .unwrap_or_else(|_| {
                            panic!("LLM function not found: {}", pending_future.llm_function)
                        });

                    let llm_args = pending_future
                        .args
                        .iter()
                        .map(|v| try_baml_value_from_vm_value(&vm, v))
                        .collect::<Result<Vec<_>, _>>()
                        .unwrap_or_else(|e| panic!("failed to convert vm args to baml values: {e}"))
                        .into_iter()
                        .zip(llm_fn.inputs().iter().map(|(name, _)| name.to_owned()))
                        .map(|(arg, param_name)| (param_name, arg))
                        .collect::<BamlMap<_, _>>();

                    let future = {
                        let llm_runtime = Arc::clone(&self.llm_runtime);
                        let llm_fn_name = llm_fn.name().to_owned();
                        let ctx = ctx.clone();
                        let tb = tb.cloned();
                        let cb = cb.cloned();

                        // TODO: Collectors are not supported yet.
                        // let collectors = collectors.clone();
                        let env_vars = env_vars.clone();

                        let futures_tx = futures_tx.clone();

                        // Spanwed future basically awaits the LLM call and
                        // sends the result to the futures channel.
                        async move {
                            let result = llm_runtime
                                .call_function(
                                    llm_fn_name,
                                    &llm_args,
                                    &ctx,
                                    tb.as_ref(),
                                    cb.as_ref(),
                                    None,
                                    env_vars,
                                )
                                .await;

                            // TODO: Handle panic somehow.
                            futures_tx.send((idx, result)).unwrap_or_else(|e| {
                                panic!("failed to send result to channel: {e}")
                            });
                        }
                    };

                    // Multi threaded runtime spawn.
                    #[cfg(not(target_arch = "wasm32"))]
                    self.async_runtime.spawn(future);

                    // Spawning futures on WASM is a little bit more
                    // complicated. In WASM, Tokio does not support multi
                    // threaded runtimes, only single threaded, but the usual
                    // tokio::spawn API requires futures to impl Send, which in
                    // this case we do not because of how the
                    // RuntimeContextManager is built for WASM.
                    //
                    // So, instead of tokio::spawn, we use tokio::spawn_local
                    // which does not require Send and makes sure the future
                    // runs on the same thread that called tokio::spawn_local.
                    //
                    // The only difference is that the returned task JoinHandle
                    // is itself !Send, which means it can't be awaited from
                    // other threads.
                    //
                    // But it does not matter because in WASM we're not gonna
                    // await the task from another thread, wasm-bindgen-futures
                    // basically turns Rust futures into JavaScript promises,
                    // which are supposed to be single threaded. So, on WASM,
                    // except for compilation required types, spawn and
                    // spawn_local are essentially equivalent.
                    #[cfg(target_arch = "wasm32")]
                    tokio::task::spawn_local(future);
                }

                // VM completed execution, get the final result.
                Ok(VmExecState::Complete(value)) => break Ok(value),

                // VM error, stop execution.
                Err(e) => break Err(e),
            }
        };

        let baml_value = try_baml_value_from_vm_value(
            &vm,
            &result.unwrap_or_else(|e| panic!("failed to get vm result: {e}")),
        )
        .unwrap_or_else(|e| panic!("failed to convert vm result to baml value: {e}"));

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
    ) -> (anyhow::Result<FunctionResult>, FunctionCallId) {
        self.async_runtime.block_on(self.call_function(
            function_name,
            params,
            ctx,
            tb,
            cb,
            collectors,
            env_vars,
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
    ) -> anyhow::Result<FunctionResultStream> {
        self.llm_runtime
            .stream_function(function_name, params, ctx, tb, cb, collectors, env_vars)
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
    ) -> anyhow::Result<HTTPRequest> {
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
    ) -> anyhow::Result<HTTPRequest> {
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
}

impl ExperimentalTracingInterface for BamlAsyncVmRuntime {
    fn start_call(
        &self,
        function_name: &str,
        params: &BamlMap<String, BamlValue>,
        ctx: &RuntimeContextManager,
        env_vars: &std::collections::HashMap<String, String>,
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
        env_vars: &std::collections::HashMap<String, String>,
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
        env_vars: &std::collections::HashMap<String, String>,
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
        env_vars: &std::collections::HashMap<String, String>,
    ) -> anyhow::Result<uuid::Uuid> {
        self.llm_runtime.finish_call(call, result, ctx, env_vars)
    }

    #[cfg(target_arch = "wasm32")]
    async fn finish_call(
        &self,
        call: TracingCall,
        result: Option<BamlValue>,
        ctx: &RuntimeContextManager,
        env_vars: &std::collections::HashMap<String, String>,
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

fn try_baml_value_from_vm_value(vm: &Vm, value: &baml_vm::Value) -> anyhow::Result<BamlValue> {
    match value {
        baml_vm::Value::Null => Ok(BamlValue::Null),
        baml_vm::Value::Bool(b) => Ok(BamlValue::Bool(*b)),
        baml_vm::Value::Int(n) => Ok(BamlValue::Int(*n)),
        baml_vm::Value::Float(f) => Ok(BamlValue::Float(*f)),

        baml_vm::Value::Object(o) => match &vm.objects[*o] {
            baml_vm::Object::String(s) => Ok(BamlValue::String(s.clone())),
            baml_vm::Object::Array(a) => Ok(BamlValue::List(
                a.iter()
                    .map(|v| try_baml_value_from_vm_value(vm, v))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            _ => anyhow::bail!("unsupported object: {:?}", o),
        },
    }
}

fn try_vm_value_from_baml_value(vm: &mut Vm, value: &BamlValue) -> anyhow::Result<baml_vm::Value> {
    match value {
        BamlValue::Null => Ok(baml_vm::Value::Null),
        BamlValue::Bool(b) => Ok(baml_vm::Value::Bool(*b)),
        BamlValue::Int(n) => Ok(baml_vm::Value::Int(*n)),
        BamlValue::Float(f) => Ok(baml_vm::Value::Float(*f)),
        BamlValue::List(l) => {
            let mut array = Vec::with_capacity(l.len());

            for v in l {
                array.push(try_vm_value_from_baml_value(vm, v)?);
            }

            Ok(vm.alloc_array(array))
        }
        _ => todo!("handle strings and objects"),
    }
}

fn vm_value_from_function_result(
    vm: &mut Vm,
    result: anyhow::Result<FunctionResult>,
) -> baml_vm::Value {
    let fn_result = result.unwrap_or_else(|e| panic!("failed to get function result: {e}"));

    // TODO: I don't know what the fuck this is.
    let baml_value = fn_result
        .parsed()
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap()
        .clone()
        .0
        .value();

    let vm_value = try_vm_value_from_baml_value(vm, &baml_value)
        .unwrap_or_else(|e| panic!("failed to convert result to vm value: {e}"));

    vm_value
}
