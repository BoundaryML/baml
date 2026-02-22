use sys_types::{CallId, OpErrorKind, SysOpOutput, SysOpSys};

pub(crate) struct WasmSys;

impl WasmSys {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl SysOpSys for WasmSys {
    fn baml_sys_panic(&self, _call_id: CallId, message: String) -> SysOpOutput<()> {
        SysOpOutput::err(OpErrorKind::Other(message))
    }

    fn baml_sys_sleep(&self, _call_id: CallId, delay_ms: i64) -> SysOpOutput<()> {
        SysOpOutput::async_op(async move {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms.cast_unsigned())).await;
            Ok(())
        })
    }
}
