//! `BexExternalValue` -> `BamlOutboundValue` conversion.

use baml_type::Literal;
use bex_project::{BexExternalAdt, BexExternalValue, Ty};

use crate::{
    baml_core::cffi::{
        BamlOutboundHandle, BamlOutboundMapEntry, BamlOutboundValue, BamlTy, BamlTyBool,
        BamlTyFloat, BamlTyGenericArg, BamlTyInt, BamlTyList, BamlTyLiteral, BamlTyMap,
        BamlTyMedia, BamlTyName, BamlTyNull, BamlTyOptional, BamlTyString, BamlTyUint8Array,
        BamlTyUnionVariant, BamlTyUnknown, BamlValueClass, BamlValueEnum, BamlValueList,
        BamlValueMap, BamlValueUnionVariant, baml_outbound_value::Value as BamlValueVariant,
        baml_ty::Type as FieldType,
    },
    error::CtypesError,
    handle_table::{BexRustData, CffiHandleTableEntry, CffiHandleTableOptions},
};

/// Convert `BexExternalValue` to `BamlOutboundValue` for FFI return.
///
/// Opaque types (Handle, Resource, `FunctionRef`, Adt) are inserted into `handle_table`
/// and encoded as `BamlOutboundHandle` messages so the host can round-trip them back.
/// For `BexExternalAdt::TaggedHeapHandle { ty, .. }` the wire `name` is projected
/// from `ty` so the host sees the underlying class FQN + concrete generic args.
pub fn external_to_outbound(
    value: &BexExternalValue,
    options: &CffiHandleTableOptions,
) -> Result<BamlOutboundValue, CtypesError> {
    let variant = match value {
        BexExternalValue::Null => None,
        BexExternalValue::Int(i) => Some(BamlValueVariant::IntValue(*i)),
        BexExternalValue::Float(f) => Some(BamlValueVariant::FloatValue(*f)),
        BexExternalValue::Bool(b) => Some(BamlValueVariant::BoolValue(*b)),
        BexExternalValue::String(s) => Some(BamlValueVariant::StringValue(s.clone())),
        BexExternalValue::Array {
            items,
            element_type,
        } => {
            let values: Result<Vec<BamlOutboundValue>, CtypesError> = items
                .iter()
                .map(|v| external_to_outbound(v, options))
                .collect();
            Some(BamlValueVariant::ListValue(BamlValueList {
                item_type: Some(ty_to_field_type(element_type)),
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
                key_type: Some(ty_to_field_type(key_type)),
                value_type: Some(ty_to_field_type(value_type)),
                entries: baml_entries,
            }))
        }
        BexExternalValue::Instance { class_name, fields } => {
            let mut baml_fields = Vec::new();
            for (key, val) in fields {
                baml_fields.push(BamlOutboundMapEntry {
                    key: key.clone(),
                    value: Some(external_to_outbound(val, options)?),
                });
            }
            Some(BamlValueVariant::ClassValue(BamlValueClass {
                name: Some(BamlTyName {
                    name: class_name.clone(),
                    generic_args: vec![],
                }),
                fields: baml_fields,
            }))
        }
        BexExternalValue::Variant {
            enum_name,
            variant_name,
        } => Some(BamlValueVariant::EnumValue(BamlValueEnum {
            name: Some(BamlTyName {
                name: enum_name.clone(),
                generic_args: vec![],
            }),
            value: variant_name.clone(),
            is_dynamic: false,
        })),
        BexExternalValue::Union { value, metadata } => {
            let inner = external_to_outbound(value, options)?;
            Some(BamlValueVariant::UnionVariantValue(Box::new(
                BamlValueUnionVariant {
                    name: metadata.name.as_ref().map(|n| BamlTyName {
                        name: n.clone(),
                        generic_args: vec![],
                    }),
                    is_optional: metadata.is_optional,
                    is_single_pattern: metadata.is_single_pattern,
                    self_type: Some(ty_to_field_type(&metadata.union_type)),
                    value_option_name: format!("{}", metadata.selected_option),
                    value: Some(Box::new(inner)),
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
                name: Some(empty_ty_name()),
            }))
        }

        // All opaque types → insert into handle table, encode as BamlOutboundHandle.
        BexExternalValue::Handle(_)
        | BexExternalValue::FunctionRef { .. }
        | BexExternalValue::Adt(_) => {
            // For `TaggedHeapHandle` the underlying class FQN + concrete generic
            // args ride on the wire so the host can pick a typed wrapper.
            // Other ADTs are discriminated purely by `handle_type` and emit an
            // empty `name`. Read `ty` directly off the variant — no heap permit
            // needed (plan 23a §"Outbound encode").
            let name = match value {
                BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle { ty, .. }) => {
                    ty_to_baml_ty_name(ty)
                }
                _ => empty_ty_name(),
            };
            let table_value = CffiHandleTableEntry::try_from(value.clone()).map_err(|e| {
                CtypesError::InternalError(format!("handle table insertion failed: {e}"))
            })?;
            let ht = table_value.handle_type();
            let key = options.table.insert(table_value);
            Some(BamlValueVariant::HandleValue(BamlOutboundHandle {
                key,
                handle_type: ht as i32,
                name: Some(name),
            }))
        }
    };

    Ok(BamlOutboundValue { value: variant })
}

fn empty_ty_name() -> BamlTyName {
    BamlTyName {
        name: String::new(),
        generic_args: vec![],
    }
}

/// Project a `Ty` to its `BamlTyName` wire shape.
///
/// Canonically called with `Ty::Class(tn, args, _)` from `TaggedHeapHandle.ty`,
/// where the class FQN becomes `BamlTyName.name` and each `arg` becomes a
/// positional `BamlTyGenericArg`. The arg `name` field is left empty: the host
/// only consults `name.name` to dispatch to the right wrapper and walks
/// `generic_args[i].ty` positionally for parameterization. See plan 23a
/// §"Outbound encode" + open question 2.
fn ty_to_baml_ty_name(ty: &Ty) -> BamlTyName {
    match ty {
        Ty::Class(tn, args, _) => BamlTyName {
            name: tn.display_name.to_string(),
            generic_args: args
                .iter()
                .map(|arg| BamlTyGenericArg {
                    name: String::new(),
                    ty: Some(ty_to_field_type(arg)),
                })
                .collect(),
        },
        _ => BamlTyName {
            name: format!("{ty}"),
            generic_args: vec![],
        },
    }
}

fn literal_to_field_type_literal(lit: &Literal) -> BamlTyLiteral {
    use crate::baml_core::cffi::{
        BamlLiteralBool, BamlLiteralInt, BamlLiteralString,
        baml_ty_literal::Literal as LiteralOneof,
    };
    let literal = match lit {
        Literal::String(s) => LiteralOneof::StringLiteral(BamlLiteralString { value: s.clone() }),
        Literal::Int(i) => LiteralOneof::IntLiteral(BamlLiteralInt { value: *i }),
        Literal::Bool(b) => LiteralOneof::BoolLiteral(BamlLiteralBool { value: *b }),
        Literal::Float(s) => LiteralOneof::StringLiteral(BamlLiteralString { value: s.clone() }),
    };
    BamlTyLiteral {
        literal: Some(literal),
    }
}

fn media_kind_to_proto_enum(kind: bex_project::MediaKind) -> crate::baml_core::cffi::MediaTypeEnum {
    use crate::baml_core::cffi::MediaTypeEnum as E;
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
) -> crate::baml_core::cffi::BamlValueMedia {
    use crate::baml_core::cffi::{BamlValueMedia, baml_value_media::Value as BamlValueMediaValue};
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
) -> crate::baml_core::cffi::BamlValuePromptAst {
    bex_prompt_ast_to_proto_prompt_ast(p.as_ref())
}

/// Adapter so we can use `.map(arc_prompt_ast_simple_to_proto)` instead of a closure (PR review).
fn arc_prompt_ast_simple_to_proto(
    s: &std::sync::Arc<bex_project::PromptAstSimple>,
) -> crate::baml_core::cffi::BamlValuePromptAstSimple {
    bex_prompt_ast_simple_to_proto_prompt_ast_simple(s.as_ref())
}

fn bex_prompt_ast_to_proto_prompt_ast(
    prompt_ast: &bex_project::PromptAst,
) -> crate::baml_core::cffi::BamlValuePromptAst {
    use crate::baml_core::cffi::{
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
) -> crate::baml_core::cffi::BamlValuePromptAstSimple {
    use crate::baml_core::cffi::{
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

fn ty_to_field_type(ty: &Ty) -> BamlTy {
    let field_type = match ty {
        Ty::Null { .. } => Some(FieldType::NullType(BamlTyNull {})),
        Ty::Int { .. } => Some(FieldType::IntType(BamlTyInt {})),
        Ty::Float { .. } => Some(FieldType::FloatType(BamlTyFloat {})),
        Ty::Bool { .. } => Some(FieldType::BoolType(BamlTyBool {})),
        Ty::String { .. } => Some(FieldType::StringType(BamlTyString {})),
        Ty::List(inner, _) => Some(FieldType::ListType(Box::new(BamlTyList {
            item_type: Some(Box::new(ty_to_field_type(inner))),
        }))),
        Ty::Map { key, value, .. } => Some(FieldType::MapType(Box::new(BamlTyMap {
            key_type: Some(Box::new(ty_to_field_type(key))),
            value_type: Some(Box::new(ty_to_field_type(value))),
        }))),
        Ty::Class(tn, _, _) => Some(FieldType::ClassType(crate::baml_core::cffi::BamlTyClass {
            name: Some(BamlTyName {
                name: tn.display_name.to_string(),
                generic_args: vec![],
            }),
        })),
        Ty::EnumVariant(tn, ..) | Ty::Enum(tn, _) => {
            Some(FieldType::EnumType(crate::baml_core::cffi::BamlTyEnum {
                name: tn.display_name.to_string(),
            }))
        }
        Ty::Union(_, _) => Some(FieldType::UnionVariantType(BamlTyUnionVariant {
            name: None,
        })),
        Ty::Optional(inner, _) => Some(FieldType::OptionalType(Box::new(BamlTyOptional {
            value: Some(Box::new(ty_to_field_type(inner))),
        }))),
        Ty::Media(kind, _) => Some(FieldType::MediaType(BamlTyMedia {
            media: media_kind_to_proto_enum(*kind).into(),
        })),
        Ty::Literal(lit, _) => Some(FieldType::LiteralType(literal_to_field_type_literal(lit))),
        Ty::Opaque(tn, _) => {
            unreachable!("runtime-only {tn} should not reach FFI type encoding")
        }
        Ty::Uint8Array { .. } => Some(FieldType::Uint8arrayType(BamlTyUint8Array {})),
        // BuiltinUnknown is used for dynamic types (e.g., map values, array elements)
        // when the element type isn't known at compile time.
        Ty::BuiltinUnknown { .. } => Some(FieldType::UnknownType(BamlTyUnknown {})),
        Ty::TypeAlias(_, _)
        | Ty::Future(..)
        | Ty::Function { .. }
        | Ty::Void { .. }
        | Ty::WatchAccessor(_, _) => {
            unreachable!("compiler-only variant should not reach FFI: {ty:?}")
        }
    };

    BamlTy { r#type: field_type }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bex_project::{BexExternalValue, MediaContent, MediaValue, PromptAst, PromptAstSimple};

    use super::*;
    use crate::baml_core::cffi::{BamlHandleType, baml_outbound_value::Value as BamlValueVariant};

    fn extract_handle(out: BamlOutboundValue) -> BamlOutboundHandle {
        match out.value {
            Some(BamlValueVariant::HandleValue(h)) => h,
            other => panic!("expected HandleValue, got {other:?}"),
        }
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
}
