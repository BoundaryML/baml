//! Inbound (host → engine) wire construction helpers for generated code.
//!
//! These build the prost structs that [`crate::runtime`] encodes into one
//! `CallFunctionArgs` buffer per call for the C ABI.

use crate::wire::{self, inbound_map_entry::Key, inbound_value::Value as In};

/// Build the kwargs list for a function call. An entry whose value is
/// `None` is omitted entirely — that is how [`OptionalArg::Unset`] makes
/// the engine evaluate the parameter's BAML default (distinct from an
/// explicit null, which rides as a value-less `InboundValue`).
///
/// [`OptionalArg::Unset`]: crate::OptionalArg::Unset
pub fn kwargs(entries: Vec<(&str, Option<wire::InboundValue>)>) -> Vec<wire::InboundMapEntry> {
    entries
        .into_iter()
        .filter_map(|(name, value)| {
            value.map(|v| wire::InboundMapEntry {
                key: Some(Key::StringKey(name.to_string())),
                value: Some(v),
            })
        })
        .collect()
}

/// Encode a class instance. `fqn` is the BAML class FQN the generated
/// impl bakes in; the engine binds the instance nominally through it.
pub fn class(fqn: &str, fields: Vec<(&str, wire::InboundValue)>) -> wire::InboundValue {
    wire::InboundValue {
        value: Some(In::ClassValue(wire::InboundClassValue {
            fields: fields
                .into_iter()
                .map(|(name, value)| wire::InboundMapEntry {
                    key: Some(Key::StringKey(name.to_string())),
                    value: Some(value),
                })
                .collect(),
            class_ty: Some(wire::BamlTyClass {
                name: fqn.to_string(),
                // Empty is correct for the current scope: sdkgen skips generic
                // classes (`analyze.rs`), so every class routed here is
                // non-parameterized. When generic classes land, their concrete
                // type args must be threaded through this helper.
                type_args: Vec::new(),
            }),
        })),
    }
}

/// Encode an enum value. `variant_value` is the variant's wire value (not
/// necessarily its Rust variant name).
pub fn enum_value(fqn: &str, variant_value: &str) -> wire::InboundValue {
    wire::InboundValue {
        value: Some(In::EnumValue(wire::InboundEnumValue {
            name: fqn.to_string(),
            value: variant_value.to_string(),
        })),
    }
}
