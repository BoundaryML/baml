//! WASM io implementation via JS callback.
//!
//! `WasmIo` holds the JS `input` function and implements the io `sys_ops`.
//! The JS callback receives an optional prompt string and returns a string
//! (or `Promise<string>`).

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use bex_events::run::{
    HostCallId, InMemoryRunStore, OutputStream, RequestCommandOutcome, RunRequestState,
};
use js_sys::{Function, Promise};
use sys_ops::io::IoNamespaceIo;
use sys_types::{BexHeap, CallId, SysOpContext, SysOpOutput, VmBamlError, VmRustFnError};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::send_wrapper::{SendFuture, SendWrapper};

/// WASM io implementation that holds the JS `input` callback.
///
/// Signature of the JS function: `(requestId: number, prompt: string | undefined) => Promise<string> | string`
pub(crate) struct WasmIo {
    input_fn: SendWrapper<Function>,
    run_store: Arc<InMemoryRunStore>,
    notification_callback: SendWrapper<Function>,
    next_request_id: AtomicU64,
}

impl WasmIo {
    pub(crate) fn new(
        input_fn: Function,
        run_store: Arc<InMemoryRunStore>,
        notification_callback: SendWrapper<Function>,
    ) -> Self {
        Self {
            input_fn: SendWrapper::new(input_fn),
            run_store,
            notification_callback,
            next_request_id: AtomicU64::new(1),
        }
    }

    fn input_fn(&self) -> &Function {
        self.input_fn.inner()
    }

    /// Record a `baml.io` stream write and notify the webview.
    ///
    /// A write that cannot be attributed to a live run is dropped rather than
    /// failed. Panicking a program over an unroutable debug print costs more
    /// than the lost line.
    fn write_output(&self, call_id: CallId, stream: OutputStream, text: String) {
        if text.is_empty() {
            return;
        }
        let Some(host_call_id) = crate::runs::wasm_host_call_id(call_id) else {
            return;
        };
        if let Some(patch) = self.run_store.ingest_output(&host_call_id, stream, text) {
            crate::runs::send_run_patch(&self.notification_callback, &patch);
        }
    }
}

impl IoNamespaceIo for WasmIo {
    fn input(
        &self,
        _heap: &Arc<BexHeap>,
        call_id: CallId,
        prompt: Option<String>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        let input_fn = self.input_fn().clone();
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let host_call_id = crate::runs::wasm_host_call_id(call_id);
        #[allow(clippy::cast_precision_loss)] // request IDs are small sequential integers
        let js_request_id = wasm_bindgen::JsValue::from_f64(request_id as f64);
        let js_prompt = match &prompt {
            Some(p) => wasm_bindgen::JsValue::from_str(p),
            None => wasm_bindgen::JsValue::UNDEFINED,
        };
        let result = input_fn
            .call2(&wasm_bindgen::JsValue::NULL, &js_request_id, &js_prompt)
            .map_err(|e| {
                let msg = e.as_string().unwrap_or_else(|| format!("{e:?}"));
                VmBamlError::Io {
                    message: format!("Failed to call input function: {msg}"),
                }
            });
        let result = match result {
            Ok(result) => result,
            Err(e) => return SysOpOutput::err(e),
        };
        if let Some(host_call_id) = &host_call_id
            && let Some(patch) =
                self.run_store
                    .ingest_input_requested(host_call_id, request_id, prompt)
        {
            crate::runs::send_run_patch(&self.notification_callback, &patch);
        }

        if result.is_instance_of::<Promise>() {
            let promise: Promise = result.unchecked_into();
            let run_store = self.run_store.clone();
            let notification_callback = self.notification_callback.clone();
            return SysOpOutput::async_op(SendFuture(async move {
                let result = JsFuture::from(promise).await.map_err(|e| {
                    let msg = e.as_string().unwrap_or_else(|| format!("{e:?}"));
                    VmBamlError::Io {
                        message: format!("Input callback promise rejected: {msg}"),
                    }
                })?;
                let value = result
                    .as_string()
                    .ok_or_else(|| VmBamlError::DevOther {
                        message: "Input callback did not return a string".into(),
                    })
                    .map_err(VmRustFnError::from)?;
                publish_input_resolved(
                    &run_store,
                    &notification_callback,
                    host_call_id.as_ref(),
                    request_id,
                );
                Ok(value)
            }));
        }

        match result.as_string() {
            Some(s) => {
                publish_input_resolved(
                    &self.run_store,
                    &self.notification_callback,
                    host_call_id.as_ref(),
                    request_id,
                );
                SysOpOutput::ok(s)
            }
            None => SysOpOutput::err(VmBamlError::DevOther {
                message: "Input callback did not return a string".into(),
            }),
        }
    }

    fn print(
        &self,
        _heap: &Arc<BexHeap>,
        call_id: CallId,
        s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        self.write_output(call_id, OutputStream::Stdout, s);
        SysOpOutput::ok(())
    }
    fn println(
        &self,
        _heap: &Arc<BexHeap>,
        call_id: CallId,
        mut s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        s.push('\n');
        self.write_output(call_id, OutputStream::Stdout, s);
        SysOpOutput::ok(())
    }
    fn eprint(
        &self,
        _heap: &Arc<BexHeap>,
        call_id: CallId,
        s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        self.write_output(call_id, OutputStream::Stderr, s);
        SysOpOutput::ok(())
    }
    fn eprintln(
        &self,
        _heap: &Arc<BexHeap>,
        call_id: CallId,
        mut s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        s.push('\n');
        self.write_output(call_id, OutputStream::Stderr, s);
        SysOpOutput::ok(())
    }
}

fn publish_input_resolved(
    run_store: &InMemoryRunStore,
    notification_callback: &SendWrapper<Function>,
    host_call_id: Option<&HostCallId>,
    request_id: u64,
) {
    let Some(host_call_id) = host_call_id else {
        return;
    };
    if let Some(boundary_id) = run_store.boundary_id_for_host_call(host_call_id) {
        let result = run_store.resolve_input_request_for_run(
            boundary_id,
            request_id,
            RunRequestState::Resolved,
        );
        if result.outcome == RequestCommandOutcome::Accepted
            && let Some(patch) = result.patch
        {
            crate::runs::send_run_patch(notification_callback, &patch);
        }
        return;
    }

    if let Some(patch) =
        run_store.ingest_input_resolved(host_call_id, request_id, RunRequestState::Resolved)
    {
        crate::runs::send_run_patch(notification_callback, &patch);
    }
}
