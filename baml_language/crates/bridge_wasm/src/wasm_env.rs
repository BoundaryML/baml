//! WASM env implementation via JS callback.
//!
//! `WasmEnv` holds the JS `env_vars` function and implements the env `sys_ops`.
//! The JS callback returns a `Promise<string | undefined>`, allowing the
//! host page to show an interactive prompt and resolve when the user submits.

use std::sync::Arc;

use js_sys::{Function, Promise};
use sys_ops::io::IoNamespaceEnv;
use sys_types::{BexHeap, CallId, OpErrorKind, SysOpContext, SysOpOutput};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::send_wrapper::{SendFuture, SendWrapper};

/// WASM env implementation that holds the JS `env_vars` callback.
///
/// Signature of the JS function: `(var: string) => Promise<string | undefined>`
pub(crate) struct WasmEnv {
    /// The JS function to call for env lookups.
    env_fn: SendWrapper<Function>,
}

impl WasmEnv {
    pub(crate) fn new(env_fn: Function) -> Self {
        Self {
            env_fn: SendWrapper::new(env_fn),
        }
    }

    fn env_fn(&self) -> &Function {
        self.env_fn.inner()
    }
}

fn js_to_env_value(value: &wasm_bindgen::JsValue) -> Result<Option<String>, OpErrorKind> {
    if value.is_undefined() || value.is_null() {
        return Ok(None);
    }
    let s = value.as_string().ok_or_else(|| {
        OpErrorKind::Other("Env function did not return a string or undefined".into())
    })?;
    Ok(Some(s))
}

impl IoNamespaceEnv for WasmEnv {
    fn get(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        key: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        let env_fn = self.env_fn().clone();
        let result = env_fn
            .call1(&wasm_bindgen::JsValue::NULL, &key.into())
            .map_err(|e| {
                let msg = e.as_string().unwrap_or_else(|| format!("{e:?}"));
                OpErrorKind::Other(format!("Failed to call env function: {msg}"))
            });
        let result = match result {
            Ok(result) => result,
            Err(e) => return SysOpOutput::err(e),
        };

        if result.is_instance_of::<Promise>() {
            let promise: Promise = result.unchecked_into();
            return SysOpOutput::Async(Box::pin(SendFuture(async move {
                let result = JsFuture::from(promise).await.map_err(|e| {
                    let msg = e.as_string().unwrap_or_else(|| format!("{e:?}"));
                    OpErrorKind::Other(format!("Env callback promise rejected: {msg}"))
                })?;
                js_to_env_value(&result)
            })));
        }

        match js_to_env_value(&result) {
            Ok(v) => SysOpOutput::ok(v),
            Err(e) => SysOpOutput::err(e),
        }
    }
}
