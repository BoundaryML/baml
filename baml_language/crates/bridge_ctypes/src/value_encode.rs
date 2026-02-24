//! `BexExternalValue` -> `BamlOutboundValue` conversion.

use baml_type::Literal;
use bex_project::{BexExternalAdt, BexExternalValue, Ty};

use crate::{
    baml::cffi::{
        BamlFieldType, BamlFieldTypeBool, BamlFieldTypeFloat, BamlFieldTypeInt, BamlFieldTypeList,
        BamlFieldTypeLiteral, BamlFieldTypeMap, BamlFieldTypeMedia, BamlFieldTypeNull,
        BamlFieldTypeOptional, BamlFieldTypeString, BamlFieldTypeUnionVariant, BamlHandle,
        BamlOutboundMapEntry, BamlOutboundValue, BamlTypeName, BamlTypeNamespace, BamlValueClass,
        BamlValueEnum, BamlValueList, BamlValueMap, BamlValueUnionVariant,
        baml_field_type::Type as FieldType, baml_outbound_value::Value as BamlValueVariant,
    },
    error::CtypesError,
    handle_table::{HandleTableOptions, HandleTableValue},
};

/// Convert `BexExternalValue` to `BamlOutboundValue` for FFI return.
///
/// Opaque types (Handle, Resource, `FunctionRef`, Adt) are inserted into `handle_table`
/// and encoded as `BamlHandle` messages so the host can round-trip them back.
pub fn external_to_baml_value(
    value: &BexExternalValue,
    options: &HandleTableOptions,
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
                .map(|v| external_to_baml_value(v, options))
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
                    value: Some(external_to_baml_value(val, options)?),
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
                    value: Some(external_to_baml_value(val, options)?),
                });
            }
            Some(BamlValueVariant::ClassValue(BamlValueClass {
                name: Some(BamlTypeName {
                    namespace: BamlTypeNamespace::Types as i32,
                    name: class_name.clone(),
                }),
                fields: baml_fields,
            }))
        }
        BexExternalValue::Variant {
            enum_name,
            variant_name,
        } => Some(BamlValueVariant::EnumValue(BamlValueEnum {
            name: Some(BamlTypeName {
                namespace: BamlTypeNamespace::Types as i32,
                name: enum_name.clone(),
            }),
            value: variant_name.clone(),
            is_dynamic: false,
        })),
        BexExternalValue::Union { value, metadata } => {
            let inner = external_to_baml_value(value, options)?;
            Some(BamlValueVariant::UnionVariantValue(Box::new(
                BamlValueUnionVariant {
                    name: metadata.name.as_ref().map(|n| BamlTypeName {
                        namespace: BamlTypeNamespace::Types as i32,
                        name: n.clone(),
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

        // All opaque types → insert into handle table, encode as BamlHandle.
        BexExternalValue::Handle(_)
        | BexExternalValue::Resource(_)
        | BexExternalValue::FunctionRef { .. }
        | BexExternalValue::Adt(_) => {
            let table_value = HandleTableValue::try_from(value.clone()).map_err(|e| {
                CtypesError::InternalError(format!("handle table insertion failed: {e}"))
            })?;
            let ht = table_value.handle_type();
            let key = options.table.insert(table_value);
            Some(BamlValueVariant::HandleValue(BamlHandle {
                key,
                handle_type: ht as i32,
            }))
        }
    };

    Ok(BamlOutboundValue { value: variant })
}

fn literal_to_field_type_literal(lit: &Literal) -> BamlFieldTypeLiteral {
    use crate::baml::cffi::{
        BamlLiteralBool, BamlLiteralInt, BamlLiteralString,
        baml_field_type_literal::Literal as LiteralOneof,
    };
    let literal = match lit {
        Literal::String(s) => LiteralOneof::StringLiteral(BamlLiteralString { value: s.clone() }),
        Literal::Int(i) => LiteralOneof::IntLiteral(BamlLiteralInt { value: *i }),
        Literal::Bool(b) => LiteralOneof::BoolLiteral(BamlLiteralBool { value: *b }),
        Literal::Float(s) => LiteralOneof::StringLiteral(BamlLiteralString { value: s.clone() }),
    };
    BamlFieldTypeLiteral {
        literal: Some(literal),
    }
}

fn media_kind_to_proto_enum(kind: bex_project::MediaKind) -> crate::baml::cffi::MediaTypeEnum {
    use crate::baml::cffi::MediaTypeEnum as E;
    match kind {
        bex_project::MediaKind::Image => E::Image,
        bex_project::MediaKind::Audio => E::Audio,
        bex_project::MediaKind::Video => E::Video,
        bex_project::MediaKind::Pdf => E::Pdf,
        bex_project::MediaKind::Generic => E::Other,
    }
}

fn bex_media_to_proto_media(media: &bex_project::MediaValue) -> crate::baml::cffi::BamlValueMedia {
    use crate::baml::cffi::{BamlValueMedia, baml_value_media::Value as BamlValueMediaValue};
    BamlValueMedia {
        media: media_kind_to_proto_enum(media.kind).into(),
        mime_type: media.mime_type.clone(),
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
) -> crate::baml::cffi::BamlValuePromptAst {
    bex_prompt_ast_to_proto_prompt_ast(p.as_ref())
}

/// Adapter so we can use `.map(arc_prompt_ast_simple_to_proto)` instead of a closure (PR review).
fn arc_prompt_ast_simple_to_proto(
    s: &std::sync::Arc<bex_project::PromptAstSimple>,
) -> crate::baml::cffi::BamlValuePromptAstSimple {
    bex_prompt_ast_simple_to_proto_prompt_ast_simple(s.as_ref())
}

fn bex_prompt_ast_to_proto_prompt_ast(
    prompt_ast: &bex_project::PromptAst,
) -> crate::baml::cffi::BamlValuePromptAst {
    use crate::baml::cffi::{
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
) -> crate::baml::cffi::BamlValuePromptAstSimple {
    use crate::baml::cffi::{
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

fn ty_to_field_type(ty: &Ty) -> BamlFieldType {
    let field_type = match ty {
        Ty::Null => Some(FieldType::NullType(BamlFieldTypeNull {})),
        Ty::Int => Some(FieldType::IntType(BamlFieldTypeInt {})),
        Ty::Float => Some(FieldType::FloatType(BamlFieldTypeFloat {})),
        Ty::Bool => Some(FieldType::BoolType(BamlFieldTypeBool {})),
        Ty::String => Some(FieldType::StringType(BamlFieldTypeString {})),
        Ty::List(inner) => Some(FieldType::ListType(Box::new(BamlFieldTypeList {
            item_type: Some(Box::new(ty_to_field_type(inner))),
        }))),
        Ty::Map { key, value } => Some(FieldType::MapType(Box::new(BamlFieldTypeMap {
            key_type: Some(Box::new(ty_to_field_type(key))),
            value_type: Some(Box::new(ty_to_field_type(value))),
        }))),
        Ty::Class(tn) => Some(FieldType::ClassType(
            crate::baml::cffi::BamlFieldTypeClass {
                name: Some(BamlTypeName {
                    namespace: BamlTypeNamespace::Types as i32,
                    name: tn.display_name.to_string(),
                }),
            },
        )),
        Ty::Enum(tn) => Some(FieldType::EnumType(crate::baml::cffi::BamlFieldTypeEnum {
            name: tn.display_name.to_string(),
        })),
        Ty::Union(_) => Some(FieldType::UnionVariantType(BamlFieldTypeUnionVariant {
            name: None,
        })),
        Ty::Optional(inner) => Some(FieldType::OptionalType(Box::new(BamlFieldTypeOptional {
            value: Some(Box::new(ty_to_field_type(inner))),
        }))),
        Ty::Media(kind) => Some(FieldType::MediaType(BamlFieldTypeMedia {
            media: media_kind_to_proto_enum(*kind).into(),
        })),
        Ty::Literal(lit) => Some(FieldType::LiteralType(literal_to_field_type_literal(lit))),
        Ty::Opaque(tn) => {
            unreachable!("runtime-only {tn} should not reach FFI type encoding")
        }
        Ty::TypeAlias(_)
        | Ty::Function { .. }
        | Ty::Void
        | Ty::WatchAccessor(_)
        | Ty::BuiltinUnknown => unreachable!("compiler-only variant should not reach FFI"),
    };

    BamlFieldType { r#type: field_type }
}
