//! Native packs provide generated host adapters rather than the legacy bridge.
use crate::NativeSysOps;
use bex_heap::BexHeap;
use std::sync::Arc;
use sys_ops::io::{self, BexExternalValue, CallId, SysOpContext, SysOpOutput};
use sys_types::{SapTy, VmInternalError};
impl io::IoNamespaceHost for NativeSysOps {
    fn call_host_value(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _handle: BexExternalValue,
        _args: Vec<BexExternalValue>,
        _return_type: SapTy,
        _throws_type: SapTy,
        _context: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        SysOpOutput::err(VmInternalError::BridgeFailure {
            message:
                "legacy host bridge is not linked; native calls require a generated host adapter"
                    .into(),
        })
    }
}
