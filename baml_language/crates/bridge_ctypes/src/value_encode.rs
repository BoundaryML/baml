//! `BexExternalValue` -> `BamlOutboundValue` conversion.

use baml_type::Literal;
use bex_project::{BexExternalAdt, BexExternalValue, MediaContent, Ty};

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
    let mut handles_created = Vec::new();
    let value = external_to_baml_value_inner(value, options, &mut handles_created)?;
    Ok(BamlOutboundValue {
        handles_created,
        value,
    })
}

fn encode_value(
    value: &BexExternalValue,
    options: &HandleTableOptions,
    handles_created: &mut Vec<BamlHandle>,
) -> Result<BamlOutboundValue, CtypesError> {
    let value = external_to_baml_value_inner(value, options, handles_created)?;
    Ok(BamlOutboundValue {
        value,
        ..Default::default()
    })
}

fn encode_entries<'a, I>(
    entries: I,
    options: &HandleTableOptions,
    handles_created: &mut Vec<BamlHandle>,
) -> Result<Vec<BamlOutboundMapEntry>, CtypesError>
where
    I: IntoIterator<Item = (&'a String, &'a BexExternalValue)>,
{
    entries
        .into_iter()
        .map(|(key, val)| {
            let value = encode_value(val, options, handles_created)?;
            Ok(BamlOutboundMapEntry {
                key: key.clone(),
                value: Some(value),
            })
        })
        .collect()
}

fn build_handle(key: u64, handle_type: i32) -> BamlHandle {
    BamlHandle { key, handle_type }
}

fn external_to_baml_value_inner(
    value: &BexExternalValue,
    options: &HandleTableOptions,
    handles_created: &mut Vec<BamlHandle>,
) -> Result<Option<BamlValueVariant>, CtypesError> {
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
                .map(|v| encode_value(v, options, handles_created))
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
            let baml_entries = encode_entries(entries.iter(), options, handles_created)?;
            Some(BamlValueVariant::MapValue(BamlValueMap {
                key_type: Some(ty_to_field_type(key_type)),
                value_type: Some(ty_to_field_type(value_type)),
                entries: baml_entries,
            }))
        }
        BexExternalValue::Instance { class_name, fields } => {
            let baml_fields = encode_entries(fields.iter(), options, handles_created)?;
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
            let inner = encode_value(value, options, handles_created)?;
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

        BexExternalValue::Adt(BexExternalAdt::Media(media))
            if options.serialize_media && should_inline_media(media, options) =>
        {
            Some(BamlValueVariant::MediaValue(bex_media_to_proto_media(
                media,
            )))
        }

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
            let table_value = HandleTableValue::try_from(value).map_err(|e| {
                CtypesError::InternalError(format!("handle table insertion failed: {e}"))
            })?;
            let handle_type = table_value.handle_type() as i32;
            let key = options.table.insert(table_value);
            handles_created.push(build_handle(key, handle_type));
            Some(BamlValueVariant::HandleValue(build_handle(
                key,
                handle_type,
            )))
        }
    };

    Ok(variant)
}

fn should_inline_media(media: &bex_project::MediaValue, options: &HandleTableOptions) -> bool {
    let Some(limit) = options.max_inline_media_bytes else {
        return true;
    };
    media.read_content(|content| match content {
        MediaContent::Base64 { base64_data } => base64_data.len() <= limit,
        MediaContent::Url { .. } | MediaContent::File { .. } => true,
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handle_table::HandleTable;
    use baml_type::MediaKind;
    use bex_project::{BexExternalAdt, BexExternalValue, MediaContent, MediaValue};

    fn options_for_media(
        table: &HandleTable,
        max_inline_media_bytes: usize,
    ) -> HandleTableOptions<'_> {
        HandleTableOptions {
            table,
            serialize_media: true,
            serialize_prompt_ast: false,
            max_inline_media_bytes: Some(max_inline_media_bytes),
        }
    }

    #[test]
    fn handles_created_collects_opaque_values() {
        let table = HandleTable::new();
        let options = HandleTableOptions {
            table: &table,
            serialize_media: false,
            serialize_prompt_ast: false,
            max_inline_media_bytes: None,
        };

        let value = BexExternalValue::FunctionRef { global_index: 1 };
        let out = external_to_baml_value(&value, &options).unwrap();

        assert_eq!(out.handles_created.len(), 1);
        assert!(matches!(out.value, Some(BamlValueVariant::HandleValue(_))));
    }

    #[test]
    fn media_inlines_when_under_limit() {
        let table = HandleTable::new();
        let options = options_for_media(&table, 3);

        let media = MediaValue::new(
            MediaKind::Image,
            MediaContent::Base64 {
                base64_data: "AAA".to_string(),
            },
            None,
        );
        let value = BexExternalValue::Adt(BexExternalAdt::Media(std::sync::Arc::new(media)));
        let out = external_to_baml_value(&value, &options).unwrap();

        assert!(out.handles_created.is_empty());
        assert!(matches!(out.value, Some(BamlValueVariant::MediaValue(_))));
    }

    #[test]
    fn media_falls_back_to_handle_over_limit() {
        let table = HandleTable::new();
        let options = options_for_media(&table, 3);

        let media = MediaValue::new(
            MediaKind::Image,
            MediaContent::Base64 {
                base64_data: "AAAA".to_string(),
            },
            None,
        );
        let value = BexExternalValue::Adt(BexExternalAdt::Media(std::sync::Arc::new(media)));
        let out = external_to_baml_value(&value, &options).unwrap();

        assert_eq!(out.handles_created.len(), 1);
        assert!(matches!(out.value, Some(BamlValueVariant::HandleValue(_))));
    }
}
