//! Decode C FFI / protobuf host values into `BexExternalValue`.
//!
//! Converts `InboundValue` (from the C bridge) to the engine's `BexExternalValue` representation
//! so the BEX engine can use them as function arguments.

use std::{collections::HashMap, sync::Arc};

use bex_project::{
    BexExternalAdt, BexExternalValue, MediaKind, MediaValue, PromptAst, PromptAstSimple, RuntimeTy,
};
use indexmap::IndexMap;
use prost::Message;

use crate::{
    baml_bridge::cffi::{
        BamlHandleType, BamlValueMedia, BamlValuePromptAst, BamlValuePromptAstSimple,
        InboundClassValue, InboundEnumValue, InboundListValue, InboundMapEntry, InboundMapValue,
        InboundValue, MediaTypeEnum, baml_value_media,
        baml_value_prompt_ast::Value as PromptAstVariant,
        baml_value_prompt_ast_simple::Value as PromptAstSimpleVariant,
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
    let value_type = value
        .value_type
        .as_ref()
        .map(crate::ty_decode::proto_ty_to_runtime_ty)
        .transpose()?;
    // `value_type` identifies the exact type selected for this node. A root
    // union (including `optional<T>`, which lowers to `T | null`) only repeats
    // a set of possible types and therefore cannot select an arm. Unions are
    // still valid below an exact outer type, such as `list<int | string>`.
    if matches!(value_type, Some(RuntimeTy::Union(..))) {
        return Err(CtypesError::InvalidInboundValueTypeRootUnion);
    }
    let decoded = match value.value {
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
            InboundValueVariant::Uint8arrayValue(bytes) => Ok(BexExternalValue::Uint8Array(bytes)),
            // A reflected BAML type passed as an argument value.
            InboundValueVariant::TyValue(ty) => crate::ty_decode::proto_ty_to_external(&ty),
            InboundValueVariant::TyDefValue(ty) => crate::ty_decode::proto_ty_def_to_external(&ty),
            InboundValueVariant::MediaValue(media) => Ok(BexExternalValue::Adt(
                BexExternalAdt::Media(proto_media_to_bex_media(media)?),
            )),
            InboundValueVariant::PromptAstValue(prompt) => Ok(BexExternalValue::Adt(
                BexExternalAdt::PromptAst(Arc::new(proto_prompt_ast_to_bex_prompt_ast(prompt)?)),
            )),
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
    }?;
    Ok(match value_type {
        Some(value_type) => BexExternalValue::typed(decoded, value_type),
        None => decoded,
    })
}

fn proto_media_to_bex_media(media: BamlValueMedia) -> Result<Arc<MediaValue>, CtypesError> {
    let kind = match MediaTypeEnum::try_from(media.media) {
        Ok(MediaTypeEnum::Image) => MediaKind::Image,
        Ok(MediaTypeEnum::Audio) => MediaKind::Audio,
        Ok(MediaTypeEnum::Video) => MediaKind::Video,
        Ok(MediaTypeEnum::Pdf) => MediaKind::Pdf,
        Ok(MediaTypeEnum::Other) => MediaKind::Generic,
        Ok(MediaTypeEnum::MediaTypeUnspecified) | Err(_) => {
            return Err(CtypesError::InternalError(
                "portable media payload has no valid media kind".to_string(),
            ));
        }
    };
    let mime_type = media.mime_type.as_deref();
    match media.value {
        Some(baml_value_media::Value::Url(url)) => Ok(MediaValue::from_url(kind, &url, mime_type)),
        Some(baml_value_media::Value::Base64(base64)) => {
            Ok(MediaValue::from_base64(kind, &base64, mime_type))
        }
        Some(baml_value_media::Value::File(file)) => {
            Ok(MediaValue::from_file(kind, &file, mime_type))
        }
        None => Err(CtypesError::InternalError(
            "portable media payload has no content".to_string(),
        )),
    }
}

fn proto_prompt_ast_to_bex_prompt_ast(
    prompt: BamlValuePromptAst,
) -> Result<PromptAst, CtypesError> {
    match prompt.value {
        Some(PromptAstVariant::Simple(simple)) => Ok(PromptAst::Simple(Arc::new(
            proto_prompt_ast_simple_to_bex_prompt_ast_simple(simple)?,
        ))),
        Some(PromptAstVariant::Message(message)) => {
            let content = message.content.ok_or_else(|| {
                CtypesError::InternalError("portable prompt message has no content".to_string())
            })?;
            let metadata = serde_json::from_str(&message.metadata_as_json).map_err(|error| {
                CtypesError::InternalError(format!(
                    "portable prompt message has invalid metadata JSON: {error}"
                ))
            })?;
            Ok(PromptAst::Message {
                role: message.role,
                content: Arc::new(proto_prompt_ast_simple_to_bex_prompt_ast_simple(content)?),
                metadata,
            })
        }
        Some(PromptAstVariant::Multiple(multiple)) => Ok(PromptAst::Vec(
            multiple
                .items
                .into_iter()
                .map(proto_prompt_ast_to_bex_prompt_ast)
                .map(|item| item.map(Arc::new))
                .collect::<Result<_, _>>()?,
        )),
        None => Err(CtypesError::InternalError(
            "portable prompt payload has no value".to_string(),
        )),
    }
}

fn proto_prompt_ast_simple_to_bex_prompt_ast_simple(
    simple: BamlValuePromptAstSimple,
) -> Result<PromptAstSimple, CtypesError> {
    match simple.value {
        Some(PromptAstSimpleVariant::String(string)) => Ok(PromptAstSimple::String(string)),
        Some(PromptAstSimpleVariant::Media(media)) => {
            Ok(PromptAstSimple::Media(proto_media_to_bex_media(media)?))
        }
        Some(PromptAstSimpleVariant::Multiple(multiple)) => Ok(PromptAstSimple::Multiple(
            multiple
                .items
                .into_iter()
                .map(proto_prompt_ast_simple_to_bex_prompt_ast_simple)
                .map(|item| item.map(Arc::new))
                .collect::<Result<_, _>>()?,
        )),
        None => Err(CtypesError::InternalError(
            "portable prompt content has no value".to_string(),
        )),
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
    let items: Result<Vec<BexExternalValue>, CtypesError> = list
        .values
        .into_iter()
        .map(|v| inbound_to_external(v, handle_table))
        .collect();
    Ok(BexExternalValue::Array {
        element_type: default_scalar_union_ty(),
        items: items?,
    })
}

fn convert_map(
    map: InboundMapValue,
    handle_table: &CffiHandleTable,
) -> Result<BexExternalValue, CtypesError> {
    let mut entries = IndexMap::new();
    for entry in map.entries {
        let key = extract_string_key(&entry)?;
        let value = entry
            .value
            .map(|v| inbound_to_external(v, handle_table))
            .transpose()?
            .unwrap_or(BexExternalValue::Null);
        entries.insert(key, value);
    }
    Ok(BexExternalValue::Map {
        key_type: RuntimeTy::string(),
        value_type: default_scalar_union_ty(),
        entries,
    })
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
    Ok(BexExternalValue::Instance {
        class_name: String::new(),
        type_args: vec![],
        fields,
    })
}

fn convert_enum(e: InboundEnumValue) -> BexExternalValue {
    BexExternalValue::Variant {
        enum_name: e.name,
        variant_name: e.value,
    }
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

    fn typed_input(value_type: &RuntimeTy, value: InboundValueVariant) -> InboundValue {
        InboundValue {
            value_type: Some(crate::ty_encode::runtime_ty_to_proto_ty(value_type)),
            value: Some(value),
        }
    }

    #[test]
    fn sparse_value_type_preserves_authoritative_type() {
        let decoded = inbound_to_external(
            typed_input(&RuntimeTy::int(), InboundValueVariant::IntValue(7)),
            &CffiHandleTable::new(),
        )
        .unwrap();
        let BexExternalValue::Union { value, metadata } = decoded else {
            panic!("expected typed-value carrier")
        };
        assert_eq!(*value, BexExternalValue::Int(7));
        assert!(metadata.is_inbound_type_annotation);
        assert_eq!(metadata.selected_option, RuntimeTy::int());
    }

    #[test]
    fn value_type_is_optional() {
        let decoded = inbound_to_external(
            InboundValue {
                value_type: None,
                value: Some(InboundValueVariant::BoolValue(true)),
            },
            &CffiHandleTable::new(),
        )
        .unwrap();
        assert_eq!(decoded, BexExternalValue::Bool(true));
    }

    #[test]
    fn portable_prompt_with_media_decodes_as_owned_adts() {
        use crate::baml_bridge::cffi::{
            BamlValuePromptAstMessage, BamlValuePromptAstMultiple, BamlValuePromptAstSimpleMultiple,
        };

        let image = BamlValueMedia {
            media: MediaTypeEnum::Image as i32,
            mime_type: Some("image/png".to_string()),
            value: Some(baml_value_media::Value::Base64("aW1hZ2U=".to_string())),
        };
        let content = BamlValuePromptAstSimple {
            value: Some(PromptAstSimpleVariant::Multiple(
                BamlValuePromptAstSimpleMultiple {
                    items: vec![
                        BamlValuePromptAstSimple {
                            value: Some(PromptAstSimpleVariant::String("look: ".to_string())),
                        },
                        BamlValuePromptAstSimple {
                            value: Some(PromptAstSimpleVariant::Media(image)),
                        },
                    ],
                },
            )),
        };
        let prompt = BamlValuePromptAst {
            value: Some(PromptAstVariant::Multiple(BamlValuePromptAstMultiple {
                items: vec![BamlValuePromptAst {
                    value: Some(PromptAstVariant::Message(BamlValuePromptAstMessage {
                        role: "user".to_string(),
                        content: Some(content),
                        metadata_as_json: "{\"cache\":true}".to_string(),
                    })),
                }],
            })),
        };

        for _ in 0..2 {
            let decoded = inbound_to_external(
                InboundValue {
                    value_type: None,
                    value: Some(InboundValueVariant::PromptAstValue(prompt.clone())),
                },
                &CffiHandleTable::new(),
            )
            .unwrap();
            let BexExternalValue::Adt(BexExternalAdt::PromptAst(ast)) = decoded else {
                panic!("expected a prompt AST payload")
            };
            let messages = ast.to_structured_messages();
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0].0, "user");
            assert_eq!(messages[0].2["cache"], true);
            let PromptAstSimple::Multiple(parts) = messages[0].1.as_ref() else {
                panic!("expected ordered text and media parts")
            };
            let PromptAstSimple::Media(media) = parts[1].as_ref() else {
                panic!("expected media in the second prompt part")
            };
            assert_eq!(media.kind, MediaKind::Image);
            assert_eq!(media.mime_type().as_deref(), Some("image/png"));
            assert_eq!(media.base64(), "aW1hZ2U=");
        }
    }

    #[test]
    fn typed_literal_preserves_identity_beyond_payload_shape() {
        let literal = RuntimeTy::Literal(
            baml_type::Literal::String("draft".to_string()),
            baml_type::Freshness::Regular,
            baml_type::TyAttr::default(),
        );
        let decoded = inbound_to_external(
            typed_input(
                &literal,
                InboundValueVariant::StringValue("draft".to_string()),
            ),
            &CffiHandleTable::new(),
        )
        .unwrap();
        let BexExternalValue::Union { value, metadata } = decoded else {
            panic!("expected typed-value carrier")
        };
        assert_eq!(*value, BexExternalValue::String("draft".into()));
        assert_eq!(metadata.selected_option, literal);
    }

    #[test]
    fn typed_empty_container_preserves_exact_value_type() {
        let list_type = RuntimeTy::list(RuntimeTy::int());
        let decoded = inbound_to_external(
            typed_input(
                &list_type,
                InboundValueVariant::ListValue(InboundListValue { values: vec![] }),
            ),
            &CffiHandleTable::new(),
        )
        .unwrap();
        let BexExternalValue::Union { value, metadata } = decoded else {
            panic!("expected typed-value carrier")
        };
        assert!(matches!(*value, BexExternalValue::Array { items, .. } if items.is_empty()));
        assert_eq!(metadata.selected_option, list_type);
    }

    #[test]
    fn root_union_value_type_is_rejected() {
        let error = inbound_to_external(
            typed_input(
                &RuntimeTy::union([RuntimeTy::int(), RuntimeTy::string()]),
                InboundValueVariant::IntValue(7),
            ),
            &CffiHandleTable::new(),
        )
        .unwrap_err();

        assert!(matches!(
            error,
            CtypesError::InvalidInboundValueTypeRootUnion
        ));
    }

    #[test]
    fn root_optional_value_type_is_rejected() {
        let error = inbound_to_external(
            typed_input(
                &RuntimeTy::optional(RuntimeTy::int()),
                InboundValueVariant::IntValue(7),
            ),
            &CffiHandleTable::new(),
        )
        .unwrap_err();

        assert!(matches!(
            error,
            CtypesError::InvalidInboundValueTypeRootUnion
        ));
    }

    #[test]
    fn nested_union_value_type_is_allowed() {
        let list_type = RuntimeTy::list(RuntimeTy::union([RuntimeTy::int(), RuntimeTy::string()]));
        let decoded = inbound_to_external(
            typed_input(
                &list_type,
                InboundValueVariant::ListValue(InboundListValue { values: vec![] }),
            ),
            &CffiHandleTable::new(),
        )
        .unwrap();

        let BexExternalValue::Union { metadata, .. } = decoded else {
            panic!("expected typed-value carrier")
        };
        assert_eq!(metadata.selected_option, list_type);
    }

    #[test]
    fn untyped_empty_containers_remain_shape_only() {
        let table = CffiHandleTable::new();
        let list = InboundValue {
            value_type: None,
            value: Some(InboundValueVariant::ListValue(InboundListValue {
                values: vec![],
            })),
        };
        let decoded = inbound_to_external(list, &table).unwrap();
        assert!(matches!(
            decoded,
            BexExternalValue::Array { items, .. } if items.is_empty()
        ));

        let map = InboundValue {
            value_type: None,
            value: Some(InboundValueVariant::MapValue(InboundMapValue {
                entries: vec![],
            })),
        };
        let decoded = inbound_to_external(map, &table).unwrap();
        assert!(matches!(
            decoded,
            BexExternalValue::Map { entries, .. } if entries.is_empty()
        ));
    }

    #[test]
    fn decode_inbound_host_value_callable() {
        let table = CffiHandleTable::new();
        let handle = BamlHandle {
            key: 999,
            handle_type: BamlHandleType::HostValueCallable as i32,
        };
        let inbound = InboundValue {
            value_type: None,
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
            value_type: None,
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
            value_type: None,
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
            value_type: None,
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
            value_type: None,
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
            value_type: None,
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
            value_type: None,
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

    /// A sparse node annotation carries nominal generic class identity without
    /// duplicating it on the class payload.
    #[test]
    fn class_value_type_annotation_preserves_generic_identity() {
        use crate::baml_bridge::cffi::InboundClassValue;
        let class_type = RuntimeTy::Class(
            baml_type::TypeName::local(baml_type::Name::new("GenericBox")),
            vec![RuntimeTy::int()],
            baml_type::TyAttr::default(),
        );
        let decoded = inbound_to_external(
            typed_input(
                &class_type,
                InboundValueVariant::ClassValue(InboundClassValue { fields: vec![] }),
            ),
            &CffiHandleTable::new(),
        )
        .expect("decode succeeds");
        let BexExternalValue::Union { value, metadata } = decoded else {
            panic!("expected sparse type carrier")
        };
        assert_eq!(metadata.selected_option, class_type);
        assert!(matches!(
            *value,
            BexExternalValue::Instance { class_name, type_args, .. }
                if class_name.is_empty() && type_args.is_empty()
        ));
    }
}
