//! WASM env implementation via JS callback.
//!
//! `WasmEnv` holds the JS `env_vars` function and implements the env `sys_ops`.
//! The JS callback returns a `Promise<string | undefined>`, allowing the
//! host page to show an interactive prompt and resolve when the user submits.

use js_sys::{Function, Promise};
use sys_types::{CallId, OpErrorKind, SysOpEnv, SysOpOutput};
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

impl SysOpEnv for WasmEnv {
    fn env_get(&self, _call_id: CallId, key: String) -> SysOpOutput<Option<String>> {
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

        // The callback may return a plain value or a Promise.
        let value = if result.is_instance_of::<Promise>() {
            let promise: Promise = result.unchecked_into();
            return SysOpOutput::Async(Box::pin(SendFuture(async move {
                let result = JsFuture::from(promise).await.map_err(|e| {
                    let msg = e.as_string().unwrap_or_else(|| format!("{e:?}"));
                    OpErrorKind::Other(format!("Env callback promise rejected: {msg}"))
                })?;
                if result.is_undefined() || result.is_null() {
                    return Ok(None);
                }
                let s = result.as_string().ok_or_else(|| {
                    OpErrorKind::Other("Env function did not return a string or undefined".into())
                });
                match s {
                    Ok(s) => Ok(Some(s)),
                    Err(e) => Err(e),
                }
            })));
        } else {
            result
        };

        if value.is_undefined() || value.is_null() {
            return SysOpOutput::ok(None);
        }
        let s = value.as_string().ok_or_else(|| {
            OpErrorKind::Other("Env function did not return a string or undefined".into())
        });
        match s {
            Ok(s) => SysOpOutput::ok(Some(s)),
            Err(e) => SysOpOutput::err(e),
        }
    }

    fn env_get_or_panic(&self, _call_id: CallId, key: String) -> SysOpOutput<String> {
        let env_fn = self.env_fn().clone();
        let key_for_err = key.clone();
        SysOpOutput::Async(Box::pin(SendFuture(async move {
            let result = env_fn
                .call1(&wasm_bindgen::JsValue::NULL, &key.into())
                .map_err(|e| {
                    let msg = e.as_string().unwrap_or_else(|| format!("{e:?}"));
                    OpErrorKind::Other(format!("Failed to call env function: {msg}"))
                })?;

            let value = if result.is_instance_of::<Promise>() {
                let promise: Promise = result.unchecked_into();
                JsFuture::from(promise).await.map_err(|e| {
                    let msg = e.as_string().unwrap_or_else(|| format!("{e:?}"));
                    OpErrorKind::Other(format!("Env callback promise rejected: {msg}"))
                })?
            } else {
                result
            };

            if value.is_undefined() || value.is_null() {
                return Err(OpErrorKind::Other(format!(
                    "Environment variable '{key_for_err}' not found",
                )));
            }
            value.as_string().ok_or_else(|| {
                OpErrorKind::Other("Env function did not return a string or undefined".into())
            })
        })))
    }
}
