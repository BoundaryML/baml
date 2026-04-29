use std::sync::Arc;

use sys_ops::io::{self, IoNamespaceSys};
use sys_types::{BexHeap, CallId, OpErrorKind, SysOpContext, SysOpOutput};
use wasm_bindgen::prelude::*;

use crate::send_wrapper::SendFuture;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = "setTimeout")]
    fn set_timeout(closure: &js_sys::Function, millis: i32) -> i32;
}

pub(crate) struct WasmSys;

impl WasmSys {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl IoNamespaceSys for WasmSys {
    fn shell(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _command: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::sys::ShellOutput> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }

    fn sleep(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        delay_ms: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        let millis = i32::try_from(delay_ms.clamp(0, i64::from(i32::MAX))).unwrap_or(i32::MAX);
        let promise = js_sys::Promise::new(&mut |resolve, _reject| {
            set_timeout(&resolve, millis);
        });
        SysOpOutput::async_op(SendFuture(async move {
            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
            Ok(())
        }))
    }
}
