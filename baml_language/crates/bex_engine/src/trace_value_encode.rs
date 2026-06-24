//! Artifact-safe encoding for trace-owned value snapshots.

use num_bigint::BigInt;
use prost::Message;

use crate::trace_heap::{
    TraceOmissionDescriptor, TraceOmissionReason, TraceSnapshot, TraceValue, TraceValueRef,
};

#[derive(Clone, PartialEq, Message)]
struct BamlOutboundValue {
    #[prost(
        oneof = "BamlValueVariant",
        tags = "2, 3, 4, 5, 6, 7, 8, 11, 12, 19, 20"
    )]
    value: Option<BamlValueVariant>,
}

#[derive(Clone, PartialEq, ::prost::Oneof)]
enum BamlValueVariant {
    #[prost(message, tag = "2")]
    NullValue(BamlValueNull),
    #[prost(string, tag = "3")]
    StringValue(String),
    #[prost(int64, tag = "4")]
    IntValue(i64),
    #[prost(double, tag = "5")]
    FloatValue(f64),
    #[prost(bool, tag = "6")]
    BoolValue(bool),
    #[prost(message, tag = "7")]
    ClassValue(BamlValueClass),
    #[prost(message, tag = "8")]
    EnumValue(BamlValueEnum),
    #[prost(message, tag = "11")]
    ListValue(BamlValueList),
    #[prost(message, tag = "12")]
    MapValue(BamlValueMap),
    #[prost(bytes, tag = "19")]
    Uint8arrayValue(Vec<u8>),
    #[prost(string, tag = "20")]
    BigintValue(String),
}

#[derive(Clone, PartialEq, Message)]
struct BamlValueNull {}

#[derive(Clone, PartialEq, Message)]
struct BamlValueList {
    #[prost(message, repeated, tag = "2")]
    items: Vec<BamlOutboundValue>,
}

#[derive(Clone, PartialEq, Message)]
struct BamlOutboundMapEntry {
    #[prost(string, tag = "1")]
    key: String,
    #[prost(message, optional, tag = "2")]
    value: Option<BamlOutboundValue>,
}

#[derive(Clone, PartialEq, Message)]
struct BamlValueMap {
    #[prost(message, repeated, tag = "3")]
    entries: Vec<BamlOutboundMapEntry>,
}

#[derive(Clone, PartialEq, Message)]
struct BamlValueClass {
    #[prost(string, tag = "1")]
    name: String,
    #[prost(message, repeated, tag = "2")]
    fields: Vec<BamlOutboundMapEntry>,
}

#[derive(Clone, PartialEq, Message)]
struct BamlValueEnum {
    #[prost(string, tag = "1")]
    name: String,
    #[prost(string, tag = "2")]
    value: String,
    #[prost(bool, tag = "3")]
    is_dynamic: bool,
}

pub(crate) fn encode_trace_snapshot_body(snapshot: &TraceSnapshot) -> Result<Vec<u8>, String> {
    let value = encode_value(snapshot, snapshot.root())?;
    Ok(value.encode_to_vec())
}

fn encode_value(
    snapshot: &TraceSnapshot,
    value_ref: TraceValueRef,
) -> Result<BamlOutboundValue, String> {
    let value = snapshot
        .value(value_ref)
        .ok_or_else(|| format!("trace value ref {} is missing", value_ref.raw()))?;
    let variant = match value {
        TraceValue::Null => Some(BamlValueVariant::NullValue(BamlValueNull {})),
        TraceValue::Bool(value) => Some(BamlValueVariant::BoolValue(*value)),
        TraceValue::Int(value) => Some(BamlValueVariant::IntValue(*value)),
        TraceValue::Float(value) => Some(BamlValueVariant::FloatValue(*value)),
        TraceValue::Bigint(value) => Some(BamlValueVariant::BigintValue(bigint_to_hex(value))),
        TraceValue::String(value) => Some(BamlValueVariant::StringValue(value.clone())),
        TraceValue::Bytes(value) => Some(BamlValueVariant::Uint8arrayValue(value.clone())),
        TraceValue::Array(items) => Some(BamlValueVariant::ListValue(BamlValueList {
            items: items
                .iter()
                .copied()
                .map(|item| encode_value(snapshot, item))
                .collect::<Result<_, _>>()?,
        })),
        TraceValue::Map(entries) => Some(BamlValueVariant::MapValue(BamlValueMap {
            entries: entries
                .iter()
                .map(|(key, value)| {
                    Ok(BamlOutboundMapEntry {
                        key: key.clone(),
                        value: Some(encode_value(snapshot, *value)?),
                    })
                })
                .collect::<Result<_, String>>()?,
        })),
        TraceValue::Instance {
            type_name,
            type_args: _,
            fields,
        } => Some(BamlValueVariant::ClassValue(BamlValueClass {
            name: type_name.clone(),
            fields: fields
                .iter()
                .map(|(key, value)| {
                    Ok(BamlOutboundMapEntry {
                        key: key.clone(),
                        value: Some(encode_value(snapshot, *value)?),
                    })
                })
                .collect::<Result<_, String>>()?,
        })),
        TraceValue::Enum { type_name, variant } => {
            Some(BamlValueVariant::EnumValue(BamlValueEnum {
                name: type_name.clone(),
                value: variant.clone(),
                is_dynamic: false,
            }))
        }
        TraceValue::Omitted(descriptor) => {
            Some(BamlValueVariant::ClassValue(omission_to_class(descriptor)))
        }
    };
    Ok(BamlOutboundValue { value: variant })
}

fn omission_to_class(descriptor: &TraceOmissionDescriptor) -> BamlValueClass {
    BamlValueClass {
        name: "baml.trace.OmittedValue".to_string(),
        fields: vec![
            BamlOutboundMapEntry {
                key: "reason".to_string(),
                value: Some(BamlOutboundValue {
                    value: Some(BamlValueVariant::StringValue(
                        omission_reason_wire(descriptor.reason).to_string(),
                    )),
                }),
            },
            BamlOutboundMapEntry {
                key: "message".to_string(),
                value: Some(BamlOutboundValue {
                    value: Some(BamlValueVariant::StringValue(descriptor.message.clone())),
                }),
            },
        ],
    }
}

fn omission_reason_wire(reason: TraceOmissionReason) -> &'static str {
    match reason {
        TraceOmissionReason::OmittedArgument => "omittedArgument",
        TraceOmissionReason::UnsupportedValue => "unsupportedValue",
        TraceOmissionReason::HostOwnedValue => "hostOwnedValue",
        TraceOmissionReason::InvalidRuntimeValue => "invalidRuntimeValue",
    }
}

fn bigint_to_hex(value: &str) -> String {
    value
        .parse::<BigInt>()
        .map(|value| format!("{value:x}"))
        .unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use bridge_ctypes::baml_core::cffi::{
        BamlOutboundValue, baml_outbound_value::Value as BamlValueVariant,
    };
    use prost::Message;

    use crate::trace_heap::{
        TraceOmissionDescriptor, TraceOmissionReason, TraceSnapshot, TraceValue, TraceValueRef,
    };

    #[test]
    fn trace_snapshot_encodes_as_bare_baml_outbound_value() {
        let snapshot = TraceSnapshot::for_test(
            TraceValueRef::for_test(2),
            vec![
                TraceValue::String("world".to_string()),
                TraceValue::Int(7),
                TraceValue::Map(vec![
                    ("hello".to_string(), TraceValueRef::for_test(0)),
                    ("count".to_string(), TraceValueRef::for_test(1)),
                ]),
            ],
        );

        let bytes = super::encode_trace_snapshot_body(&snapshot).unwrap();
        let decoded = BamlOutboundValue::decode(bytes.as_slice()).unwrap();
        let Some(BamlValueVariant::MapValue(map)) = decoded.value else {
            panic!("root should encode as a map");
        };
        assert_eq!(map.entries.len(), 2);
    }

    #[test]
    fn omitted_trace_values_encode_as_renderable_class_values() {
        let snapshot = TraceSnapshot::for_test(
            TraceValueRef::for_test(0),
            vec![TraceValue::Omitted(TraceOmissionDescriptor {
                reason: TraceOmissionReason::HostOwnedValue,
                message: "host-owned callable".to_string(),
            })],
        );

        let bytes = super::encode_trace_snapshot_body(&snapshot).unwrap();
        let decoded = BamlOutboundValue::decode(bytes.as_slice()).unwrap();
        let Some(BamlValueVariant::ClassValue(class)) = decoded.value else {
            panic!("omission should encode as a class");
        };
        assert_eq!(class.name, "baml.trace.OmittedValue");
        assert_eq!(class.fields[0].key, "reason");
    }
}
