//! Data-only projections used by every generated invocation and trace facade.

use bex_project::{BexExternalAdt, RustPayload, TaggedHeapHandleKind};
use bridge_ctypes::{
    CffiHandleTableEntry, HANDLE_TABLE, TableKey, TraceReservationHandle, WireHandle,
    baml_bridge::cffi::{
        BamlOutboundMapEntry, BamlOutboundValue, BamlValueClass, BamlValueMap, TraceMetadataValue,
        TraceMode, TraceOptions, TraceSelection, baml_outbound_value::Value, trace_metadata_value,
        trace_selection,
    },
};
use btel_types::{InvocationMode, context::ContextValue};
use prost::Message;

use crate::BridgeError;

fn invalid(message: &str) -> BridgeError {
    BridgeError::InvocationProtocol(message.into())
}

/// The payload of the `@@rust_type` instance the engine reference keyed `key`
/// names, read from `runtime`: a trace control a host hands the bridge is an
/// instance its runtime keeps. `what` names the control for refusals.
pub(crate) fn instance_payload(
    runtime: &dyn bex_project::Bex,
    key: u64,
    what: &str,
) -> Result<RustPayload, BridgeError> {
    let entry = TableKey::new(key)
        .and_then(|key| HANDLE_TABLE.resolve(key))
        .ok_or_else(|| invalid(&format!("{what} is no longer live")))?;
    let CffiHandleTableEntry::Adt(BexExternalAdt::TaggedHeapHandle {
        kind: TaggedHeapHandleKind::RuntimeValue,
        heap_handle,
        ..
    }) = &*entry
    else {
        return Err(invalid(&format!("expected a reference to {what}")));
    };
    futures::executor::block_on(runtime.rust_payload(heap_handle)).ok_or_else(|| {
        invalid(&format!(
            "{what} is stale or belongs to a different BAML runtime"
        ))
    })
}

pub fn trace_selection(call_id: u64, key: u64) -> Result<(Vec<u8>, Option<u64>), BridgeError> {
    // Read through the call's own runtime: a reservation belongs to the
    // recording scope it was made in.
    let owner = crate::CALL_ALLOCATIONS.owner(call_id)?;
    let reservation = match instance_payload(&*owner, key, "a generated trace selection")? {
        RustPayload::TraceOptions(options) => {
            let mut wire = TraceOptions {
                mode: options.mode.map(|mode| match mode {
                    InvocationMode::Hidden => TraceMode::Hidden as i32,
                    InvocationMode::Timing => TraceMode::Timing as i32,
                    InvocationMode::Span => TraceMode::Span as i32,
                }),
                inputs: options.inputs,
                output: options.output,
                error: options.error,
                ..Default::default()
            };
            if let Some(context) = &options.context {
                wire.distinct_id.clone_from(&context.distinct_id);
                for (key, value) in &context.metadata {
                    use trace_metadata_value::Value as Metadata;
                    let value = match value {
                        None => Metadata::Remove(true),
                        Some(ContextValue::String(v)) => Metadata::StringValue(v.clone()),
                        Some(ContextValue::Int(v)) => Metadata::IntValue(*v),
                        Some(ContextValue::Float(v)) => Metadata::FloatValue(*v),
                        Some(ContextValue::Bool(v)) => Metadata::BoolValue(*v),
                    };
                    wire.metadata
                        .insert(key.clone(), TraceMetadataValue { value: Some(value) });
                }
            }
            return Ok((
                TraceSelection {
                    selection: Some(trace_selection::Selection::Options(wire)),
                }
                .encode_to_vec(),
                None,
            ));
        }
        RustPayload::ReservedSpan(reservation) => reservation,
        _ => {
            return Err(invalid("expected generated trace options or reservation"));
        }
    };
    // The typed capability pins the owner during preparation. The engine also
    // validates the reservation's original recording scope before admission.
    let key = HANDLE_TABLE
        .insert(CffiHandleTableEntry::TraceReservation(
            TraceReservationHandle { owner, reservation },
        ))
        .get();
    Ok((
        TraceSelection {
            selection: Some(trace_selection::Selection::Reservation(key)),
        }
        .encode_to_vec(),
        Some(key),
    ))
}

pub fn invocation_context(key: u64) -> Result<Vec<u8>, BridgeError> {
    let entry = match TableKey::new(key) {
        None => None,
        Some(key) => Some(
            HANDLE_TABLE
                .resolve(key)
                .ok_or_else(|| invalid("invocation is no longer live"))?,
        ),
    };
    let context = match entry.as_deref() {
        Some(CffiHandleTableEntry::InvocationState(state)) => state.state.trace_context().clone(),
        None => Default::default(),
        _ => return Err(invalid("expected an invocation-state handle")),
    };
    let wrap = |value| BamlOutboundValue { value: Some(value) };
    let fields = vec![
        BamlOutboundMapEntry {
            key: "distinct_id".into(),
            value: Some(wrap(context.distinct_id().map_or_else(
                || Value::NullValue(Default::default()),
                |id| Value::StringValue(id.into()),
            ))),
        },
        BamlOutboundMapEntry {
            key: "metadata".into(),
            value: Some(wrap(Value::MapValue(BamlValueMap {
                entries: context
                    .metadata()
                    .iter()
                    .map(|(key, value)| BamlOutboundMapEntry {
                        key: key.clone(),
                        value: Some(wrap(match value {
                            ContextValue::String(v) => Value::StringValue(v.clone()),
                            ContextValue::Int(v) => Value::IntValue(*v),
                            ContextValue::Float(v) => Value::FloatValue(*v),
                            ContextValue::Bool(v) => Value::BoolValue(*v),
                        })),
                    })
                    .collect(),
                ..Default::default()
            }))),
        },
    ];
    Ok(wrap(Value::ClassValue(BamlValueClass {
        name: "trace.Context".into(),
        fields,
        type_args: vec![],
    }))
    .encode_to_vec())
}

/// Clone each owned occurrence before a generated token escapes its dispatch.
pub fn clone_outbound(value: &BamlOutboundValue) -> Result<BamlOutboundValue, BridgeError> {
    fn clone_refs(
        value: &mut BamlOutboundValue,
        owned: &mut Vec<TableKey>,
    ) -> Result<(), BridgeError> {
        match value.value.as_mut() {
            // A host value is the engine's reference; the token borrows nothing of it.
            Some(Value::HandleValue(handle)) => match WireHandle::try_from(&*handle) {
                Ok(WireHandle::Engine(key)) => {
                    let cloned = HANDLE_TABLE
                        .clone_handle(key)
                        .ok_or_else(|| invalid("invocation token handle is no longer live"))?;
                    handle.key = cloned.get();
                    owned.push(cloned);
                }
                Ok(WireHandle::Host(..)) => {}
                Err(_) => return Err(invalid("invocation token handle names nothing")),
            },
            Some(Value::ClassValue(class)) => {
                for field in &mut class.fields {
                    if let Some(value) = &mut field.value {
                        clone_refs(value, owned)?;
                    }
                }
            }
            Some(Value::ListValue(list)) => {
                for value in &mut list.items {
                    clone_refs(value, owned)?;
                }
            }
            Some(Value::MapValue(map)) => {
                for field in &mut map.entries {
                    if let Some(value) = &mut field.value {
                        clone_refs(value, owned)?;
                    }
                }
            }
            Some(Value::UnionVariantValue(union)) => {
                if let Some(value) = &mut union.value {
                    clone_refs(value, owned)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut cloned = value.clone();
    let mut owned = vec![];
    if let Err(error) = clone_refs(&mut cloned, &mut owned) {
        for key in owned {
            HANDLE_TABLE.release(key);
        }
        return Err(error);
    }
    Ok(cloned)
}
