//! WASM io implementation via JS callback.
//!
//! `WasmIo` holds the JS `input` function and implements the io `sys_ops`.
//! The JS callback receives an optional prompt string and returns a string
//! (or `Promise<string>`).

use std::sync::Arc;

use js_sys::{Function, Promise};
use sys_ops::io::IoNamespaceIo;
use sys_types::{BexHeap, CallId, OpErrorKind, SysOpContext, SysOpOutput};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::send_wrapper::{SendFuture, SendWrapper};

/// WASM io implementation that holds the JS `input` callback.
///
/// Signature of the JS function: `(callId: number, prompt: string | undefined) => Promise<string> | string`
pub(crate) struct WasmIo {
    input_fn: SendWrapper<Function>,
}

impl WasmIo {
    pub(crate) fn new(input_fn: Function) -> Self {
        Self {
            input_fn: SendWrapper::new(input_fn),
        }
    }

    fn input_fn(&self) -> &Function {
        self.input_fn.inner()
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
        #[allow(clippy::cast_precision_loss)] // call IDs are small sequential integers
        let js_call_id = wasm_bindgen::JsValue::from_f64(call_id.0 as f64);
        let js_prompt = match prompt {
            Some(p) => wasm_bindgen::JsValue::from_str(&p),
            None => wasm_bindgen::JsValue::UNDEFINED,
        };
        let result = input_fn
            .call2(&wasm_bindgen::JsValue::NULL, &js_call_id, &js_prompt)
            .map_err(|e| {
                let msg = e.as_string().unwrap_or_else(|| format!("{e:?}"));
                OpErrorKind::Other(format!("Failed to call input function: {msg}"))
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
                    OpErrorKind::Other(format!("Input callback promise rejected: {msg}"))
                })?;
                result.as_string().ok_or_else(|| {
                    OpErrorKind::Other("Input callback did not return a string".into())
                })
            })));
        }

        match result.as_string() {
            Some(s) => SysOpOutput::ok(s),
            None => SysOpOutput::err(OpErrorKind::Other(
                "Input callback did not return a string".into(),
            )),
        }
    }

    fn print(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
    fn println(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
    fn eprint(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
    fn eprintln(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
}
