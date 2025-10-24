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
    str::FromStr,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, Context};
use baml_compiler::{
    self,
    watch::{self, SharedWatchHandler},
};
use baml_ids::FunctionCallId;
use baml_types::{tracing::events::HTTPRequest, BamlMap, BamlValue, BamlValueWithMeta, Completion};
use baml_vm::{BamlVmProgram, EvalStack, FunctionKind, ObjectIndex, Vm, VmExecState};
use internal_baml_core::ir::IRHelper;
use jsonish::{deserializer::deserialize_flags::Flag, ResponseBamlValue, ResponseValueMeta};

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

/// Async VM runtime.
///
/// This is an async wrapper over our synchronous, single-threaded VM. The VM
/// yields control flow to this struct when futures have to be created and when
/// it's blocked awaiting a future. From there, the async runtime schedules
/// Tokio futures or awaits them, respectively. After that control flow goes
/// back to the VM and bytecode execution continues.
#[derive(Clone)]
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

        let program = baml_compiler::compile(&llm_runtime.db)?;

        Ok(Self {
            llm_runtime: Arc::new(llm_runtime),
            program,

            #[cfg(not(target_arch = "wasm32"))]
            async_runtime,
        })
    }
}

impl BamlAsyncVmRuntime {
    pub fn internal(&self) -> &LlmRuntime {
        &self.llm_runtime
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
            &EvalStack::new(),
            &self.program.objects,
            &self.program.globals,
        );
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

    // TODO: Tuple return type (Result, FunctionCallId) makes it hard to use
    // early returns and `?` syntax. Change this.
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
        watch_handler: Option<SharedWatchHandler>,
    ) -> (anyhow::Result<FunctionResult>, FunctionCallId) {
        // Find the function.
        let Some((function_index, function_kind)) =
            self.program.resolved_function_names.get(&function_name)
        else {
            // TODO: We don't have an ID here! We can't call tracer here for llm functions here.
            return (
                Err(anyhow!("function '{function_name}' not found")),
                FunctionCallId::new(),
            );
        };

        // If we're not running an expression function, then just delegate the
        // call to the LLM runtime.
        if matches!(function_kind, FunctionKind::Llm) {
            return self
                .llm_runtime
                .call_function(
                    function_name,
                    params,
                    ctx,
                    tb,
                    cb,
                    collectors,
                    env_vars,
                    tags,
                    cancel_tripwire,
                )
                .await;
        }
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
                None,
            )
            .curr_call_id();

        let Some(expr_fn) = self
            .llm_runtime
            .ir()
            .expr_fns
            .iter()
            .find(|f| f.elem.name == function_name)
        else {
            return (
                Err(anyhow!("function '{function_name}' not found")),
                current_call_id,
            );
        };

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
        let mut vm = Vm::new(self.program.clone(), env_vars.clone());

        // TODO: We can't assume ordering of `params` is correct, figure out why.
        let args = match expr_fn
            .elem
            .inputs()
            .iter()
            .map(|(name, _)| {
                let Some(param) = params.get(name) else {
                    anyhow::bail!("missing parameter: {name}");
                };

                try_vm_value_from_baml_value(
                    &mut vm,
                    &self.program.resolved_class_names,
                    &self.program.resolved_enums_names,
                    param,
                )
                .context("failed to convert baml argument to vm value")
            })
            .collect::<Result<Vec<_>, _>>()
            .context("failed to convert baml args to vm values")
        {
            Ok(args) => args,
            Err(e) => return (Err(e), current_call_id),
        };

        vm.set_entry_point(*function_index, &args);

        let (futures_tx, mut futures_rx) = tokio::sync::mpsc::unbounded_channel::<(
            ObjectIndex,
            (anyhow::Result<FunctionResult>, FunctionCallId),
        )>();

        let tags_clone = tags.cloned();
        let vm_result = 'mainloop: loop {
            match vm.exec() {
                Ok(VmExecState::Await(idx)) => {
                    let mut fulfilled = false;

                    while let Ok((ready_idx, (result, call_id))) = futures_rx.try_recv() {
                        let vm_value = match try_vm_value_from_function_result(
                            &mut vm,
                            &self.program.resolved_class_names,
                            &self.program.resolved_enums_names,
                            result,
                        ) {
                            Ok(vm_value) => vm_value,
                            Err(e) => break 'mainloop Err(e),
                        };

                        if let Err(e) = vm.fulfil_future(ready_idx, vm_value) {
                            break 'mainloop Err(
                                anyhow::Error::from(e).context("failed to fulfil VM future")
                            );
                        }

                        if ready_idx == idx {
                            fulfilled = true;
                        }
                    }

                    // Now we do have to wait until the future is ready. Let
                    // Tokio take care of it.
                    while !fulfilled {
                        // TODO: Handle errors.
                        let (ready_idx, (result, call_id)) = match futures_rx.recv().await {
                            Some(result) => result,

                            // This should not happen because VM will never close the channel.
                            None => {
                                break 'mainloop Err(anyhow!(
                                    "failed to receive function result from futures channel (channel closed)"
                                ));
                            }
                        };

                        let vm_value = match try_vm_value_from_function_result(
                            &mut vm,
                            &self.program.resolved_class_names,
                            &self.program.resolved_enums_names,
                            result,
                        ) {
                            Ok(vm_value) => vm_value,
                            Err(e) => {
                                break 'mainloop Err(
                                    e.context("failed to convert function result to vm value")
                                );
                            }
                        };

                        if let Err(e) = vm.fulfil_future(ready_idx, vm_value) {
                            break 'mainloop Err(
                                anyhow::Error::from(e).context("failed to fulfil VM future")
                            );
                        }

                        // After this one we don't have to wait for more futures
                        // even if they are running in the background, because
                        // the VM has not "awaited" them yet.
                        if ready_idx == idx {
                            fulfilled = true;
                        }
                    }
                }

                Ok(VmExecState::Notify(notification)) => {
                    log::debug!("[VM] Notify: {notification:?}");
                    match notification {
                        baml_vm::vm::WatchNotification::Variables(nodes) => {
                            for node in nodes {
                                let state = vm.watch.root_state(node).unwrap();
                                let baml_vm::watch::NodeId::LocalVar(stack_index) = node else {
                                    break 'mainloop Err(anyhow!("expected local variable notification, got object notification {:?}", node));
                                };
                                let (watched_var_name, function_name) =
                                    vm.watched_vars.get(&stack_index).unwrap();
                                baml_log::debug!("[VM] Notify: {}", &state.channel);

                                let fake_meta = watch::WatchValueMetadata {
                                    constraints: Vec::new(),
                                    response_checks: Vec::new(),
                                    completion: Completion::default(),
                                    r#type: baml_types::TypeIR::Top(Default::default()),
                                };

                                let current_value =
                                    try_baml_value_from_vm_value(&vm, &state.value).unwrap();

                                let baml_value_with_meta =
                                    BamlValueWithMeta::with_const_meta(&current_value, fake_meta);

                                let notification = watch::WatchNotification::new_var(
                                    watched_var_name.to_owned(), // variable name
                                    state.channel.to_owned(),    // channel name
                                    baml_value_with_meta,
                                    function_name.to_owned(),
                                );

                                if let Some(handler) = watch_handler.as_ref() {
                                    if let Ok(mut handler) = handler.lock() {
                                        handler.notify(notification);
                                    }
                                }
                            }
                        }
                        baml_vm::vm::WatchNotification::Block(notification) => {
                            if let Some(handler) = watch_handler.as_ref() {
                                if let Ok(mut handler) = handler.lock() {
                                    handler.notify(watch::WatchNotification::new_block(
                                        notification.block_name.as_str().to_string(),
                                        notification.function_name.as_str().to_string(),
                                    ));
                                }
                            }
                        }
                    }
                }

                Ok(VmExecState::ScheduleFuture(idxan)) => {
                    let pending_future = match vm.pending_future(idxan) {
                        Ok(f) => f,
                        Err(e) => {
                            break 'mainloop Err(
                                anyhow::Error::from(e).context("failed to get pending future")
                            );
                        }
                    };

                    match pending_future.kind {
                        baml_vm::FutureKind::Llm => {
                            let llm_fn = match self
                                .llm_runtime
                                .ir()
                                .find_function(&pending_future.function)
                            {
                                Ok(f) => f,
                                Err(e) => {
                                    break 'mainloop Err(e.context(format!(
                                        "Failed scheduling LLM future: {}",
                                        pending_future.function
                                    )));
                                }
                            };

                            let llm_args = match pending_future
                                .args
                                .iter()
                                .map(|v| try_baml_value_from_vm_value(&vm, v))
                                .collect::<Result<Vec<_>, _>>()
                            {
                                Ok(args) => args,
                                Err(e) => {
                                    break 'mainloop Err(
                                        e.context("failed to convert VM args to baml values")
                                    );
                                }
                            }
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
                                let tags_for_future = tags_clone.clone();

                                let futures_tx = futures_tx.clone();

                                let cancel_tripwire = cancel_tripwire.to_owned();

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
                                            tags_for_future.as_ref(),
                                            cancel_tripwire,
                                        )
                                        .await;

                                    // TODO: Handle panic somehow.
                                    futures_tx.send((idxan, result)).unwrap_or_else(|e| {
                                        panic!("failed to send LLM function result to futures channel: {e}")
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
                            wasm_bindgen_futures::spawn_local(future);
                        }

                        baml_vm::FutureKind::Net => {
                            // Only `baml.fetch_as` is supported for now.
                            if pending_future.function != "baml.fetch_as" {
                                break 'mainloop Err(anyhow!(
                                    "unkown function: {}",
                                    pending_future.function
                                ));
                            }

                            if pending_future.args.len() != 2 {
                                break 'mainloop Err(anyhow!(
                                    "expected 2 arguments for `baml.fetch_as`, got {}",
                                    pending_future.args.len()
                                ));
                            }

                            let url_or_request =
                                match try_baml_value_from_vm_value(&vm, &pending_future.args[0]) {
                                    Ok(url_or_request) => url_or_request,
                                    Err(e) => {
                                        break 'mainloop Err(e.context(
                                        "baml.fetch_as: failed to get url or request from VM value",
                                    ));
                                    }
                                };

                            match &url_or_request {
                                BamlValue::String(_) => {} // Ok,

                                BamlValue::Class(name, _) if name == "baml.HttpRequest" => {} // Ok,

                                _ => {
                                    break 'mainloop Err(anyhow!(
                                        "baml.fetch_as: expected baml.fetch_as arg to be a string or HttpRequest, got {}",
                                        url_or_request
                                    ));
                                }
                            }

                            let parse_as_type = match vm
                                .objects
                                .as_object(&pending_future.args[1], baml_vm::ObjectType::Any)
                            {
                                Ok(idx) => match &vm.objects[idx] {
                                    baml_vm::Object::BamlType(type_ir) => type_ir.to_owned(),
                                    _ => {
                                        break 'mainloop Err(anyhow!(
                                            "baml.fetch_as: expected type parameter to be a Baml type, got {}",
                                            vm.objects[idx]
                                        ));
                                    }
                                },
                                Err(e) => {
                                    break 'mainloop Err(anyhow::Error::from(e).context(
                                        "baml.fetch_as: failed to get type parameter from VM value",
                                    ));
                                }
                            };

                            let future = {
                                let futures_tx = futures_tx.clone();
                                let cancel_tripwire = cancel_tripwire.to_owned();
                                let current_call_id = current_call_id.to_owned();

                                let output_format = jsonish::helpers::render_output_format(
                                    &self.llm_runtime.ir,
                                    &parse_as_type,
                                    &baml_types::EvaluationContext::default(),
                                    baml_types::StreamingMode::NonStreaming,
                                )
                                .unwrap();

                                async move {
                                    let response = 'res: {
                                        let client = reqwest::Client::new();

                                        let req = match &url_or_request {
                                            BamlValue::String(url) => {
                                                client.get(url)
                                            }

                                            // we type checked earlier, should be http request type
                                            BamlValue::Class(name, fields) => {
                                                let Some(BamlValue::String(url)) = fields.get("url") else {
                                                    break 'res Err(anyhow!(
                                                        "baml.fetch_as: expected url to be a string, got {}",
                                                        url_or_request
                                                    ));
                                                };

                                                let Some(BamlValue::Enum(_, method)) = fields.get("method") else {
                                                    break 'res Err(anyhow!(
                                                        "baml.fetch_as: expected method to be a valid HTTP method, got {}",
                                                        url_or_request
                                                    ));
                                                };

                                                let mut req = match method.as_str() {
                                                    "Get" => client.get(url),
                                                    "Post" => client.post(url),
                                                    "Put" => client.put(url),
                                                    "Patch" => client.patch(url),
                                                    "Delete" => client.delete(url),
                                                    _ => break 'res Err(anyhow!(
                                                        "baml.fetch_as: expected method to be a valid HTTP method, got {}",
                                                        method
                                                    ))
                                                };

                                                if let Some(BamlValue::Map(headers)) = fields.get("headers") {
                                                    let mut header_map = reqwest::header::HeaderMap::new();

                                                    for (k, v) in headers {
                                                        let Ok(key) = reqwest::header::HeaderName::from_str(k) else {
                                                            break 'res Err(anyhow!(
                                                                "baml.fetch_as: expected header key to be a valid HTTP header name, got {}",
                                                                k
                                                            ));
                                                        };

                                                        let Some(value_as_string) = v.as_str() else {
                                                            break 'res Err(anyhow!(
                                                                "baml.fetch_as: expected header value to be a string, got {}",
                                                                v
                                                            ));
                                                        };

                                                        let Ok(value) = reqwest::header::HeaderValue::from_str(value_as_string) else {
                                                            break 'res Err(anyhow!(
                                                                "baml.fetch_as: expected header value to be a string, got {}",
                                                                v
                                                            ));
                                                        };

                                                        header_map.insert(key, value);
                                                    }

                                                    req = req.headers(header_map);
                                                }

                                                if let Some(BamlValue::Map(query_params)) = fields.get("query_params") {
                                                    req = req.query(query_params);
                                                }

                                                if let Some(json) = fields.get("json") {
                                                    req = req.json(json)
                                                }

                                                req
                                            }

                                            _ => break 'res Err(anyhow!(
                                                "baml.fetch_as: expected baml.fetch_as arg to be a string or HttpRequest, got {}",
                                                url_or_request
                                            ))
                                        };

                                        let res = match req.send().await {
                                            Ok(res) => res,
                                            Err(e) => {
                                                break 'res Err(anyhow::Error::from(e).context(
                                                    "baml.fetch_as: failed to send request",
                                                ));
                                            }
                                        };

                                        let status = res.status();

                                        let body = match res.text().await {
                                            Ok(body) => body,
                                            Err(e) => {
                                                break 'res Err(anyhow::Error::from(e).context(
                                                    "baml.fetch_as: failed to read response body",
                                                ));
                                            }
                                        };

                                        if status.is_client_error() || status.is_server_error() {
                                            break 'res Err(anyhow::anyhow!(
                                            "baml.fetch_as: HTTP request failed: HTTP {status}\nBody: {body}"
                                        ));
                                        }

                                        jsonish::from_str(
                                            &output_format,
                                            &parse_as_type,
                                            &body,
                                            true,
                                        )
                                        .context(
                                            "(jsonish) Failed parsing response of fetch_value call",
                                        )
                                    };

                                    let response_baml_value = response.map(|r| {
                                        ResponseBamlValue(
                                            BamlValueWithMeta::<Vec<Flag>>::from(r).map_meta(
                                                |_| {
                                                    ResponseValueMeta(
                                                        vec![],
                                                        vec![],
                                                        Completion::default(),
                                                        parse_as_type.clone(),
                                                    )
                                                },
                                            ),
                                        )
                                    });

                                    let result = FunctionResult::new(
                                        Default::default(),
                                        LlmRuntime::dummy_llm_placeholder_for_expr_fn(),
                                        Some(response_baml_value),
                                    );

                                    // TODO: Handle panic somehow.
                                    futures_tx.send((idxan, (Ok(result), current_call_id))).unwrap_or_else(|e| {
                                        panic!("failed to send LLM function result to futures channel: {e}")
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
                    };
                }

                // VM completed execution, get the final result.
                Ok(VmExecState::Complete(value)) => break Ok(value),

                // VM error, stop execution.
                Err(e) => break Err(e.into()),
            }
        };

        let vm_value = match vm_result {
            Ok(vm_value) => vm_value,
            Err(e) => {
                return (Err(e.context("VM execution failed")), current_call_id);
            }
        };

        let baml_value = match try_baml_value_from_vm_value(&vm, &vm_value) {
            Ok(baml_value) => baml_value,
            Err(e) => {
                return (
                    Err(e.context("failed to convert vm result to baml value")),
                    current_call_id,
                );
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
        tags: Option<&HashMap<String, String>>,
        cancel_tripwire: Arc<TripWire>,
        watch_handler: Option<SharedWatchHandler>,
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
            watch_handler,
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
            cancel_tripwire,
            tags,
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
        watch_handler: Option<SharedWatchHandler>,
    ) -> (
        Result<crate::TestResponse, anyhow::Error>,
        baml_ids::FunctionCallId,
    )
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
                watch_handler,
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
            futures::channel::mpsc::UnboundedSender<
                Vec<internal_baml_core::internal_baml_diagnostics::SerializedSpan>,
            >,
        >,
        collector: Option<Arc<crate::tracingv2::storage::storage::Collector>>,
        env_vars: HashMap<String, String>,
        tags: Option<HashMap<String, String>>,
        cancel_tripwire: Arc<crate::TripWire>,
        on_tick: Option<G>,
        watch_handler: Option<SharedWatchHandler>,
    ) -> (
        Result<crate::TestResponse, anyhow::Error>,
        baml_ids::FunctionCallId,
    )
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
                watch_handler,
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
    ) -> Result<BamlMap<String, BamlValue>, anyhow::Error> {
        self.llm_runtime
            .get_test_params(function_name, test_name, ctx, strict)
    }

    pub fn get_test_type_builder(
        &self,
        function_name: &str,
        test_name: &str,
    ) -> Result<Option<TypeBuilder>, anyhow::Error> {
        self.llm_runtime
            .get_test_type_builder_impl(function_name, test_name)
    }

    pub fn tracer_wrapper(&self) -> &Arc<BamlTracerWrapper> {
        &self.llm_runtime.tracer_wrapper
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

        baml_vm::Value::Object(obj_index) => match &vm.objects[*obj_index] {
            baml_vm::Object::String(string) => Ok(BamlValue::String(string.clone())),

            baml_vm::Object::Media(media) => Ok(BamlValue::Media(media.clone())),

            baml_vm::Object::Array(array) => Ok(BamlValue::List(
                array
                    .iter()
                    .map(|v| try_baml_value_from_vm_value(vm, v))
                    .collect::<Result<Vec<_>, _>>()?,
            )),

            baml_vm::Object::Map(vm_map) => {
                let mut baml_map = BamlMap::new();

                for (k, v) in vm_map {
                    baml_map.insert(k.clone(), try_baml_value_from_vm_value(vm, v)?);
                }

                Ok(BamlValue::Map(baml_map))
            }

            baml_vm::Object::Instance(instance) => {
                let baml_vm::Object::Class(class) = &vm.objects[instance.class] else {
                    anyhow::bail!(
                        "internal error: cannot convert VM value {value} to Baml value: class ID '{}' not found in VM objects",
                        instance.class
                    );
                };

                let mut fields = BamlMap::new();
                for (i, v) in instance.fields.iter().enumerate() {
                    fields.insert(
                        class.field_names[i].clone(),
                        try_baml_value_from_vm_value(vm, v)?,
                    );
                }

                Ok(BamlValue::Class(class.name.clone(), fields))
            }

            baml_vm::Object::Variant(variant) => {
                let baml_vm::Object::Enum(enm) = &vm.objects[variant.enm] else {
                    anyhow::bail!(
                        "internal error: cannot convert VM value {value} to Baml value: enum ID '{}' not found in VM objects",
                        variant.enm
                    );
                };

                Ok(BamlValue::Enum(
                    enm.name.clone(),
                    enm.variant_names[variant.index].clone(),
                ))
            }

            baml_vm::Object::Future(_)
            | baml_vm::Object::Class(_)
            | baml_vm::Object::Enum(_)
            | baml_vm::Object::Function(_)
            | baml_vm::Object::BamlType(_) => anyhow::bail!(
                "internal error: unsupported VM object to BamlValue convertion: {}",
                vm.objects[*obj_index]
            ),
        },
    }
}

fn try_vm_value_from_baml_value(
    vm: &mut Vm,
    resolved_class_names: &HashMap<String, ObjectIndex>,
    resolved_enums_names: &HashMap<String, ObjectIndex>,
    value: &BamlValue,
) -> anyhow::Result<baml_vm::Value> {
    match value {
        BamlValue::Null => Ok(baml_vm::Value::Null),
        BamlValue::Bool(b) => Ok(baml_vm::Value::Bool(*b)),
        BamlValue::Int(n) => Ok(baml_vm::Value::Int(*n)),
        BamlValue::Float(f) => Ok(baml_vm::Value::Float(*f)),

        BamlValue::String(s) => Ok(vm.alloc_string(s.clone())),

        BamlValue::List(l) => {
            let mut array = Vec::with_capacity(l.len());

            for v in l {
                array.push(try_vm_value_from_baml_value(
                    vm,
                    resolved_class_names,
                    resolved_enums_names,
                    v,
                )?);
            }

            Ok(vm.alloc_array(array))
        }

        BamlValue::Map(map) => {
            let mut vm_map = BamlMap::new();

            for (k, v) in map {
                vm_map.insert(
                    k.to_owned(),
                    try_vm_value_from_baml_value(
                        vm,
                        resolved_class_names,
                        resolved_enums_names,
                        v,
                    )?,
                );
            }

            Ok(vm.alloc_map(vm_map))
        }

        BamlValue::Class(name, fields) => {
            let Some(class_index) = resolved_class_names.get(name) else {
                anyhow::bail!("cannot convert value {value} to VM value: class '{name}' not found");
            };

            let baml_vm::Object::Class(class) = &vm.objects[*class_index] else {
                anyhow::bail!(
                    "internal error: cannot convert value {value} to VM value: class '{name}' not found in VM objects"
                );
            };

            let mut ordered_field_values = Vec::new();
            for field_name in &class.field_names {
                let Some(value) = fields.get(field_name) else {
                    anyhow::bail!(
                        "cannot convert value {value} to VM value: class '{name}' has no field '{field_name}'"
                    );
                };

                ordered_field_values.push(value);
            }

            let mut vm_fields_layout = Vec::new();
            for v in ordered_field_values {
                vm_fields_layout.push(try_vm_value_from_baml_value(
                    vm,
                    resolved_class_names,
                    resolved_enums_names,
                    v,
                )?);
            }

            Ok(vm.alloc_instance(*class_index, vm_fields_layout))
        }

        BamlValue::Enum(enm, variant) => {
            let Some(enum_index) = resolved_enums_names.get(enm) else {
                anyhow::bail!("cannot convert value {value} to VM value: enum '{enm}' not found");
            };

            let baml_vm::Object::Enum(enm) = &vm.objects[*enum_index] else {
                anyhow::bail!(
                    "internal error: cannot convert value {value} to VM value: enum '{enm}' not found in VM objects"
                );
            };

            let Some(variant_index) = enm.variant_names.iter().position(|v| v == variant) else {
                anyhow::bail!(
                    "cannot convert value {value} to VM value: enum '{enm}' has no variant '{variant}'"
                );
            };

            Ok(vm.alloc_variant(*enum_index, variant_index))
        }

        BamlValue::Media(media) => Ok(vm.alloc_media(media.clone())),
    }
}

fn try_vm_value_from_function_result(
    vm: &mut Vm,
    resolved_class_names: &HashMap<String, ObjectIndex>,
    resolved_enums_names: &HashMap<String, ObjectIndex>,
    result: anyhow::Result<FunctionResult>,
) -> anyhow::Result<baml_vm::Value> {
    let fn_result = result.context("failed to get function result")?;

    // TODO: Return type of .parsed() sucks.
    let baml_value = fn_result
        .parsed()
        .as_ref()
        .ok_or_else(|| anyhow!("no parsed result available from function call"))?
        .as_ref()
        .map_err(|e| anyhow!("error parsing function result: {e}"))?
        .clone()
        .0
        .value();

    try_vm_value_from_baml_value(vm, resolved_class_names, resolved_enums_names, &baml_value)
}

impl crate::runtime_interface::InternalRuntimeInterface for BamlAsyncVmRuntime {
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
    ) -> anyhow::Result<Vec<crate::internal::llm_client::orchestrator::OrchestratorNode>> {
        self.llm_runtime.orchestration_graph(client_name, ctx)
    }

    fn function_graph(
        &self,
        function_name: &str,
        ctx: &crate::runtime_context::RuntimeContext,
    ) -> anyhow::Result<String> {
        self.llm_runtime.function_graph(function_name, ctx)
    }

    fn get_function<'ir>(
        &'ir self,
        function_name: &str,
    ) -> anyhow::Result<internal_baml_core::ir::FunctionWalker<'ir>> {
        self.llm_runtime.get_function(function_name)
    }

    fn get_expr_function<'ir>(
        &'ir self,
        function_name: &str,
        ctx: &crate::runtime_context::RuntimeContext,
    ) -> anyhow::Result<internal_baml_core::ir::ExprFunctionWalker<'ir>> {
        self.llm_runtime.get_expr_function(function_name, ctx)
    }

    async fn render_prompt(
        &self,
        function_name: &str,
        ctx: &crate::runtime_context::RuntimeContext,
        params: &BamlMap<String, BamlValue>,
        node_index: Option<usize>,
    ) -> anyhow::Result<(
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
    ) -> anyhow::Result<String> {
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
    ) -> anyhow::Result<BamlMap<String, BamlValue>> {
        self.llm_runtime
            .get_test_params(function_name, test_name, ctx, strict)
    }

    fn get_test_constraints(
        &self,
        function_name: &str,
        test_name: &str,
        ctx: &crate::runtime_context::RuntimeContext,
    ) -> anyhow::Result<Vec<baml_types::Constraint>> {
        self.llm_runtime
            .get_test_constraints(function_name, test_name, ctx)
    }

    fn get_test_type_builder(
        &self,
        function_name: &str,
        test_name: &str,
    ) -> anyhow::Result<Option<TypeBuilder>> {
        self.llm_runtime
            .get_test_type_builder(function_name, test_name)
    }
}
