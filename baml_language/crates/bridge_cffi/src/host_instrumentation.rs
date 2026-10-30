//! Shared host instrumentation bindings. No host body is executed here.
use std::sync::Arc;

use bex_project::{HostDefinition, HostInvocation, RustDataArc as _, TraceOptionsData};
use bridge_ctypes::{CffiHandleTableEntry, HANDLE_TABLE, InvocationStateHandle, TableKey};

use crate::BridgeError;

fn invalid(message: &str) -> BridgeError {
    BridgeError::InvocationProtocol(message.into())
}

/// Validate marker configuration without capturing a runtime or ambient parent.
pub fn options(key: u64) -> Result<TraceOptionsData, BridgeError> {
    let Some(key) = TableKey::new(key) else {
        // Bare Python/Node instrumentation uses the same capture defaults as span().
        return Ok(TraceOptionsData {
            output: Some(true),
            error: Some(true),
            ..Default::default()
        });
    };
    let entry = HANDLE_TABLE
        .resolve(key)
        .ok_or_else(|| invalid("host trace options are no longer live"))?;
    let CffiHandleTableEntry::RustData(data) = &*entry else {
        return Err(invalid("instrument accepts generated trace.Options only"));
    };
    data.downcast_ref::<TraceOptionsData>()
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
    let inherited = match TableKey::new(inherited_key) {
        None => None,
        Some(inherited_key) => {
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
        }
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
    Ok((execution, key.get()))
}

/// A marker retains immutable declaration data only, not a runtime or parent.
pub fn define_marker(definition: HostDefinition, options_key: u64) -> Result<u64, BridgeError> {
    let marker = bex_project::HostMarker {
        definition,
        options: options(options_key)?,
    };
    Ok(HANDLE_TABLE
        .insert(
            CffiHandleTableEntry::try_from(bex_project::BexExternalValue::RustData(Arc::new(
                marker,
            )))
            .expect("RustData is a handle-table value"),
        )
        .get())
}

pub fn registration_marker(
    key: u64,
    language: &str,
) -> Result<Arc<bex_project::HostMarker>, BridgeError> {
    let Some(key) = TableKey::new(key) else {
        return Ok(Arc::new(bex_project::HostMarker {
            definition: HostDefinition {
                language: language.into(),
                module: "<host>".into(),
                qualified_name: "<unmarked-callback>".into(),
                source_file: "<native>".into(),
                definition_line: 0,
                wrapper_line: 0,
                display_name: "<callback>".into(),
            },
            options: TraceOptionsData::default(),
        }));
    };
    let entry = HANDLE_TABLE
        .resolve(key)
        .ok_or_else(|| invalid("host marker is no longer live"))?;
    let CffiHandleTableEntry::RustData(data) = &*entry else {
        return Err(invalid("expected a host marker"));
    };
    Arc::clone(data)
        .downcast_payload::<bex_project::HostMarker>()
        .map_err(|_| invalid("expected a host marker"))
}

pub fn callback(call_id: u32) -> Option<Arc<bex_project::CallbackHostInvocation>> {
    let capture = sys_native::host_dispatch::execution_capture(call_id)?;
    capture
        .downcast_payload::<bex_project::InvocationCapture>()
        .ok()?
        .host
        .clone()
}

/// The `baml.spawn.CancelToken` a host body holds, encoded as the outbound
/// value a host decodes: a live reference to an engine instance observing the
/// execution's effective cancellation.
pub async fn encode_cancel_token(execution: &HostInvocation) -> Result<Vec<u8>, BridgeError> {
    use prost::Message as _;

    let token = execution
        .cancel_token()
        .await
        .map_err(bex_project::RuntimeError::from)?;
    Ok(bridge_ctypes::external_to_outbound(
        &token,
        &bridge_ctypes::CffiHandleTableOptions::for_wire(),
    )?
    .encode_to_vec())
}

/// [`encode_cancel_token`] for a host thread that drives no async runtime of
/// its own (a host body's own thread). Never call it from a runtime worker.
#[cfg(not(target_arch = "wasm32"))]
pub fn cancel_token(execution: &HostInvocation) -> Result<Vec<u8>, BridgeError> {
    crate::get_tokio_runtime()?.block_on(encode_cancel_token(execution))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_instrumentation_requests_outputs_and_errors_but_not_inputs() {
        let resolved = options(0).unwrap();
        assert_eq!(resolved.inputs, None);
        assert_eq!(resolved.output, Some(true));
        assert_eq!(resolved.error, Some(true));
    }

    #[test]
    fn explicit_empty_span_keeps_capture_disabled() {
        let key = HANDLE_TABLE.insert(CffiHandleTableEntry::RustData(Arc::new(TraceOptionsData {
            inputs: Some(false),
            output: Some(false),
            error: Some(false),
            ..Default::default()
        })));
        let resolved = options(key.get()).unwrap();
        assert_eq!(resolved.inputs, Some(false));
        assert_eq!(resolved.output, Some(false));
        assert_eq!(resolved.error, Some(false));
        assert!(HANDLE_TABLE.release(key));
    }
}
