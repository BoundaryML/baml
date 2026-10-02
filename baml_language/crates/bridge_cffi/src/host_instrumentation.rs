//! Shared host instrumentation bindings. No host body is executed here.
use std::sync::Arc;

use bex_project::{HostDefinition, HostInvocation, TraceOptionsData};
use bridge_ctypes::{CffiHandleTableEntry, HANDLE_TABLE, InvocationStateHandle};

use crate::BridgeError;

fn invalid(message: &str) -> BridgeError {
    BridgeError::InvocationProtocol(message.into())
}

/// Validate marker configuration without capturing a runtime or ambient parent.
pub fn options(key: u64) -> Result<TraceOptionsData, BridgeError> {
    if key == 0 {
        return Ok(TraceOptionsData::default());
    }
    let entry = HANDLE_TABLE
        .resolve(key)
        .ok_or_else(|| invalid("host trace options are no longer live"))?;
    let CffiHandleTableEntry::RustData(data) = &*entry else {
        return Err(invalid("instrument accepts generated trace.Options only"));
    };
    data.0
        .downcast_ref::<TraceOptionsData>()
        .cloned()
        .ok_or_else(|| {
            invalid("instrument accepts generated trace.Options only; reservations are unsupported")
        })
}

pub fn begin(
    definition: &HostDefinition,
    inherited_key: u64,
    trace_options: &TraceOptionsData,
    caller: &bex_project::HostCallSite,
    inputs: Option<&bex_project::HostCapture>,
) -> Result<(HostInvocation, u64), BridgeError> {
    let owner = crate::get_runtime()?;
    let inherited = if inherited_key == 0 {
        None
    } else {
        let entry = HANDLE_TABLE
            .resolve(inherited_key)
            .ok_or_else(|| invalid("host context is no longer live"))?;
        let CffiHandleTableEntry::InvocationState(state) = &*entry else {
            return Err(invalid("expected an invocation-state handle"));
        };
        if !Arc::ptr_eq(&owner, &state.owner) {
            return Err(invalid("host context belongs to a different runtime"));
        }
        Some(state.state.clone())
    };
    let execution = Arc::clone(&owner).begin_host_invocation(
        definition,
        inherited.as_ref(),
        trace_options,
        caller,
        inputs,
    )?;
    let key = HANDLE_TABLE.insert(CffiHandleTableEntry::InvocationState(
        InvocationStateHandle {
            owner,
            state: execution.inherited_state(),
        },
    ));
    Ok((execution, key))
}
