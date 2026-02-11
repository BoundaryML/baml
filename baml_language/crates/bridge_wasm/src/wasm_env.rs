//! WASM env implementation via JS callback.
//!
//! `WasmEnv` holds the JS `env_vars` function and implements the env `sys_ops`.
//! Each `BamlWasmRuntime` can get its own `WasmEnv` instance.

use js_sys::Function;
use sys_types::{OpErrorKind, SysOpEnv, SysOpOutput};

use crate::send_wrapper::SendWrapper;

/// WASM env implementation that holds the JS `env_vars` callback.
///
/// Signature of the JS function: `(var: string) => string | undefined`
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

    /// Call the JS callback with the given key; returns Some(s) or None.
    fn get(&self, key: &str) -> Result<Option<String>, OpErrorKind> {
        let result = self
            .env_fn()
            .call1(&wasm_bindgen::JsValue::NULL, &key.into())
            .map_err(|e| {
                let msg = e.as_string().unwrap_or_else(|| format!("{e:?}"));
                OpErrorKind::Other(format!("Failed to call env function: {msg}"))
            })?;
        if result.is_undefined() || result.is_null() {
            return Ok(None);
        }
        let s = result.as_string().ok_or_else(|| {
            OpErrorKind::Other("Env function did not return a string or undefined".into())
        })?;
        Ok(Some(s))
    }
}

impl SysOpEnv for WasmEnv {
    fn env_get(&self, key: String) -> SysOpOutput<Option<String>> {
        match self.get(&key) {
            Ok(v) => SysOpOutput::ok(v),
            Err(e) => SysOpOutput::err(e),
        }
    }

    fn env_get_or_panic(&self, key: String) -> SysOpOutput<String> {
        match self.get(&key) {
            Ok(Some(v)) => SysOpOutput::ok(v),
            Ok(None) => SysOpOutput::err(OpErrorKind::Other(format!(
                "Environment variable '{key}' not found",
            ))),
            Err(e) => SysOpOutput::err(e),
        }
    }
}
