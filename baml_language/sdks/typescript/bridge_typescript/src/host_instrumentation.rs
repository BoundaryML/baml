//! Node's host executor owns the body; this binding owns native recording.
use std::sync::Mutex;

use bridge_ctypes::baml_bridge::cffi::BamlHandleType;
use btel_snapshot::host::HostValue;
use napi::bindgen_prelude::Buffer;
use napi_derive::napi;
use prost::Message;

use crate::{errors::bridge_error_to_napi, handle::BamlHandle};

fn host_error(error: bridge_cffi::BridgeError) -> napi::Error {
    match error {
        bridge_cffi::BridgeError::InvocationProtocol(message) => {
            napi::Error::new(napi::Status::InvalidArg, message)
        }
        other => bridge_error_to_napi(other),
    }
}

#[napi(object)]
pub struct HostDefinition {
    pub module: String,
    pub qualified_name: String,
    pub source_file: String,
    pub definition_line: u32,
    pub wrapper_line: u32,
    pub display_name: String,
}

#[napi(object)]
pub struct HostCallSite {
    pub source_file: String,
    pub line: u32,
}

#[napi(js_name = "_HostExecution")]
pub struct HostExecution(Mutex<Option<bex_project::HostInvocation>>);

#[napi]
impl HostExecution {
    #[napi]
    pub fn abandon(&self) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
    }

    #[napi]
    pub fn finish(&self, outcome: String, value: Option<String>) -> napi::Result<()> {
        let outcome = match outcome.as_str() {
            "ok" => btel_types::InvocationOutcome::Ok,
            "error" => btel_types::InvocationOutcome::Errored,
            "cancelled" => btel_types::InvocationOutcome::Cancelled,
            _ => return Err(napi::Error::from_reason("invalid host outcome")),
        };
        if let Some(execution) = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let captured = value
                .filter(|_| execution.wants_value(outcome))
                .map(|value| capture(&value));
            execution.finish_with_value(outcome, captured.as_ref());
        }
        Ok(())
    }
}

#[napi(js_name = "_validateHostOptions")]
pub fn validate_host_options(options: Option<&BamlHandle>) -> napi::Result<Vec<bool>> {
    bridge_cffi::host_instrumentation::options(options.map_or(0, BamlHandle::key_u64))
        .map(|options| {
            vec![
                options.inputs == Some(true),
                options.output == Some(true),
                options.error == Some(true),
            ]
        })
        .map_err(host_error)
}

#[napi(js_name = "_beginHostInvocation")]
pub fn begin_host_invocation(
    definition: HostDefinition,
    inherited: Option<&BamlHandle>,
    options: Option<&BamlHandle>,
    caller: HostCallSite,
    inputs: Option<String>,
) -> napi::Result<(HostExecution, BamlHandle, Buffer)> {
    let options =
        bridge_cffi::host_instrumentation::options(options.map_or(0, BamlHandle::key_u64))
            .map_err(host_error)?;
    let inputs = inputs
        .filter(|_| options.inputs == Some(true))
        .map(|value| capture(&value));
    let (execution, key) = bridge_cffi::host_instrumentation::begin(
        &bex_project::HostDefinition {
            language: "typescript".into(),
            module: definition.module,
            qualified_name: definition.qualified_name,
            source_file: definition.source_file,
            definition_line: definition.definition_line,
            wrapper_line: definition.wrapper_line,
            display_name: definition.display_name,
        },
        inherited.map_or(0, BamlHandle::key_u64),
        &options,
        &bex_project::HostCallSite {
            source_file: caller.source_file,
            line: caller.line,
        },
        inputs.as_ref(),
    )
    .map_err(host_error)?;
    let state = BamlHandle::from_parts(key, BamlHandleType::InvocationState as i32);
    let cancel = bridge_ctypes::external_to_outbound(
        &execution.inherited_state().cancellation_projection(),
        &bridge_ctypes::CffiHandleTableOptions::for_wire(),
    )
    .map_err(|error| napi::Error::from_reason(error.to_string()))?
    .encode_to_vec();
    Ok((
        HostExecution(Mutex::new(Some(execution))),
        state,
        cancel.into(),
    ))
}

// The adapter supplies a bounded tagged tree of copied data, never a JS object
// that native conversion would inspect through getters or serializers.
pub(crate) fn capture(wire: &str) -> HostValue {
    if wire.len() > btel_snapshot::host::MAX_BYTES * 8 {
        return HostValue::Truncated(btel_snapshot::Limit::Bytes);
    }
    serde_json::from_str::<serde_json::Value>(wire).map_or(HostValue::Unavailable, |value| {
        let mut remaining = btel_snapshot::host::MAX_VALUES;
        copy(&value, 0, &mut remaining)
    })
}

fn copy(value: &serde_json::Value, depth: usize, remaining: &mut usize) -> HostValue {
    if depth > btel_snapshot::host::MAX_DEPTH {
        return HostValue::Truncated(btel_snapshot::Limit::Depth);
    }
    if *remaining == 0 {
        return HostValue::Truncated(btel_snapshot::Limit::Values);
    }
    *remaining -= 1;
    let Some(items) = value.as_array() else {
        return HostValue::Unavailable;
    };
    let Some(tag) = items.first().and_then(serde_json::Value::as_str) else {
        return HostValue::Unavailable;
    };
    let payload = items.get(1).unwrap_or(&serde_json::Value::Null);
    match tag {
        "null" => HostValue::Null,
        "bool" => payload
            .as_bool()
            .map_or(HostValue::Unavailable, HostValue::Bool),
        "number" => payload.as_i64().map_or_else(
            || {
                payload
                    .as_f64()
                    .map_or(HostValue::Unavailable, HostValue::Float)
            },
            HostValue::Int,
        ),
        "string" => payload
            .as_str()
            .map_or(HostValue::Unavailable, |v| HostValue::String(v.into())),
        "list" => payload.as_array().map_or(HostValue::Unavailable, |values| {
            let mut copied = Vec::new();
            for value in values {
                if *remaining == 0 {
                    return HostValue::Truncated(btel_snapshot::Limit::Values);
                }
                copied.push(copy(value, depth + 1, remaining));
            }
            HostValue::List(copied)
        }),
        "map" => payload
            .as_array()
            .map_or(HostValue::Unavailable, |entries| {
                let mut copied = Vec::new();
                for entry in entries {
                    if *remaining == 0 {
                        return HostValue::Truncated(btel_snapshot::Limit::Values);
                    }
                    let Some(pair) = entry.as_array() else {
                        return HostValue::Unavailable;
                    };
                    let Some(key) = pair.first().and_then(serde_json::Value::as_str) else {
                        return HostValue::Unavailable;
                    };
                    let Some(value) = pair.get(1) else {
                        return HostValue::Unavailable;
                    };
                    copied.push((key.into(), copy(value, depth + 1, remaining)));
                }
                HostValue::Map(copied)
            }),
        "depth" => HostValue::Truncated(btel_snapshot::Limit::Depth),
        "values" => HostValue::Truncated(btel_snapshot::Limit::Values),
        "bytes" => HostValue::Truncated(btel_snapshot::Limit::Bytes),
        _ => HostValue::Unavailable,
    }
}

#[napi(js_name = "_defineHostMarker")]
pub fn define_host_marker(
    definition: HostDefinition,
    options: Option<&BamlHandle>,
) -> napi::Result<BamlHandle> {
    let key = bridge_cffi::host_instrumentation::define_marker(
        bex_project::HostDefinition {
            language: "typescript".into(),
            module: definition.module,
            qualified_name: definition.qualified_name,
            source_file: definition.source_file,
            definition_line: definition.definition_line,
            wrapper_line: definition.wrapper_line,
            display_name: definition.display_name,
        },
        options.map_or(0, BamlHandle::key_u64),
    )
    .map_err(host_error)?;
    Ok(BamlHandle::from_parts(
        key,
        BamlHandleType::UntaggedRustData as i32,
    ))
}
