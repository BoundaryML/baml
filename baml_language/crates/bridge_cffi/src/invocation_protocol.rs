//! Invocation-control validation, independent of runtime admission.
//!
//! Protobuf is a breaking internal contract. Native function-table layout
//! compatibility is protected by the ABI revision, not protobuf negotiation.

use prost::{
    Message,
    encoding::{DecodeContext, WireType, decode_key, decode_varint, skip_field},
};

use crate::{
    BridgeError,
    baml_bridge::cffi::{
        CallFunctionArgs, HostInvocation, TraceMode, trace_metadata_value, trace_selection,
    },
};

fn invalid(message: impl Into<String>) -> BridgeError {
    BridgeError::InvocationProtocol(message.into())
}

/// Decode a canonical invocation envelope. Runtime ownership and concrete token types
/// are resolved separately during preparation, never from a host type name.
pub fn decode_call(bytes: &[u8]) -> Result<CallFunctionArgs, BridgeError> {
    validate_fields(bytes, ControlMessage::Call)?;
    let call = CallFunctionArgs::decode(bytes).map_err(bridge_ctypes::CtypesError::from)?;
    if call.call_id == 0 {
        return Err(BridgeError::InvalidCallId);
    }
    if call.call_target.is_none() {
        return Err(BridgeError::MissingCallTarget);
    }
    let options = call
        .invocation
        .as_ref()
        .ok_or_else(|| invalid("invocation is required; legacy envelopes are unsupported"))?;
    if options
        .cancel
        .as_ref()
        .is_some_and(|cancel| cancel.value.is_none())
    {
        return Err(invalid(
            "cancel must be absent or an actual generated CancelToken; explicit null is malformed",
        ));
    }
    if let Some(trace) = &options.trace {
        match trace
            .selection
            .as_ref()
            .ok_or_else(|| invalid("trace selection is required"))?
        {
            trace_selection::Selection::Reservation(0) => {
                return Err(invalid("reservation must identify a live typed handle"));
            }
            trace_selection::Selection::Reservation(_) => {}
            trace_selection::Selection::Options(options) => {
                if let Some(mode) = options.mode {
                    match TraceMode::try_from(mode) {
                        Ok(TraceMode::Hidden | TraceMode::Timing | TraceMode::Span) => {}
                        _ => return Err(invalid("trace mode must be Hidden, Timing or Span")),
                    }
                }
                for metadata in options.metadata.values() {
                    match metadata.value.as_ref() {
                        None | Some(trace_metadata_value::Value::Remove(false)) => {
                            return Err(invalid(
                                "trace metadata needs a primitive value or a true removal marker",
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    Ok(call)
}

pub fn decode_host_invocation(bytes: &[u8]) -> Result<HostInvocation, BridgeError> {
    validate_fields(bytes, ControlMessage::Host)?;
    let call = HostInvocation::decode(bytes).map_err(bridge_ctypes::CtypesError::from)?;
    if call.host_value_key == 0 || call.callback_id == 0 || call.effective_state == 0 {
        return Err(invalid(
            "host invocation requires nonzero callable, callback and effective-state identities",
        ));
    }
    if call
        .cancel
        .as_ref()
        .is_none_or(|cancel| cancel.value.is_none())
    {
        return Err(invalid(
            "host invocation requires the generated effective CancelToken projection",
        ));
    }
    Ok(call)
}

#[derive(Clone, Copy)]
enum ControlMessage {
    Call,
    Options,
    Selection,
    Trace,
    MetadataEntry,
    MetadataValue,
    Host,
}

// Protobuf unknown-field skipping is insufficient for invocation controls.
// Walk the raw control messages before prost discards unknown fields. Ordinary
// application values retain their existing value codec and type validation.
fn validate_fields(mut bytes: &[u8], message: ControlMessage) -> Result<(), BridgeError> {
    while !bytes.is_empty() {
        let (tag, wire) = decode_key(&mut bytes).map_err(bridge_ctypes::CtypesError::from)?;
        let max_tag = match message {
            ControlMessage::Call | ControlMessage::Trace => 6,
            ControlMessage::Options => 5,
            ControlMessage::Selection | ControlMessage::MetadataEntry => 2,
            ControlMessage::MetadataValue => 5,
            ControlMessage::Host => 7,
        };
        if tag == 0 || tag > max_tag {
            return Err(invalid(format!("unknown invocation control field {tag}")));
        }
        let child = match (message, tag) {
            (ControlMessage::Call, 6) => Some(ControlMessage::Options),
            (ControlMessage::Options, 1) => Some(ControlMessage::Selection),
            (ControlMessage::Selection, 1) => Some(ControlMessage::Trace),
            (ControlMessage::Trace, 6) => Some(ControlMessage::MetadataEntry),
            (ControlMessage::MetadataEntry, 2) => Some(ControlMessage::MetadataValue),
            _ => None,
        };
        if let Some(child) = child {
            if wire != WireType::LengthDelimited {
                return Err(invalid("invocation controls have the wrong wire type"));
            }
            let length = decode_varint(&mut bytes).map_err(bridge_ctypes::CtypesError::from)?;
            let length = usize::try_from(length)
                .map_err(|_| invalid("invocation control length overflows"))?;
            if length > bytes.len() {
                return Err(invalid("truncated invocation controls"));
            }
            validate_fields(&bytes[..length], child)?;
            bytes = &bytes[length..];
        } else {
            skip_field(wire, tag, &mut bytes, DecodeContext::default())
                .map_err(bridge_ctypes::CtypesError::from)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::baml_bridge::cffi::{
        BamlHandleType, BamlOutboundHandle, BamlOutboundValue, InvocationOptions,
        TraceMetadataValue, TraceOptions, TraceSelection, baml_outbound_value,
        call_function_args::CallTarget,
    };

    fn canonical() -> CallFunctionArgs {
        CallFunctionArgs {
            call_id: 1,
            call_target: Some(CallTarget::FunctionName("identity".into())),
            invocation: Some(InvocationOptions::default()),
            ..Default::default()
        }
    }

    #[test]
    fn empty_canonical_options_are_required_even_without_controls() {
        let mut call = canonical();
        assert!(decode_call(&call.encode_to_vec()).is_ok());
        call.invocation = None;
        assert!(decode_call(&call.encode_to_vec()).is_err());
    }

    #[test]
    fn both_target_variants_and_expired_absolute_deadlines_survive_decoding() {
        let mut call = canonical();
        call.call_target = Some(CallTarget::FunctionHandle(123));
        call.invocation.as_mut().unwrap().deadline_ns = Some(0);
        let decoded = decode_call(&call.encode_to_vec()).unwrap();
        assert_eq!(decoded.call_target, call.call_target);
        assert_eq!(decoded.invocation.unwrap().deadline_ns, Some(0));
    }

    #[test]
    fn unknown_controls_are_rejected_before_protobuf_can_skip_them() {
        // Encode an unknown field inside InvocationOptions.
        let mut call = canonical();
        let mut options = call.invocation.take().unwrap().encode_to_vec();
        options.extend([0x38, 1]);
        let mut bytes = call.encode_to_vec();
        bytes.push(0x32); // CallFunctionArgs.invocation, length-delimited.
        prost::encoding::encode_varint(options.len() as u64, &mut bytes);
        bytes.extend(options);
        assert!(decode_call(&bytes).is_err());
        assert!(decode_call(&[0x32, 255, 255, 255]).is_err());
    }

    #[test]
    fn malformed_trace_selections_modes_and_removal_markers_are_rejected() {
        let mut call = canonical();
        let options = call.invocation.as_mut().unwrap();
        options.trace = Some(TraceSelection::default());
        assert!(decode_call(&call.encode_to_vec()).is_err());
        call.invocation.as_mut().unwrap().trace = Some(TraceSelection {
            selection: Some(trace_selection::Selection::Reservation(0)),
        });
        assert!(decode_call(&call.encode_to_vec()).is_err());
        for mode in [0, 4, -1] {
            call.invocation.as_mut().unwrap().trace = Some(TraceSelection {
                selection: Some(trace_selection::Selection::Options(TraceOptions {
                    mode: Some(mode),
                    ..Default::default()
                })),
            });
            assert!(decode_call(&call.encode_to_vec()).is_err());
        }
        for value in [None, Some(trace_metadata_value::Value::Remove(false))] {
            call.invocation.as_mut().unwrap().trace = Some(TraceSelection {
                selection: Some(trace_selection::Selection::Options(TraceOptions {
                    metadata: [("key".into(), TraceMetadataValue { value })].into(),
                    ..Default::default()
                })),
            });
            assert!(decode_call(&call.encode_to_vec()).is_err());
        }
    }

    #[test]
    fn trace_presence_and_null_removal_markers_are_preserved() {
        let mut call = canonical();
        call.invocation.as_mut().unwrap().trace = Some(TraceSelection {
            selection: Some(trace_selection::Selection::Options(TraceOptions {
                mode: Some(TraceMode::Span as i32),
                inputs: Some(false),
                output: Some(true),
                metadata: [(
                    "remove".into(),
                    TraceMetadataValue {
                        value: Some(trace_metadata_value::Value::Remove(true)),
                    },
                )]
                .into(),
                ..Default::default()
            })),
        });
        assert_eq!(decode_call(&call.encode_to_vec()).unwrap(), call);
    }

    #[test]
    fn v2_dispatch_requires_state_identity_and_effective_cancel_projection() {
        let mut host = HostInvocation {
            host_value_key: 1,
            callback_id: 2,
            effective_state: 3,
            cancel: Some(BamlOutboundValue {
                value: Some(baml_outbound_value::Value::HandleValue(
                    BamlOutboundHandle {
                        key: 4,
                        handle_type: BamlHandleType::AdtRuntimeValue as i32,
                        ..Default::default()
                    },
                )),
            }),
            deadline_ns: Some(0),
            ..Default::default()
        };
        assert_eq!(decode_host_invocation(&host.encode_to_vec()).unwrap(), host);
        host.effective_state = 0;
        assert!(decode_host_invocation(&host.encode_to_vec()).is_err());
        host.effective_state = 3;
        host.cancel = None;
        assert!(decode_host_invocation(&host.encode_to_vec()).is_err());
    }
}
