//! `BexExternalValue` -> `BamlOutboundValue` conversion.

use bex_project::{BexExternalAdt, BexExternalValue, RuntimeTy, selected_arm_equal};
use indexmap::IndexMap;

use crate::{
    baml_bridge::cffi::{
        BamlOutboundHandle, BamlOutboundMapEntry, BamlOutboundValue, BamlToHostArg, BamlToHostCall,
        BamlValueClass, BamlValueEnum, BamlValueList, BamlValueMap, BamlValueUnionVariant,
        baml_outbound_value::Value as BamlValueVariant,
    },
    error::CtypesError,
    handle_table::{BexRustData, CffiHandleTableEntry, CffiHandleTableOptions},
};

/// Convert `BexExternalValue` to `BamlOutboundValue` for FFI return.
///
/// Opaque types (Handle, Resource, `FunctionRef`, Adt) are inserted into `handle_table`
/// and encoded as `BamlOutboundHandle` messages so the host can round-trip them back.
/// For `BexExternalAdt::TaggedHeapHandle { ty, .. }` the underlying type is encoded
/// as a full `BamlTy` on the handle's `ty` field so the host sees the class FQN +
/// concrete generic args (and an interface keeps its bindings).
pub fn external_to_outbound(
    value: &BexExternalValue,
    options: &CffiHandleTableOptions,
) -> Result<BamlOutboundValue, CtypesError> {
    let variant = match value {
        BexExternalValue::Null => None,
        BexExternalValue::Int(i) => Some(BamlValueVariant::IntValue(*i)),
        // Hex / base sixteen on the wire (see Phase 10 of the bigint plan).
        // Power-of-two-base parsing is SIMD-friendly; `num-bigint`'s
        // `LowerHex` impl handles the leading-minus sign convention.
        BexExternalValue::Bigint(bi) => Some(BamlValueVariant::BigintValue(format!("{bi:x}"))),
        BexExternalValue::Float(f) => Some(BamlValueVariant::FloatValue(*f)),
        BexExternalValue::Bool(b) => Some(BamlValueVariant::BoolValue(*b)),
        BexExternalValue::String(s) => Some(BamlValueVariant::StringValue(s.to_string())),
        BexExternalValue::Array {
            items,
            element_type,
        } => {
            let values: Result<Vec<BamlOutboundValue>, CtypesError> = items
                .iter()
                .map(|v| external_to_outbound(v, options))
                .collect();
            Some(BamlValueVariant::ListValue(BamlValueList {
                item_type: Some(crate::ty_encode::runtime_ty_to_proto_ty(element_type)),
                items: values?,
            }))
        }
        BexExternalValue::Map {
            entries,
            key_type,
            value_type,
        } => {
            let mut baml_entries = Vec::new();
            for (key, val) in entries {
                baml_entries.push(BamlOutboundMapEntry {
                    key: key.clone(),
                    value: Some(external_to_outbound(val, options)?),
                });
            }
            Some(BamlValueVariant::MapValue(BamlValueMap {
                key_type: Some(crate::ty_encode::runtime_ty_to_proto_ty(key_type)),
                value_type: Some(crate::ty_encode::runtime_ty_to_proto_ty(value_type)),
                entries: baml_entries,
            }))
        }
        BexExternalValue::Instance {
            class_name,
            fields,
            type_args,
        } => {
            let mut baml_fields = Vec::new();
            for (key, val) in fields {
                baml_fields.push(BamlOutboundMapEntry {
                    key: key.clone(),
                    value: Some(external_to_outbound(val, options)?),
                });
            }
            // Carry a generic instance's concrete class type args (De Bruijn
            // order) as positional `type_args` so the host can reconstruct the
            // parameterized type (Python: `cls[args]`; Node: the `$types` field).
            // Non-generic instances have empty `type_args`.
            Some(BamlValueVariant::ClassValue(BamlValueClass {
                name: class_name.clone(),
                fields: baml_fields,
                type_args: type_args
                    .iter()
                    .map(crate::ty_encode::runtime_ty_to_proto_ty)
                    .collect(),
            }))
        }
        BexExternalValue::Variant {
            enum_name,
            variant_name,
        } => Some(BamlValueVariant::EnumValue(BamlValueEnum {
            name: enum_name.clone(),
            value: variant_name.clone(),
            is_dynamic: false,
        })),
        BexExternalValue::Union { value, metadata } => {
            let selected_option_index =
                selected_union_option_index(&metadata.union_type, &metadata.selected_option)?;
            let inner = external_to_outbound(value, options)?;
            Some(BamlValueVariant::UnionVariantValue(Box::new(
                BamlValueUnionVariant {
                    name: metadata.name.clone().unwrap_or_default(),
                    is_optional: metadata.is_optional,
                    is_single_pattern: metadata.is_single_pattern,
                    self_type: Some(crate::ty_encode::runtime_ty_to_proto_ty(
                        &metadata.union_type,
                    )),
                    value_option_name: format!("{}", metadata.selected_option),
                    value: Some(Box::new(inner)),
                    selected_option_index: Some(selected_option_index),
                },
            )))
        }

        BexExternalValue::Adt(BexExternalAdt::Media(media)) if options.serialize_media => Some(
            BamlValueVariant::MediaValue(bex_media_to_proto_media(media)),
        ),

        BexExternalValue::Adt(BexExternalAdt::PromptAst(prompt_ast))
            if options.serialize_prompt_ast =>
        {
            Some(BamlValueVariant::PromptAstValue(
                bex_prompt_ast_to_proto_prompt_ast(prompt_ast),
            ))
        }
        BexExternalValue::Uint8Array(bytes) => {
            Some(BamlValueVariant::Uint8arrayValue(bytes.clone()))
        }
        BexExternalValue::RustData(arc) => {
            if let Some(converted) = bex_project::try_convert_rust_data(arc) {
                return external_to_outbound(&converted, options);
            }
            let table_value = CffiHandleTableEntry::RustData(BexRustData(arc.clone()));
            let ht = table_value.handle_type();
            let key = options.table.insert(table_value);
            Some(BamlValueVariant::HandleValue(BamlOutboundHandle {
                key,
                handle_type: ht as i32,
                ty: None,
            }))
        }

        // Host-value handles do NOT live in HANDLE_TABLE. Encode directly using
        // the host-side key; drop semantics are preserved by the Arc
        // (HostValueArc::drop fires HostReleaseFn on last drop).
        BexExternalValue::HostValue(arc) => {
            use crate::baml_bridge::cffi::BamlHandleType;
            let ht = match arc.kind {
                bex_project::HostValueKind::Callable => BamlHandleType::HostValueCallable as i32,
                bex_project::HostValueKind::Opaque => BamlHandleType::HostValueOpaque as i32,
            };
            Some(BamlValueVariant::HandleValue(BamlOutboundHandle {
                key: arc.key,
                handle_type: ht,
                ty: None,
            }))
        }

        // A reflected BAML type returned as a value (`type.of<T>()`)
        // crosses the boundary as a first-class `Ty`, sharing the inbound
        // representation. Must precede the opaque-ADT catch-all, which would
        // otherwise box it into a handle.
        BexExternalValue::Adt(BexExternalAdt::Type(rt)) => Some(BamlValueVariant::TyValue(
            crate::ty_encode::runtime_ty_to_proto_ty(rt),
        )),
        // A live handle is an engine capability, not data: only the portable
        // definitions cross a process (BEP-066 H-4).
        BexExternalValue::Adt(BexExternalAdt::TypeDef(definition)) => {
            Some(BamlValueVariant::TyDefValue(
                crate::ty_encode::portable_type_def_to_proto(definition.def()),
            ))
        }

        // All opaque types → insert into handle table, encode as BamlOutboundHandle.
        BexExternalValue::Handle(_)
        | BexExternalValue::FunctionRef { .. }
        | BexExternalValue::Adt(_) => {
            // For `TaggedHeapHandle` the underlying type rides on the wire as a
            // full `BamlTy` so the host can pick a typed wrapper (from the class
            // FQN) and the wire form stays faithful (an interface keeps its
            // bindings). Other ADTs are discriminated purely by `handle_type`
            // and leave `ty` unset. Read `ty` directly off the variant — no heap
            // permit needed (plan 23a §"Outbound encode").
            let ty = match value {
                BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle { ty, .. }) => {
                    Some(crate::ty_encode::runtime_ty_to_proto_ty(ty))
                }
                _ => None,
            };
            let table_value = CffiHandleTableEntry::try_from(value.clone()).map_err(|e| {
                CtypesError::InternalError(format!("handle table insertion failed: {e}"))
            })?;
            let ht = table_value.handle_type();
            let key = options.table.insert(table_value);
            Some(BamlValueVariant::HandleValue(BamlOutboundHandle {
                key,
                handle_type: ht as i32,
                ty,
            }))
        }
    };

    Ok(BamlOutboundValue { value: variant })
}

/// Convert `BexExternalValue` to a durable artifact-safe `BamlOutboundValue`.
///
/// Unlike [`external_to_outbound`], this entry point never inserts into the CFFI
/// handle table and never serializes host/process-local handle keys. Opaque or
/// callable values become renderable omission descriptors.
#[allow(
    dead_code,
    reason = "Phase 7 guardrail seam; production host trace serializer API is not designed yet"
)]
pub(crate) fn artifact_safe_external_to_outbound(
    value: &BexExternalValue,
) -> Result<BamlOutboundValue, CtypesError> {
    let variant = match value {
        BexExternalValue::Null => None,
        BexExternalValue::Int(i) => Some(BamlValueVariant::IntValue(*i)),
        BexExternalValue::Bigint(bi) => Some(BamlValueVariant::BigintValue(format!("{bi:x}"))),
        BexExternalValue::Float(f) => Some(BamlValueVariant::FloatValue(*f)),
        BexExternalValue::Bool(b) => Some(BamlValueVariant::BoolValue(*b)),
        BexExternalValue::String(s) => Some(BamlValueVariant::StringValue(s.to_string())),
        BexExternalValue::Array {
            items,
            element_type,
        } => Some(BamlValueVariant::ListValue(BamlValueList {
            item_type: Some(crate::ty_encode::runtime_ty_to_proto_ty(element_type)),
            items: items
                .iter()
                .map(artifact_safe_external_to_outbound)
                .collect::<Result<_, _>>()?,
        })),
        BexExternalValue::Map {
            entries,
            key_type,
            value_type,
        } => Some(BamlValueVariant::MapValue(BamlValueMap {
            key_type: Some(crate::ty_encode::runtime_ty_to_proto_ty(key_type)),
            value_type: Some(crate::ty_encode::runtime_ty_to_proto_ty(value_type)),
            entries: entries
                .iter()
                .map(|(key, val)| {
                    Ok(BamlOutboundMapEntry {
                        key: key.clone(),
                        value: Some(artifact_safe_external_to_outbound(val)?),
                    })
                })
                .collect::<Result<_, CtypesError>>()?,
        })),
        BexExternalValue::Instance {
            class_name,
            fields,
            type_args,
        } => Some(BamlValueVariant::ClassValue(BamlValueClass {
            name: class_name.clone(),
            fields: fields
                .iter()
                .map(|(key, val)| {
                    Ok(BamlOutboundMapEntry {
                        key: key.clone(),
                        value: Some(artifact_safe_external_to_outbound(val)?),
                    })
                })
                .collect::<Result<_, CtypesError>>()?,
            type_args: type_args
                .iter()
                .map(crate::ty_encode::runtime_ty_to_proto_ty)
                .collect(),
        })),
        BexExternalValue::Variant {
            enum_name,
            variant_name,
        } => Some(BamlValueVariant::EnumValue(BamlValueEnum {
            name: enum_name.clone(),
            value: variant_name.clone(),
            is_dynamic: false,
        })),
        BexExternalValue::Union { value, metadata } => {
            let selected_option_index =
                selected_union_option_index(&metadata.union_type, &metadata.selected_option)?;
            let inner = artifact_safe_external_to_outbound(value)?;
            Some(BamlValueVariant::UnionVariantValue(Box::new(
                BamlValueUnionVariant {
                    name: metadata.name.clone().unwrap_or_default(),
                    is_optional: metadata.is_optional,
                    is_single_pattern: metadata.is_single_pattern,
                    self_type: Some(crate::ty_encode::runtime_ty_to_proto_ty(
                        &metadata.union_type,
                    )),
                    value_option_name: format!("{}", metadata.selected_option),
                    value: Some(Box::new(inner)),
                    selected_option_index: Some(selected_option_index),
                },
            )))
        }
        BexExternalValue::Adt(BexExternalAdt::Media(media)) => Some(BamlValueVariant::MediaValue(
            bex_media_to_proto_media(media),
        )),
        BexExternalValue::Adt(BexExternalAdt::PromptAst(prompt_ast)) => Some(
            BamlValueVariant::PromptAstValue(bex_prompt_ast_to_proto_prompt_ast(prompt_ast)),
        ),
        BexExternalValue::Uint8Array(bytes) => {
            Some(BamlValueVariant::Uint8arrayValue(bytes.clone()))
        }
        BexExternalValue::RustData(arc) => {
            if let Some(converted) = bex_project::try_convert_rust_data(arc) {
                return artifact_safe_external_to_outbound(&converted);
            }
            Some(artifact_safe_omission(
                "hostOwnedValue",
                "host-owned rust data",
            ))
        }
        BexExternalValue::Adt(BexExternalAdt::Type(rt)) => Some(BamlValueVariant::TyValue(
            crate::ty_encode::runtime_ty_to_proto_ty(rt),
        )),
        // A live handle is an engine capability, not data: only the portable
        // definitions cross a process (BEP-066 H-4).
        BexExternalValue::Adt(BexExternalAdt::TypeDef(definition)) => {
            Some(BamlValueVariant::TyDefValue(
                crate::ty_encode::portable_type_def_to_proto(definition.def()),
            ))
        }
        BexExternalValue::HostValue(arc) => Some(artifact_safe_omission(
            "hostOwnedValue",
            match arc.kind {
                bex_project::HostValueKind::Callable => "host-owned callable",
                bex_project::HostValueKind::Opaque => "host-owned opaque value",
            },
        )),
        BexExternalValue::Handle(_)
        | BexExternalValue::FunctionRef { .. }
        | BexExternalValue::Adt(_) => Some(artifact_safe_omission(
            "unsupportedValue",
            "process-local handle",
        )),
    };

    Ok(BamlOutboundValue { value: variant })
}

#[allow(
    dead_code,
    reason = "Only used by the currently crate-private artifact-safe encoder seam"
)]
fn artifact_safe_omission(reason: &str, message: &str) -> BamlValueVariant {
    BamlValueVariant::ClassValue(BamlValueClass {
        name: "baml.trace.OmittedValue".to_string(),
        fields: vec![
            BamlOutboundMapEntry {
                key: "reason".to_string(),
                value: Some(BamlOutboundValue {
                    value: Some(BamlValueVariant::StringValue(reason.to_string())),
                }),
            },
            BamlOutboundMapEntry {
                key: "message".to_string(),
                value: Some(BamlOutboundValue {
                    value: Some(BamlValueVariant::StringValue(message.to_string())),
                }),
            },
        ],
        type_args: Vec::new(),
    })
}

fn selected_union_option_index(
    union_type: &RuntimeTy,
    selected_option: &RuntimeTy,
) -> Result<u32, CtypesError> {
    let RuntimeTy::Union(members, _) = union_type else {
        return Err(CtypesError::UnionSelectedTypeNotMember {
            selected: selected_option.to_string(),
            union: union_type.to_string(),
        });
    };
    let Some(index) = members
        .iter()
        .position(|member| selected_arm_equal(member, selected_option))
    else {
        return Err(CtypesError::UnionSelectedTypeNotMember {
            selected: selected_option.to_string(),
            union: union_type.to_string(),
        });
    };
    u32::try_from(index).map_err(|_| {
        CtypesError::InternalError(format!(
            "union selected option index {index} does not fit in uint32"
        ))
    })
}

fn media_kind_to_proto_enum(
    kind: bex_project::MediaKind,
) -> crate::baml_bridge::cffi::MediaTypeEnum {
    use crate::baml_bridge::cffi::MediaTypeEnum as E;
    match kind {
        bex_project::MediaKind::Image => E::Image,
        bex_project::MediaKind::Audio => E::Audio,
        bex_project::MediaKind::Video => E::Video,
        bex_project::MediaKind::Pdf => E::Pdf,
        bex_project::MediaKind::Generic => E::Other,
    }
}

fn bex_media_to_proto_media(
    media: &bex_project::MediaValue,
) -> crate::baml_bridge::cffi::BamlValueMedia {
    use crate::baml_bridge::cffi::{
        BamlValueMedia, baml_value_media::Value as BamlValueMediaValue,
    };
    BamlValueMedia {
        media: media_kind_to_proto_enum(media.kind).into(),
        mime_type: media.mime_type(),
        value: Some(media.read_content(|content| match content {
            bex_project::MediaContent::Url { url, .. } => BamlValueMediaValue::Url(url.clone()),
            bex_project::MediaContent::Base64 { base64_data } => {
                BamlValueMediaValue::Base64(base64_data.clone())
            }
            bex_project::MediaContent::File { file, .. } => BamlValueMediaValue::File(file.clone()),
        })),
    }
}

/// Adapter so we can use `.map(arc_prompt_ast_to_proto)` instead of a closure (PR review).
fn arc_prompt_ast_to_proto(
    p: &std::sync::Arc<bex_project::PromptAst>,
) -> crate::baml_bridge::cffi::BamlValuePromptAst {
    bex_prompt_ast_to_proto_prompt_ast(p.as_ref())
}

/// Adapter so we can use `.map(arc_prompt_ast_simple_to_proto)` instead of a closure (PR review).
fn arc_prompt_ast_simple_to_proto(
    s: &std::sync::Arc<bex_project::PromptAstSimple>,
) -> crate::baml_bridge::cffi::BamlValuePromptAstSimple {
    bex_prompt_ast_simple_to_proto_prompt_ast_simple(s.as_ref())
}

fn bex_prompt_ast_to_proto_prompt_ast(
    prompt_ast: &bex_project::PromptAst,
) -> crate::baml_bridge::cffi::BamlValuePromptAst {
    use crate::baml_bridge::cffi::{
        BamlValuePromptAst, BamlValuePromptAstMessage, BamlValuePromptAstMultiple,
        baml_value_prompt_ast::Value as BamlValuePromptAstValue,
    };
    BamlValuePromptAst {
        value: Some(match prompt_ast {
            bex_project::PromptAst::Simple(simple) => BamlValuePromptAstValue::Simple(
                bex_prompt_ast_simple_to_proto_prompt_ast_simple(simple),
            ),
            bex_project::PromptAst::Message {
                role,
                content,
                metadata,
            } => BamlValuePromptAstValue::Message(BamlValuePromptAstMessage {
                role: role.clone(),
                content: Some(bex_prompt_ast_simple_to_proto_prompt_ast_simple(content)),
                metadata_as_json: metadata.to_string(),
            }),
            bex_project::PromptAst::Vec(vec) => {
                BamlValuePromptAstValue::Multiple(BamlValuePromptAstMultiple {
                    items: vec.iter().map(arc_prompt_ast_to_proto).collect(),
                })
            }
        }),
    }
}

fn bex_prompt_ast_simple_to_proto_prompt_ast_simple(
    simple_prompt_ast: &bex_project::PromptAstSimple,
) -> crate::baml_bridge::cffi::BamlValuePromptAstSimple {
    use crate::baml_bridge::cffi::{
        BamlValuePromptAstSimple, BamlValuePromptAstSimpleMultiple,
        baml_value_prompt_ast_simple::Value as BamlValuePromptAstSimpleValue,
    };
    match simple_prompt_ast {
        bex_project::PromptAstSimple::String(s) => BamlValuePromptAstSimple {
            value: Some(BamlValuePromptAstSimpleValue::String(s.clone())),
        },
        bex_project::PromptAstSimple::Media(media) => BamlValuePromptAstSimple {
            value: Some(BamlValuePromptAstSimpleValue::Media(
                bex_media_to_proto_media(media),
            )),
        },
        bex_project::PromptAstSimple::Multiple(multiple) => BamlValuePromptAstSimple {
            value: Some(BamlValuePromptAstSimpleValue::Multiple(
                BamlValuePromptAstSimpleMultiple {
                    items: multiple
                        .iter()
                        .map(arc_prompt_ast_simple_to_proto)
                        .collect::<Vec<_>>(),
                },
            )),
        },
    }
}

/// Build the engine→host call for a host-callable invocation. The
/// engine has already resolved the call against the callee's declared params:
/// `positional` holds the required (leading) args and `optional` holds the
/// *supplied* optional args keyed by parameter name (omitted optionals are
/// absent, so the host's own default applies). The result is a flat,
/// declared-order list — required args first (`arg_name` empty), then the
/// supplied optionals (tagged `is_optional_arg`, keyed by name) — each value
/// encoded type-rich so the bridge can decode it without the callee type on the
/// wire.
pub fn build_to_host_call(
    positional: &[BexExternalValue],
    optional: &IndexMap<String, BexExternalValue>,
    options: &CffiHandleTableOptions,
) -> Result<BamlToHostCall, CtypesError> {
    let mut args = Vec::with_capacity(positional.len() + optional.len());
    for v in positional {
        args.push(BamlToHostArg {
            value: Some(external_to_outbound(v, options)?),
            // Positional (required) args are taken by position — no name.
            arg_name: String::new(),
            is_optional_arg: false,
        });
    }
    for (name, v) in optional {
        args.push(BamlToHostArg {
            value: Some(external_to_outbound(v, options)?),
            arg_name: name.clone(),
            is_optional_arg: true,
        });
    }
    Ok(BamlToHostCall { args })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use baml_type::{Freshness, Literal, TyAttr};
    use bex_project::{
        BexExternalAdt, BexExternalValue, HostValueArc, HostValueKind, MediaContent, MediaValue,
        PromptAst, PromptAstSimple,
    };

    use super::*;
    use crate::baml_bridge::cffi::{
        BamlHandleType, baml_outbound_value::Value as BamlValueVariant,
    };

    fn extract_handle(out: BamlOutboundValue) -> BamlOutboundHandle {
        match out.value {
            Some(BamlValueVariant::HandleValue(h)) => h,
            other => panic!("expected HandleValue, got {other:?}"),
        }
    }

    fn extract_omission(out: BamlOutboundValue) -> BamlValueClass {
        match out.value {
            Some(BamlValueVariant::ClassValue(class))
                if class.name == "baml.trace.OmittedValue" =>
            {
                class
            }
            other => panic!("expected baml.trace.OmittedValue, got {other:?}"),
        }
    }

    fn extract_union(out: BamlOutboundValue) -> BamlValueUnionVariant {
        match out.value {
            Some(BamlValueVariant::UnionVariantValue(union)) => *union,
            other => panic!("expected UnionVariantValue, got {other:?}"),
        }
    }

    fn ambiguous_numeric_union(selected: RuntimeTy, value: BexExternalValue) -> BexExternalValue {
        BexExternalValue::union(value, [RuntimeTy::int(), RuntimeTy::float()], selected)
    }

    #[test]
    fn outbound_union_encodes_selected_index_for_ambiguous_numeric_arms() {
        let options = CffiHandleTableOptions::for_in_process();
        let int = ambiguous_numeric_union(RuntimeTy::int(), BexExternalValue::Int(1));
        let float = ambiguous_numeric_union(RuntimeTy::float(), BexExternalValue::Float(1.0));

        let encoded_int = extract_union(external_to_outbound(&int, &options).unwrap());
        let encoded_float = extract_union(external_to_outbound(&float, &options).unwrap());

        assert_eq!(encoded_int.selected_option_index, Some(0));
        assert_eq!(encoded_float.selected_option_index, Some(1));
    }

    #[test]
    fn artifact_safe_union_encodes_selected_index() {
        let value = ambiguous_numeric_union(RuntimeTy::float(), BexExternalValue::Float(1.0));
        let encoded = extract_union(artifact_safe_external_to_outbound(&value).unwrap());

        assert_eq!(encoded.selected_option_index, Some(1));
    }

    #[test]
    fn outbound_optional_null_preserves_declared_member_index() {
        let value = BexExternalValue::union(
            BexExternalValue::Null,
            [RuntimeTy::string(), RuntimeTy::null()],
            RuntimeTy::null(),
        );
        let options = CffiHandleTableOptions::for_in_process();
        let encoded = extract_union(external_to_outbound(&value, &options).unwrap());

        assert_eq!(encoded.selected_option_index, Some(1));
        assert!(encoded.is_optional);
    }

    #[test]
    fn outbound_union_rejects_selected_type_absent_from_declared_union() {
        let invalid = ambiguous_numeric_union(RuntimeTy::bool(), BexExternalValue::Bool(true));
        let options = CffiHandleTableOptions::for_in_process();

        let error = external_to_outbound(&invalid, &options).unwrap_err();
        assert!(matches!(
            error,
            CtypesError::UnionSelectedTypeNotMember { .. }
        ));

        let artifact_error = artifact_safe_external_to_outbound(&invalid).unwrap_err();
        assert!(matches!(
            artifact_error,
            CtypesError::UnionSelectedTypeNotMember { .. }
        ));
    }

    #[test]
    fn outbound_union_matches_structurally_equivalent_selected_type() {
        let declared = RuntimeTy::Literal(
            Literal::String("draft".to_string()),
            Freshness::Regular,
            TyAttr::default(),
        );
        let rebuilt = RuntimeTy::Literal(
            Literal::String("draft".to_string()),
            Freshness::Fresh,
            TyAttr::default(),
        );
        assert_ne!(declared, rebuilt);

        let value = BexExternalValue::union(
            BexExternalValue::String("draft".into()),
            [declared],
            rebuilt,
        );
        let encoded = extract_union(
            external_to_outbound(&value, &CffiHandleTableOptions::for_in_process()).unwrap(),
        );
        assert_eq!(encoded.selected_option_index, Some(0));
    }

    #[test]
    fn rust_data_prompt_ast_converts_to_handle() {
        let prompt = Arc::new(PromptAst::Simple(Arc::new(PromptAstSimple::String(
            "hello".to_string(),
        ))));
        let value = BexExternalValue::RustData(prompt);
        let options = CffiHandleTableOptions::for_in_process();
        let handle = extract_handle(external_to_outbound(&value, &options).unwrap());
        assert_eq!(handle.handle_type, BamlHandleType::AdtPromptAst as i32);
    }

    #[test]
    fn rust_data_media_value_converts_to_handle() {
        let media = Arc::new(MediaValue::new(
            bex_project::MediaKind::Image,
            MediaContent::Url {
                url: "https://example.com/img.png".to_string(),
                base64_data: None,
            },
            Some("image/png".to_string()),
        ));
        let value = BexExternalValue::RustData(media);
        let options = CffiHandleTableOptions::for_in_process();
        let handle = extract_handle(external_to_outbound(&value, &options).unwrap());
        assert_eq!(handle.handle_type, BamlHandleType::AdtMediaImage as i32);
    }

    #[test]
    fn rust_data_unknown_type_inserts_handle() {
        let unknown: Arc<dyn std::any::Any + Send + Sync> = Arc::new(42u32);
        let value = BexExternalValue::RustData(unknown);
        let options = CffiHandleTableOptions::for_in_process();
        let handle = extract_handle(external_to_outbound(&value, &options).unwrap());
        assert_eq!(handle.handle_type, BamlHandleType::UntaggedRustData as i32);
    }

    #[test]
    fn encode_outbound_host_value_callable() {
        let arc = HostValueArc::new(42, HostValueKind::Callable);
        let value = BexExternalValue::HostValue(arc);
        let options = CffiHandleTableOptions::for_in_process();
        let encoded = external_to_outbound(&value, &options).expect("encode succeeds");
        let handle = match encoded.value {
            Some(BamlValueVariant::HandleValue(h)) => h,
            other => panic!("unexpected: {other:?}"),
        };
        assert_eq!(handle.key, 42);
        assert_eq!(handle.handle_type, BamlHandleType::HostValueCallable as i32);
        // Must not appear in the handle table.
        assert!(
            options.table.resolve(42).is_none(),
            "HOST_VALUE_CALLABLE must not be inserted into HANDLE_TABLE"
        );
    }

    #[test]
    fn encode_outbound_host_value_opaque() {
        let arc = HostValueArc::new(42, HostValueKind::Opaque);
        let value = BexExternalValue::HostValue(arc);
        let options = CffiHandleTableOptions::for_in_process();
        let encoded = external_to_outbound(&value, &options).expect("encode succeeds");
        let handle = match encoded.value {
            Some(BamlValueVariant::HandleValue(h)) => h,
            other => panic!("unexpected: {other:?}"),
        };
        assert_eq!(handle.key, 42);
        assert_eq!(handle.handle_type, BamlHandleType::HostValueOpaque as i32);
        // Like callables, opaque handles bypass the HANDLE_TABLE.
        assert!(
            options.table.resolve(42).is_none(),
            "HOST_VALUE_OPAQUE must not be inserted into HANDLE_TABLE"
        );
    }

    #[test]
    fn artifact_safe_host_value_callable_omits_key_instead_of_serializing_handle() {
        let arc = HostValueArc::new(42, HostValueKind::Callable);
        let value = BexExternalValue::HostValue(arc);
        let encoded =
            artifact_safe_external_to_outbound(&value).expect("artifact-safe encode succeeds");

        let omission = extract_omission(encoded);
        assert_eq!(omission.fields[0].key, "reason");
        assert_eq!(omission.fields[1].key, "message");
        let Some(BamlValueVariant::StringValue(message)) = omission.fields[1]
            .value
            .as_ref()
            .and_then(|value| value.value.as_ref())
        else {
            panic!("omission message should be a string");
        };
        assert_eq!(message, "host-owned callable");
    }

    #[test]
    fn artifact_safe_unknown_rust_data_omits_without_inserting_handle() {
        let unknown: Arc<dyn std::any::Any + Send + Sync> = Arc::new(42u32);
        let value = BexExternalValue::RustData(unknown);
        let encoded =
            artifact_safe_external_to_outbound(&value).expect("artifact-safe encode succeeds");

        let omission = extract_omission(encoded);
        assert_eq!(omission.name, "baml.trace.OmittedValue");
    }

    #[test]
    fn artifact_safe_function_ref_omits_without_handle_value() {
        let value = BexExternalValue::FunctionRef { global_index: 7 };
        let encoded =
            artifact_safe_external_to_outbound(&value).expect("artifact-safe encode succeeds");

        let omission = extract_omission(encoded);
        assert_eq!(omission.name, "baml.trace.OmittedValue");
    }

    #[test]
    fn artifact_safe_media_stays_durable_data() {
        let media = MediaValue::new(
            bex_project::MediaKind::Image,
            MediaContent::Url {
                url: "https://example.com/img.png".to_string(),
                base64_data: None,
            },
            Some("image/png".to_string()),
        );
        let value = BexExternalValue::Adt(BexExternalAdt::Media(Arc::new(media)));
        let encoded =
            artifact_safe_external_to_outbound(&value).expect("artifact-safe encode succeeds");

        assert!(matches!(
            encoded.value,
            Some(BamlValueVariant::MediaValue(_))
        ));
    }
}
