//! Decode C FFI / protobuf host values into `BexExternalValue`.
//!
//! Converts `InboundValue` (from the C bridge) to the engine's `BexExternalValue` representation
//! so the BEX engine can use them as function arguments.

use std::collections::HashMap;

use bex_project::{BexExternalAdt, BexExternalValue, RuntimeTy};
use indexmap::IndexMap;
use prost::Message;

use crate::{
    baml_bridge::cffi::{
        BamlHandleType, InboundClassValue, InboundEnumValue, InboundListValue, InboundMapEntry,
        InboundMapValue, InboundUnionValue, InboundValue,
        inbound_value::Value as InboundValueVariant,
    },
    error::CtypesError,
    handle_table::CffiHandleTable,
};

/// Decode a protobuf `InboundValue` into a `BexExternalValue` for use by the BEX engine.
///
/// Handles are resolved via `handle_table`; an unknown key returns `InvalidHandleKey`.
pub fn inbound_to_external(
    value: InboundValue,
    handle_table: &CffiHandleTable,
) -> Result<BexExternalValue, CtypesError> {
    match value.value {
        None => Ok(BexExternalValue::Null),
        Some(variant) => match variant {
            InboundValueVariant::StringValue(s) => Ok(BexExternalValue::String(s.into())),
            InboundValueVariant::IntValue(i) => Ok(BexExternalValue::Int(i)),
            InboundValueVariant::BigintValue(s) => {
                // Pre-allocation cap: stop a multi-megabyte hex blob from
                // building a multi-megabyte `BigInt` before the VM's own
                // `MAX_BIGINT_BITS` guard fires. Sized to match that VM cap
                // (~268M bits ≈ 67M hex digits, +2 for sign and slack).
                const MAX_BIGINT_HEX_LEN: usize = (1 << 28) / 4 + 2;
                // Hex / base sixteen on the wire, parsed strictly: `parse_bytes`
                // accepts only `[0-9a-fA-F]` plus an optional leading `-`/`+`
                // (no `0x` prefix, underscores, or surrounding whitespace),
                // matching the encoders' output and the Python/JS bridges.
                let len = s.len();
                if len > MAX_BIGINT_HEX_LEN {
                    return Err(CtypesError::InvalidBigint { len });
                }
                let bi = num_bigint::BigInt::parse_bytes(s.as_bytes(), 16)
                    .ok_or(CtypesError::InvalidBigint { len })?;
                Ok(BexExternalValue::Bigint(bi))
            }
            InboundValueVariant::FloatValue(f) => Ok(BexExternalValue::Float(f)),
            InboundValueVariant::BoolValue(b) => Ok(BexExternalValue::Bool(b)),
            InboundValueVariant::ListValue(list) => convert_list(list, handle_table),
            InboundValueVariant::MapValue(map) => convert_map(map, handle_table),
            InboundValueVariant::ClassValue(class) => convert_class(class, handle_table),
            InboundValueVariant::EnumValue(e) => Ok(convert_enum(e)),
            InboundValueVariant::UnionValue(union) => convert_union(*union, handle_table),
            InboundValueVariant::Uint8arrayValue(bytes) => Ok(BexExternalValue::Uint8Array(bytes)),
            // A reflected BAML type passed as an argument value.
            InboundValueVariant::TyValue(ty) => crate::ty_decode::proto_ty_to_external(&ty),
            InboundValueVariant::Handle(handle) => {
                // HOST_VALUE_* keys do NOT live in HANDLE_TABLE. The host
                // bridge owns the lookup; we construct the Arc stub here so
                // last-drop fires the registered HostReleaseFn.
                let host_value_kind =
                    if handle.handle_type == BamlHandleType::HostValueCallable as i32 {
                        Some(bex_project::HostValueKind::Callable)
                    } else if handle.handle_type == BamlHandleType::HostValueOpaque as i32 {
                        Some(bex_project::HostValueKind::Opaque)
                    } else {
                        None
                    };
                if let Some(kind) = host_value_kind {
                    // Intern by key so repeated decodes of the same wire key
                    // (e.g. BAML returns a host callable, the host passes it
                    // back in) share one Arc identity. Otherwise each decode
                    // would mint an independent Arc with its own refcount and
                    // the first last-drop would fire HostReleaseFn, tearing the
                    // registry entry out from under the still-live other Arc.
                    let arc = bex_project::HostValueArc::intern(handle.key, kind);
                    return Ok(BexExternalValue::HostValue(arc));
                }
                let value = handle_table
                    .drain(handle.key)
                    .ok_or(CtypesError::InvalidHandleKey(handle.key))?;
                Ok(BexExternalValue::from((*value).clone()))
            }
        },
    }
}

/// Build the default "any scalar" union type for untyped inbound values.
fn default_scalar_union_ty() -> RuntimeTy {
    let d = baml_type::TyAttr::default();
    RuntimeTy::Union(
        vec![
            RuntimeTy::Int { attr: d.clone() },
            RuntimeTy::Float { attr: d.clone() },
            RuntimeTy::String { attr: d.clone() },
            RuntimeTy::Bool { attr: d.clone() },
            RuntimeTy::Uint8Array { attr: d.clone() },
            RuntimeTy::Null { attr: d.clone() },
        ],
        d,
    )
}

fn convert_list(
    list: InboundListValue,
    handle_table: &CffiHandleTable,
) -> Result<BexExternalValue, CtypesError> {
    let has_item_type = list.item_type.is_some();
    let element_type = list
        .item_type
        .as_ref()
        .map(crate::ty_decode::proto_ty_to_runtime_ty)
        .transpose()?
        .unwrap_or_else(default_scalar_union_ty);
    let items = list
        .values
        .into_iter()
        .map(|v| inbound_to_external(v, handle_table))
        .collect::<Result<Vec<_>, _>>()?;
    if has_item_type {
        for (index, item) in items.iter().enumerate() {
            validate_collection_occurrence(item, &element_type).map_err(|error| {
                CtypesError::InvalidCollectionMetadata(format!(
                    "list item {index} contradicts item_type: {error}"
                ))
            })?;
        }
    }
    Ok(BexExternalValue::Array {
        element_type,
        items,
    })
}

fn convert_map(
    map: InboundMapValue,
    handle_table: &CffiHandleTable,
) -> Result<BexExternalValue, CtypesError> {
    let has_key_type = map.key_type.is_some();
    let has_value_type = map.value_type.is_some();
    let key_type = map
        .key_type
        .as_ref()
        .map(crate::ty_decode::proto_ty_to_runtime_ty)
        .transpose()?
        .unwrap_or_else(RuntimeTy::string);
    let value_type = map
        .value_type
        .as_ref()
        .map(crate::ty_decode::proto_ty_to_runtime_ty)
        .transpose()?
        .unwrap_or_else(default_scalar_union_ty);
    if has_key_type && !matches!(key_type, RuntimeTy::String { .. }) {
        return Err(CtypesError::InvalidCollectionMetadata(format!(
            "map key_type must be string, received `{key_type}`"
        )));
    }
    let mut entries = IndexMap::new();
    for (index, entry) in map.entries.into_iter().enumerate() {
        let key = extract_string_key(&entry)?;
        let value = entry
            .value
            .map(|v| inbound_to_external(v, handle_table))
            .transpose()?
            .unwrap_or(BexExternalValue::Null);
        if has_value_type {
            validate_collection_occurrence(&value, &value_type).map_err(|error| {
                CtypesError::InvalidCollectionMetadata(format!(
                    "map value {index} contradicts value_type: {error}"
                ))
            })?;
        }
        entries.insert(key, value);
    }
    Ok(BexExternalValue::Map {
        key_type,
        value_type,
        entries,
    })
}

fn validate_collection_occurrence(
    value: &BexExternalValue,
    expected: &RuntimeTy,
) -> Result<(), String> {
    if let BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle { ty, .. }) = value {
        if tagged_handle_occurrence_matches(ty, expected) {
            return Ok(());
        }
        return Err(format!(
            "tagged resource type `{ty}` does not match `{expected}`"
        ));
    }
    bex_project::validate_host_return(value, expected).map_err(|error| error.to_string())
}

fn tagged_handle_occurrence_matches(actual: &RuntimeTy, expected: &RuntimeTy) -> bool {
    match (actual, expected) {
        (_, RuntimeTy::BuiltinUnknown { .. } | RuntimeTy::TypeVar(..)) => true,
        (
            RuntimeTy::Class(actual_name, actual_args, _),
            RuntimeTy::Class(expected_name, expected_args, _),
        ) => actual_name == expected_name && actual_args == expected_args,
        (actual, RuntimeTy::Union(members, _)) => members
            .iter()
            .any(|member| tagged_handle_occurrence_matches(actual, member)),
        _ => false,
    }
}

fn convert_class(
    class: InboundClassValue,
    handle_table: &CffiHandleTable,
) -> Result<BexExternalValue, CtypesError> {
    let mut fields = IndexMap::new();
    for entry in class.fields {
        let key = extract_string_key(&entry)?;
        let value = entry
            .value
            .map(|v| inbound_to_external(v, handle_table))
            .transpose()?
            .unwrap_or(BexExternalValue::Null);
        fields.insert(key, value);
    }
    // The class binds via `class_ty`: `class_ty.name` is the FQN and
    // `class_ty.type_args` (De Bruijn order) a generic instance's concrete
    // args. A well-formed class value always sets `class_ty`; an absent one
    // decodes to an unnamed class with no type args.
    let (class_name, type_args) = match class.class_ty {
        Some(ty_class) => {
            let args = ty_class
                .type_args
                .iter()
                .map(crate::ty_decode::proto_ty_to_runtime_ty)
                .collect::<Result<Vec<_>, _>>()?;
            (ty_class.name, args)
        }
        None => (String::new(), vec![]),
    };
    Ok(BexExternalValue::Instance {
        class_name,
        type_args,
        fields,
    })
}

fn convert_enum(e: InboundEnumValue) -> BexExternalValue {
    BexExternalValue::Variant {
        enum_name: e.name,
        variant_name: e.value,
    }
}

fn convert_union(
    union: InboundUnionValue,
    handle_table: &CffiHandleTable,
) -> Result<BexExternalValue, CtypesError> {
    let self_type = union
        .self_type
        .ok_or_else(|| CtypesError::InvalidUnionMetadata("self_type is absent".to_string()))?;
    let self_type = crate::ty_decode::proto_ty_to_runtime_ty(&self_type)?;
    let RuntimeTy::Union(members, _) = self_type else {
        return Err(CtypesError::InvalidUnionMetadata(
            "self_type is not a union".to_string(),
        ));
    };
    let selected_type = union
        .selected_type
        .ok_or_else(|| CtypesError::InvalidUnionMetadata("selected_type is absent".to_string()))?;
    let selected_type = crate::ty_decode::proto_ty_to_runtime_ty(&selected_type)?;
    if !members.contains(&selected_type) {
        return Err(CtypesError::InvalidUnionMetadata(format!(
            "selected type `{selected_type}` is not a self_type member"
        )));
    }
    if union.value_option_name != selected_type.to_string() {
        return Err(CtypesError::InvalidUnionMetadata(format!(
            "value_option_name `{}` does not identify selected type `{selected_type}`",
            union.value_option_name
        )));
    }
    let value = union
        .value
        .ok_or_else(|| CtypesError::InvalidUnionMetadata("selected value is absent".to_string()))?;
    let value = inbound_to_external(*value, handle_table)?;
    bex_project::validate_host_return(&value, &selected_type).map_err(|error| {
        CtypesError::InvalidUnionMetadata(format!(
            "selected value does not satisfy selected type: {error}"
        ))
    })?;
    Ok(BexExternalValue::union(value, members, selected_type))
}

fn extract_string_key(entry: &InboundMapEntry) -> Result<String, CtypesError> {
    use crate::baml_bridge::cffi::inbound_map_entry::Key;
    match &entry.key {
        Some(Key::StringKey(s)) => Ok(s.clone()),
        Some(Key::IntKey(i)) => Ok(i.to_string()),
        Some(Key::BoolKey(b)) => Ok(b.to_string()),
        Some(Key::EnumKey(e)) => Ok(format!("{}::{}", e.name, e.value)),
        None => Err(CtypesError::MapEntryMissingKey),
    }
}

/// Decode protobuf kwargs into a `HashMap<String, BexExternalValue>` for engine call arguments.
pub fn kwargs_to_bex_values(
    kwargs: Vec<InboundMapEntry>,
    handle_table: &CffiHandleTable,
) -> Result<HashMap<String, BexExternalValue>, CtypesError> {
    let mut result = HashMap::new();
    for entry in kwargs {
        let key = extract_string_key(&entry)?;
        let value = entry
            .value
            .map(|v| inbound_to_external(v, handle_table))
            .transpose()?
            .unwrap_or(BexExternalValue::Null);
        result.insert(key, value);
    }
    Ok(result)
}

/// Decode playground run arguments from a host-call-free byte envelope.
///
/// The envelope is a sequence of length-delimited `InboundMapEntry` records.
/// It deliberately does not reuse `CallFunctionArgs`, because that CFFI
/// request type also carries a host call id. `RunStore` adapters allocate host
/// plumbing separately after the run identity exists.
pub fn playground_run_args_to_bex_values(
    mut bytes: &[u8],
    handle_table: &CffiHandleTable,
) -> Result<HashMap<String, BexExternalValue>, CtypesError> {
    let mut kwargs = Vec::new();
    while !bytes.is_empty() {
        kwargs.push(InboundMapEntry::decode_length_delimited(&mut bytes)?);
    }
    kwargs_to_bex_values(kwargs, handle_table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{baml_bridge::cffi::BamlHandle, handle_table::CffiHandleTableEntry};

    #[test]
    fn decode_inbound_host_value_callable() {
        let table = CffiHandleTable::new();
        let handle = BamlHandle {
            key: 999,
            handle_type: BamlHandleType::HostValueCallable as i32,
        };
        let inbound = InboundValue {
            value: Some(InboundValueVariant::Handle(handle)),
        };
        let result = inbound_to_external(inbound, &table).expect("decode succeeds");
        match result {
            BexExternalValue::HostValue(arc) => {
                assert_eq!(arc.key, 999);
                assert_eq!(arc.kind, bex_project::HostValueKind::Callable);
            }
            other => panic!("unexpected variant: {other:?}"),
        }
        // Key must NOT have been inserted into the handle table.
        assert!(
            table.resolve(999).is_none(),
            "HOST_VALUE_CALLABLE must not touch HANDLE_TABLE"
        );
    }

    #[test]
    fn decode_inbound_host_value_opaque() {
        let table = CffiHandleTable::new();
        let handle = BamlHandle {
            key: 777,
            handle_type: BamlHandleType::HostValueOpaque as i32,
        };
        let inbound = InboundValue {
            value: Some(InboundValueVariant::Handle(handle)),
        };
        let result = inbound_to_external(inbound, &table).expect("decode succeeds");
        match result {
            BexExternalValue::HostValue(arc) => {
                assert_eq!(arc.key, 777);
                assert_eq!(arc.kind, bex_project::HostValueKind::Opaque);
            }
            other => panic!("unexpected variant: {other:?}"),
        }
        // Like callables, opaque handles bypass the HANDLE_TABLE.
        assert!(
            table.resolve(777).is_none(),
            "HOST_VALUE_OPAQUE must not touch HANDLE_TABLE"
        );
    }

    #[test]
    fn inbound_handle_drains_table_entry() {
        let table = CffiHandleTable::new();
        let key = table.insert(CffiHandleTableEntry::FunctionRef { global_index: 7 });
        let inbound = InboundValue {
            value: Some(InboundValueVariant::Handle(BamlHandle {
                key,
                handle_type: 0,
            })),
        };
        let result = inbound_to_external(inbound, &table).expect("decode succeeds");
        assert!(matches!(
            result,
            BexExternalValue::FunctionRef { global_index: 7 }
        ));
        assert!(
            table.resolve(key).is_none(),
            "entry must be removed from table after drain"
        );
    }

    #[test]
    fn inbound_handle_missing_key_errors() {
        let table = CffiHandleTable::new();
        let inbound = InboundValue {
            value: Some(InboundValueVariant::Handle(BamlHandle {
                key: 9999,
                handle_type: 0,
            })),
        };
        let err = inbound_to_external(inbound, &table).expect_err("missing key should error");
        assert!(matches!(err, CtypesError::InvalidHandleKey(9999)));
    }

    /// Helper: round-trip a `BexExternalValue` through the outbound encoder and
    /// the inbound decoder by translating the outbound `bigint_value` discriminator
    /// to the parallel inbound discriminator.
    fn bigint_round_trip(original: &BexExternalValue) -> BexExternalValue {
        use crate::{
            baml_bridge::cffi::baml_outbound_value::Value as OutboundVariant,
            handle_table::CffiHandleTableOptions, value_encode::external_to_outbound,
        };

        let opts = CffiHandleTableOptions::for_in_process();
        let outbound = external_to_outbound(original, &opts).expect("encode succeeds");
        let s = match outbound.value {
            Some(OutboundVariant::BigintValue(s)) => s,
            other => panic!("expected outbound BigintValue, got {other:?}"),
        };
        let inbound = InboundValue {
            value: Some(InboundValueVariant::BigintValue(s)),
        };
        let table = CffiHandleTable::new();
        inbound_to_external(inbound, &table).expect("decode succeeds")
    }

    #[test]
    fn test_bigint_round_trip_small() {
        let bi = num_bigint::BigInt::from(42);
        let v = BexExternalValue::Bigint(bi);
        let decoded = bigint_round_trip(&v);
        assert_eq!(decoded, v);
    }

    #[test]
    fn test_bigint_round_trip_huge() {
        let bi = num_bigint::BigInt::parse_bytes(
            b"99999999999999999999999999999999999999999999999999",
            10,
        )
        .unwrap();
        let v = BexExternalValue::Bigint(bi);
        let decoded = bigint_round_trip(&v);
        assert_eq!(decoded, v);
    }

    #[test]
    fn test_bigint_round_trip_negative() {
        let bi = num_bigint::BigInt::from(-42);
        let v = BexExternalValue::Bigint(bi);
        let decoded = bigint_round_trip(&v);
        assert_eq!(decoded, v);
    }

    #[test]
    fn test_bigint_decode_invalid() {
        let inbound = InboundValue {
            value: Some(InboundValueVariant::BigintValue("not-a-number".into())),
        };
        let table = CffiHandleTable::new();
        let err = inbound_to_external(inbound, &table).expect_err("invalid bigint should error");
        assert!(matches!(err, CtypesError::InvalidBigint { len: 12 }));
        assert_eq!(err.to_string(), "Invalid bigint hex string (12 bytes)");
    }

    #[test]
    fn test_bigint_decode_too_long() {
        // Build a hex blob just over the FFI cap so the size check fires
        // before `parse_bytes`. The error must carry only the length, not the
        // input itself.
        let blob = "f".repeat((1 << 28) / 4 + 3);
        let blob_len = blob.len();
        let inbound = InboundValue {
            value: Some(InboundValueVariant::BigintValue(blob)),
        };
        let table = CffiHandleTable::new();
        let err = inbound_to_external(inbound, &table).expect_err("over-cap bigint should error");
        let CtypesError::InvalidBigint { len } = err else {
            panic!("expected InvalidBigint, got: {err:?}");
        };
        assert_eq!(len, blob_len);
        let message = err.to_string();
        assert_eq!(
            message,
            format!("Invalid bigint hex string ({blob_len} bytes)")
        );
        // Sanity: the error message does not embed the megabyte-scale input.
        assert!(message.len() < 200);
    }

    /// Phase 2 gate: a generic class instance carries its concrete class type
    /// args over the wire in `InboundClassValue.class_ty`, and `convert_class`
    /// lands them in `BexExternalValue::Instance::type_args`.
    #[test]
    fn convert_class_decodes_generic_type_args() {
        use crate::baml_bridge::cffi::{
            BamlTy, BamlTyClass, BamlTyPrimitive, BamlTyPrimitiveKind, InboundClassValue,
            baml_ty::Ty as TyVariant,
        };
        let int_ty = BamlTy {
            ty: Some(TyVariant::Primitive(BamlTyPrimitive {
                kind: BamlTyPrimitiveKind::BamlTyPrimitiveInt as i32,
            })),
        };
        let class = InboundClassValue {
            fields: vec![],
            class_ty: Some(BamlTyClass {
                name: "generic_tests.GenericBox".to_string(),
                type_args: vec![int_ty],
            }),
        };
        let table = CffiHandleTable::new();
        let result = convert_class(class, &table).expect("decode succeeds");
        match result {
            BexExternalValue::Instance {
                class_name,
                type_args,
                ..
            } => {
                assert_eq!(class_name, "generic_tests.GenericBox");
                assert_eq!(type_args, vec![RuntimeTy::int()]);
            }
            other => panic!("expected Instance, got: {other:?}"),
        }
    }

    /// A non-generic instance binds its FQN from `class_ty` with empty `type_args`.
    #[test]
    fn convert_class_non_generic_has_empty_type_args() {
        use crate::baml_bridge::cffi::{BamlTyClass, InboundClassValue};
        let class = InboundClassValue {
            fields: vec![],
            class_ty: Some(BamlTyClass {
                name: "user.Plain".to_string(),
                type_args: vec![],
            }),
        };
        let table = CffiHandleTable::new();
        let result = convert_class(class, &table).expect("decode succeeds");
        match result {
            BexExternalValue::Instance {
                class_name,
                type_args,
                ..
            } => {
                assert_eq!(class_name, "user.Plain");
                assert!(type_args.is_empty());
            }
            other => panic!("expected Instance, got: {other:?}"),
        }
    }
}
