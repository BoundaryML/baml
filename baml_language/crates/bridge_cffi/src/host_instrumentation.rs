//! Shared host instrumentation bindings. No host body is executed here.
use std::sync::{Arc, Mutex};

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

fn inherited_state(
    owner: &Arc<dyn bex_project::Bex>,
    inherited_key: u64,
) -> Result<Option<bex_project::InheritedInvocationState>, BridgeError> {
    let Some(inherited_key) = TableKey::new(inherited_key) else {
        return Ok(None);
    };
    let entry = HANDLE_TABLE
        .resolve(inherited_key)
        .ok_or_else(|| invalid("host context is no longer live"))?;
    let CffiHandleTableEntry::InvocationState(state) = &*entry else {
        return Err(invalid("expected an invocation-state handle"));
    };
    if !Arc::ptr_eq(owner, &state.owner) {
        return Err(invalid("host context belongs to a different runtime"));
    }
    Ok(Some(state.state.clone()))
}

fn insert_state(owner: Arc<dyn bex_project::Bex>, execution: &HostInvocation) -> u64 {
    HANDLE_TABLE
        .insert(CffiHandleTableEntry::InvocationState(
            InvocationStateHandle {
                owner,
                state: execution.inherited_state(),
            },
        ))
        .get()
}

pub fn begin(
    definition: &HostDefinition,
    inherited_key: u64,
    trace_options: &TraceOptionsData,
    caller: &bex_project::HostCallSite,
    inputs: Option<&bex_project::HostCapture>,
) -> Result<(HostInvocation, u64), BridgeError> {
    let owner = crate::get_runtime()?;
    let inherited = inherited_state(&owner, inherited_key)?;
    let execution = Arc::clone(&owner).begin_host_invocation(
        definition,
        inherited.as_ref(),
        trace_options,
        caller,
        inputs,
    )?;
    let key = insert_state(owner, &execution);
    Ok((execution, key))
}

/// A host span the host closes through `finish_span`. Releasing its handle
/// instead abandons the recording.
struct HostSpan(Mutex<Option<HostInvocation>>);

impl bex_project::BexRustData for HostSpan {
    fn measure(&self, _meter: &mut bex_project::Meter) {}
}

/// Begin a span from a JSON request with `module`, `name`, `file`, `line`,
/// `inherited`, `distinct_id`, `metadata` and `inputs` (a tagged capture).
/// Returns the span's handle and the invocation-state handle work inside it
/// inherits; the caller owns both.
pub fn begin_span(request: &[u8]) -> Result<(u64, u64), BridgeError> {
    let request: serde_json::Value =
        serde_json::from_slice(request).map_err(|_| invalid("malformed host span request"))?;
    let text = |name: &str| {
        request[name]
            .as_str()
            .ok_or_else(|| invalid("host span request is missing a field"))
    };
    let definition = HostDefinition {
        language: "rust".into(),
        module: text("module")?.into(),
        qualified_name: text("name")?.into(),
        source_file: text("file")?.into(),
        definition_line: request["line"]
            .as_u64()
            .and_then(|line| u32::try_from(line).ok())
            .unwrap_or(0),
        wrapper_line: 0,
        display_name: text("name")?.into(),
    };
    let options = TraceOptionsData {
        inputs: Some(true),
        output: Some(true),
        error: Some(true),
        context: Some(Arc::new(context_patch(&request))),
        ..Default::default()
    };
    let inputs = request["inputs"].as_str().map(crate::host_capture::decode);
    let owner = crate::get_runtime()?;
    let inherited = inherited_state(&owner, request["inherited"].as_u64().unwrap_or(0))?;
    let execution = Arc::clone(&owner).begin_detached_host_invocation(
        &definition,
        inherited.as_ref(),
        &options,
        &bex_project::HostCallSite {
            source_file: String::new(),
            line: 0,
        },
        inputs.as_ref(),
    )?;
    let state = insert_state(owner, &execution);
    let span = HANDLE_TABLE.insert(
        CffiHandleTableEntry::try_from(bex_project::BexExternalValue::RustData(Arc::new(
            HostSpan(Mutex::new(Some(execution))),
        )))
        .expect("RustData is a handle-table value"),
    );
    Ok((span.get(), state))
}

fn context_patch(request: &serde_json::Value) -> btel_types::context::ContextPatch {
    use btel_types::context::{ContextPatch, ContextValue};
    let value = |value: &serde_json::Value| match value {
        serde_json::Value::String(value) => Some(ContextValue::String(value.clone())),
        serde_json::Value::Bool(value) => Some(ContextValue::Bool(*value)),
        serde_json::Value::Number(number) => number
            .as_i64()
            .map(ContextValue::Int)
            .or_else(|| number.as_f64().map(ContextValue::Float)),
        _ => None,
    };
    ContextPatch {
        distinct_id: request["distinct_id"].as_str().map(str::to_owned),
        metadata: request["metadata"]
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(key, entry)| Some((key.clone(), Some(value(entry)?))))
            .collect(),
    }
}

/// How a host span ended, as the C ABI spells it. `Closed` makes no claim.
#[derive(Clone, Copy)]
pub enum HostSpanEnd {
    Closed,
    Ok,
    Errored,
    Cancelled,
    Panicked,
}

impl TryFrom<u32> for HostSpanEnd {
    type Error = BridgeError;

    fn try_from(end: u32) -> Result<Self, BridgeError> {
        match end {
            0 => Ok(Self::Closed),
            1 => Ok(Self::Ok),
            2 => Ok(Self::Errored),
            3 => Ok(Self::Cancelled),
            4 => Ok(Self::Panicked),
            _ => Err(invalid("invalid host span outcome")),
        }
    }
}

/// End a span once with its tagged capture value, if any. The caller still
/// owns the handle.
pub fn finish_span(key: u64, end: HostSpanEnd, value: Option<&str>) -> Result<(), BridgeError> {
    use btel_types::InvocationOutcome;
    let entry = TableKey::new(key)
        .and_then(|key| HANDLE_TABLE.resolve(key))
        .ok_or_else(|| invalid("host span is no longer live"))?;
    let CffiHandleTableEntry::RustData(data) = &*entry else {
        return Err(invalid("expected a host span"));
    };
    let span = data
        .downcast_ref::<HostSpan>()
        .ok_or_else(|| invalid("expected a host span"))?;
    let execution = span
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    if let Some(execution) = execution {
        let value = value.map(crate::host_capture::decode);
        let outcome = match end {
            HostSpanEnd::Closed => {
                execution.close();
                return Ok(());
            }
            HostSpanEnd::Ok => InvocationOutcome::Ok,
            HostSpanEnd::Errored => InvocationOutcome::Errored,
            HostSpanEnd::Cancelled => InvocationOutcome::Cancelled,
            HostSpanEnd::Panicked => InvocationOutcome::Panicked,
        };
        execution.finish_with_value(outcome, value.as_ref());
    }
    Ok(())
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
