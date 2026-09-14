//! WASM env implementation via JS callback.
//!
//! `WasmEnv` holds the JS `env_vars` function and implements the env `sys_ops`.
//! The JS callback returns a `Promise<string | undefined>`, allowing the
//! host page to show an interactive prompt and resolve when the user submits.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use bex_events::run::{EnvResolutionStatus, HostCallId, InMemoryRunStore, RequestCommandOutcome};
use js_sys::{Function, Promise};
use sys_ops::io::IoNamespaceEnv;
use sys_types::{BexHeap, CallId, SysOpContext, SysOpOutput, VmBamlError, VmRustFnError};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::send_wrapper::{SendFuture, SendWrapper};

/// WASM env implementation that holds the JS `env_vars` callback.
///
/// Signature of the JS function: `(var: string, requestId: number) => Promise<string | undefined>`
pub(crate) struct WasmEnv {
    /// The JS function to call for env lookups.
    env_fn: SendWrapper<Function>,
    run_store: Arc<InMemoryRunStore>,
    notification_callback: SendWrapper<Function>,
    next_request_id: AtomicU64,
}

impl WasmEnv {
    pub(crate) fn new(
        env_fn: Function,
        run_store: Arc<InMemoryRunStore>,
        notification_callback: SendWrapper<Function>,
    ) -> Self {
        Self {
            env_fn: SendWrapper::new(env_fn),
            run_store,
            notification_callback,
            next_request_id: AtomicU64::new(1),
        }
    }

    fn env_fn(&self) -> &Function {
        self.env_fn.inner()
    }
}

fn js_to_env_value(value: &wasm_bindgen::JsValue) -> Result<Option<String>, VmBamlError> {
    if value.is_undefined() || value.is_null() {
        return Ok(None);
    }
    let s = value.as_string().ok_or_else(|| VmBamlError::Io {
        message: "Env function did not return a string or undefined".into(),
    })?;
    Ok(Some(s))
}

impl IoNamespaceEnv for WasmEnv {
    fn get(
        &self,
        _heap: &Arc<BexHeap>,
        call_id: CallId,
        key: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        let env_fn = self.env_fn().clone();
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let host_call_id = crate::runs::wasm_host_call_id(call_id);
        #[allow(clippy::cast_precision_loss)]
        let js_request_id = wasm_bindgen::JsValue::from_f64(request_id as f64);
        let result = env_fn
            .call2(
                &wasm_bindgen::JsValue::NULL,
                &key.clone().into(),
                &js_request_id,
            )
            .map_err(|e| {
                let msg = e.as_string().unwrap_or_else(|| format!("{e:?}"));
                VmBamlError::Io {
                    message: format!("Failed to call env function: {msg}"),
                }
            });
        let result = match result {
            Ok(result) => result,
            Err(e) => return SysOpOutput::err(e),
        };
        if let Some(host_call_id) = &host_call_id
            && let Some(patch) =
                self.run_store
                    .ingest_env_requested(host_call_id, request_id, key.clone())
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
                        message: format!("Env callback promise rejected: {msg}"),
                    }
                })?;
                let value = js_to_env_value(&result).map_err(VmRustFnError::from)?;
                publish_env_resolved(
                    &run_store,
                    &notification_callback,
                    host_call_id.as_ref(),
                    request_id,
                    key,
                    value.as_ref(),
                );
                Ok(value)
            }));
        }

        match js_to_env_value(&result) {
            Ok(v) => {
                publish_env_resolved(
                    &self.run_store,
                    &self.notification_callback,
                    host_call_id.as_ref(),
                    request_id,
                    key,
                    v.as_ref(),
                );
                SysOpOutput::ok(v)
            }
            Err(e) => SysOpOutput::err(e),
        }
    }
}

fn publish_env_resolved(
    run_store: &InMemoryRunStore,
    notification_callback: &SendWrapper<Function>,
    host_call_id: Option<&HostCallId>,
    request_id: u64,
    key: String,
    value: Option<&String>,
) {
    let Some(host_call_id) = host_call_id else {
        return;
    };
    let status = env_resolution_status(value);
    if let Some(boundary_id) = run_store.boundary_id_for_host_call(host_call_id) {
        let result = run_store.resolve_env_request_for_run(boundary_id, request_id, status, None);
        if result.outcome == RequestCommandOutcome::Accepted
            && let Some(patch) = result.patch
        {
            crate::runs::send_run_patch(notification_callback, &patch);
        }
        return;
    }

    if let Some(patch) = run_store.ingest_env_resolved(host_call_id, request_id, key, status, None)
    {
        crate::runs::send_run_patch(notification_callback, &patch);
    }
}

fn env_resolution_status(value: Option<&String>) -> EnvResolutionStatus {
    if value.is_some() {
        EnvResolutionStatus::ResolvedFromOverride
    } else {
        EnvResolutionStatus::DeclinedMissing
    }
}
