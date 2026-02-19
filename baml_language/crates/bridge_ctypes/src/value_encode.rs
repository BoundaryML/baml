//! `BexExternalValue` -> `CffiValueHolder` conversion.

use bex_factory::{BexExternalAdt, BexExternalValue, MediaKind, Ty};

use crate::{
    baml::cffi::{
        CffiFieldTypeAny, CffiFieldTypeBool, CffiFieldTypeFloat, CffiFieldTypeHolder,
        CffiFieldTypeInt, CffiFieldTypeList, CffiFieldTypeMap, CffiFieldTypeNull,
        CffiFieldTypeOptional, CffiFieldTypeString, CffiFieldTypeUnionVariant, CffiMapEntry,
        CffiTypeName, CffiTypeNamespace, CffiValueClass, CffiValueEnum, CffiValueHolder,
        CffiValueList, CffiValueMap, CffiValueUnionVariant,
        cffi_field_type_holder::Type as FieldType, cffi_value_holder::Value as CffiValueVariant,
    },
    error::CtypesError,
};

/// Convert `BexExternalValue` to `CffiValueHolder` for FFI return.
pub fn external_to_cffi_value(value: &BexExternalValue) -> Result<CffiValueHolder, CtypesError> {
    let variant = match value {
        &BexExternalValue::Handle(_) => return Err(CtypesError::HandleNotSupported),
        BexExternalValue::Null => None,
        BexExternalValue::Int(i) => Some(CffiValueVariant::IntValue(*i)),
        BexExternalValue::Float(f) => Some(CffiValueVariant::FloatValue(*f)),
        BexExternalValue::Bool(b) => Some(CffiValueVariant::BoolValue(*b)),
        BexExternalValue::String(s) => Some(CffiValueVariant::StringValue(s.clone())),
        BexExternalValue::Array {
            items,
            element_type,
        } => {
            let values: Result<Vec<CffiValueHolder>, CtypesError> =
                items.iter().map(external_to_cffi_value).collect();
            Some(CffiValueVariant::ListValue(CffiValueList {
                item_type: Some(ty_to_field_type(element_type)),
                items: values?,
            }))
        }
        BexExternalValue::Map {
            entries,
            key_type,
            value_type,
        } => {
            let mut cffi_entries = Vec::new();
            for (key, val) in entries {
                cffi_entries.push(CffiMapEntry {
                    key: key.clone(),
                    value: Some(external_to_cffi_value(val)?),
                });
            }
            Some(CffiValueVariant::MapValue(CffiValueMap {
                key_type: Some(ty_to_field_type(key_type)),
                value_type: Some(ty_to_field_type(value_type)),
                entries: cffi_entries,
            }))
        }
        BexExternalValue::Instance { class_name, fields } => {
            let mut cffi_fields = Vec::new();
            for (key, val) in fields {
                cffi_fields.push(CffiMapEntry {
                    key: key.clone(),
                    value: Some(external_to_cffi_value(val)?),
                });
            }
            Some(CffiValueVariant::ClassValue(CffiValueClass {
                name: Some(CffiTypeName {
                    namespace: CffiTypeNamespace::Types as i32,
                    name: class_name.clone(),
                }),
                fields: cffi_fields,
            }))
        }
        BexExternalValue::Variant {
            enum_name,
            variant_name,
        } => Some(CffiValueVariant::EnumValue(CffiValueEnum {
            name: Some(CffiTypeName {
                namespace: CffiTypeNamespace::Types as i32,
                name: enum_name.clone(),
            }),
            value: variant_name.clone(),
            is_dynamic: false,
        })),
        BexExternalValue::Union { value, metadata } => {
            let inner = external_to_cffi_value(value)?;
            Some(CffiValueVariant::UnionVariantValue(Box::new(
                CffiValueUnionVariant {
                    name: metadata.name.as_ref().map(|n| CffiTypeName {
                        namespace: CffiTypeNamespace::Types as i32,
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
        BexExternalValue::Adt(BexExternalAdt::Media(media)) => {
            let kind_str = match media.kind {
                MediaKind::Image => "image",
                MediaKind::Audio => "audio",
                MediaKind::Video => "video",
                MediaKind::Pdf => "pdf",
                MediaKind::Generic => "media",
            };
            Some(CffiValueVariant::StringValue(format!(
                "[{kind_str}:handle]"
            )))
        }
        // Runtime-only types not representable in CFFI; map to null (caller receives null value).
        BexExternalValue::Resource(_handle) => None,
        BexExternalValue::Adt(
            BexExternalAdt::PromptAst(_) | BexExternalAdt::Collector(_) | BexExternalAdt::Type(_),
        )
        | BexExternalValue::FunctionRef { .. } => None,
    };

    Ok(CffiValueHolder { value: variant })
}

fn ty_to_field_type(ty: &Ty) -> CffiFieldTypeHolder {
    let field_type = match ty {
        Ty::Null => Some(FieldType::NullType(CffiFieldTypeNull {})),
        Ty::Int => Some(FieldType::IntType(CffiFieldTypeInt {})),
        Ty::Float => Some(FieldType::FloatType(CffiFieldTypeFloat {})),
        Ty::Bool => Some(FieldType::BoolType(CffiFieldTypeBool {})),
        Ty::String => Some(FieldType::StringType(CffiFieldTypeString {})),
        Ty::List(inner) => Some(FieldType::ListType(Box::new(CffiFieldTypeList {
            item_type: Some(Box::new(ty_to_field_type(inner))),
        }))),
        Ty::Map { key, value } => Some(FieldType::MapType(Box::new(CffiFieldTypeMap {
            key_type: Some(Box::new(ty_to_field_type(key))),
            value_type: Some(Box::new(ty_to_field_type(value))),
        }))),
        Ty::Class(tn) => Some(FieldType::ClassType(
            crate::baml::cffi::CffiFieldTypeClass {
                name: Some(CffiTypeName {
                    namespace: CffiTypeNamespace::Types as i32,
                    name: tn.display_name.to_string(),
                }),
            },
        )),
        Ty::Enum(tn) => Some(FieldType::EnumType(crate::baml::cffi::CffiFieldTypeEnum {
            name: tn.display_name.to_string(),
        })),
        Ty::Union(_) => Some(FieldType::UnionVariantType(CffiFieldTypeUnionVariant {
            name: None,
        })),
        Ty::Optional(inner) => Some(FieldType::OptionalType(Box::new(CffiFieldTypeOptional {
            value: Some(Box::new(ty_to_field_type(inner))),
        }))),
        Ty::Media(_) | Ty::Literal(_) => Some(FieldType::AnyType(CffiFieldTypeAny {})),
        Ty::Opaque(tn) => {
            unreachable!("runtime-only {tn} should not reach FFI type encoding")
        }
        Ty::TypeAlias(_)
        | Ty::Function { .. }
        | Ty::Void
        | Ty::WatchAccessor(_)
        | Ty::BuiltinUnknown => unreachable!("compiler-only variant should not reach FFI"),
    };

    CffiFieldTypeHolder { r#type: field_type }
}
