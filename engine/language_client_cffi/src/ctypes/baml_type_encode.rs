use baml_types::{ir_type::TypeGeneric, type_meta, ToUnionName};

use crate::{
    baml::cffi::{
        cffi_field_type_holder, cffi_field_type_literal, CffiCheckType, CffiFieldTypeChecked,
        CffiFieldTypeClass, CffiFieldTypeEnum, CffiFieldTypeHolder, CffiFieldTypeList,
        CffiFieldTypeLiteral, CffiFieldTypeMap, CffiFieldTypeMedia, CffiFieldTypeOptional,
        CffiFieldTypeStreamState, CffiFieldTypeTypeAlias, CffiFieldTypeUnionVariant,
        CffiLiteralBool, CffiLiteralInt, CffiLiteralString, CffiTypeName, CffiTypeNamespace,
        MediaTypeEnum,
    },
    ctypes::utils::{IsChecked, UnionAllowance, WithIr},
    ffi::Encode,
};

pub(crate) fn create_cffi_type_name(
    name: impl ToString,
    namespace: CffiTypeNamespace,
) -> CffiTypeName {
    CffiTypeName {
        name: name.to_string(),
        namespace: namespace.into(),
    }
}

// Encode for Types (moved from baml_type_encode.rs)
impl<'a, TypeLookups, T: IsChecked + type_meta::MayHaveMeta> Encode<CffiFieldTypeHolder>
    for WithIr<'a, (&'a TypeGeneric<T>, UnionAllowance), TypeLookups, T>
where
    TypeLookups: baml_types::baml_value::TypeLookups + 'a,
    T: std::hash::Hash + std::cmp::Eq + Clone,
{
    fn encode(self) -> CffiFieldTypeHolder {
        let WithIr {
            value,
            lookup,
            mode,
            mut curr_type,
        } = self;

        use cffi_field_type_holder::Type as cType;

        let c_type = if curr_type.meta().stream_with_state() {
            curr_type.meta_mut().pop_stream_state();
            cType::StreamStateType(Box::new(CffiFieldTypeStreamState {
                value: Some(Box::new(
                    WithIr {
                        value,
                        lookup,
                        mode,
                        curr_type,
                    }
                    .encode(),
                )),
            }))
        } else if let Some(checks) = curr_type.meta().checks() {
            let checks = checks
                .iter()
                .map(|c| CffiCheckType {
                    name: c.to_string(),
                })
                .collect();
            curr_type.meta_mut().pop_checks();
            cType::CheckedType(Box::new(CffiFieldTypeChecked {
                value: Some(Box::new(
                    WithIr {
                        value,
                        lookup,
                        mode,
                        curr_type,
                    }
                    .encode(),
                )),
                checks,
            }))
        } else {
            match curr_type {
                TypeGeneric::Top(_) => panic!(
                    "TypeGeneric::Top should have been resolved by the compiler before code generation. \
                    This indicates a bug in the type resolution phase."
                ),
                TypeGeneric::Tuple(_, _) => panic!("Tuple types are not supported in CFFI"),
                TypeGeneric::Arrow(_, _) => panic!("Arrow types are not supported in CFFI"),
                TypeGeneric::Primitive(type_value, _) => type_value.encode(),
                TypeGeneric::Literal(literal_value, _) => cType::LiteralType(literal_value.encode()),
                TypeGeneric::Enum {
                    name,
                    dynamic: _,
                    meta: _,
                } => cType::EnumType(CffiFieldTypeEnum { name }),
                TypeGeneric::Class {
                    name,
                    mode,
                    dynamic: _,
                    meta: _,
                } => {
                    cType::ClassType(CffiFieldTypeClass {
                        name: Some(create_cffi_type_name(name, match mode {
                            baml_types::StreamingMode::NonStreaming => CffiTypeNamespace::Types,
                            baml_types::StreamingMode::Streaming => CffiTypeNamespace::StreamTypes,
                        })),
                    })
                }
                TypeGeneric::RecursiveTypeAlias { name, mode, meta: _ } => cType::TypeAliasType(CffiFieldTypeTypeAlias {
                    name: Some(create_cffi_type_name(
                            name,
                        match mode {
                            baml_types::StreamingMode::NonStreaming => CffiTypeNamespace::Types,
                            baml_types::StreamingMode::Streaming => CffiTypeNamespace::StreamTypes,
                        }))
                }),
                // Container Types
                TypeGeneric::List(type_generic, _) => cType::ListType(Box::new(CffiFieldTypeList {
                    item_type: Some(Box::new(WithIr {
                        value,
                        lookup,
                        mode,
                        curr_type: *type_generic,
                    }.encode())),
                })),
                TypeGeneric::Map(key_type, value_type, _) => cType::MapType(Box::new(CffiFieldTypeMap {
                    key_type: Some(Box::new(WithIr {
                        value,
                        lookup,
                        mode,
                        curr_type: *key_type,
                    }.encode())),
                    value_type: Some(Box::new(WithIr {
                        value,
                        lookup,
                        mode,
                        curr_type: *value_type,
                    }.encode())),
                })),
                TypeGeneric::Union(union_type, meta) => {
                    fn wrap_in_optional(encoded_type: CffiFieldTypeHolder) -> cType {
                        cType::OptionalType(Box::new(CffiFieldTypeOptional {
                            value: Some(Box::new(encoded_type)),
                        }))
                    }

                    let make_union_variant = |union_type: baml_types::ir_type::UnionTypeGeneric<T>, meta: T| -> cType {
                        let curr_type = TypeGeneric::Union(union_type, meta);
                        cType::UnionVariantType(CffiFieldTypeUnionVariant {
                            name: Some(create_cffi_type_name(
                                curr_type.to_union_name(false).as_str(),
                                match curr_type
                                    .mode(&mode, lookup, 1)
                                    .expect("Failed to get mode for field type")
                                {
                                    baml_types::StreamingMode::NonStreaming => CffiTypeNamespace::Types,
                                    baml_types::StreamingMode::Streaming => CffiTypeNamespace::StreamTypes,
                                },
                            )),
                        })
                    };

                    match union_type.view() {
                        baml_types::ir_type::UnionTypeViewGeneric::Null => cType::NullType(Default::default()),
                        baml_types::ir_type::UnionTypeViewGeneric::Optional(type_generic) => wrap_in_optional(WithIr {
                                                value,
                                                lookup,
                                                mode,
                                                curr_type: type_generic.clone(),
                                            }.encode()),
baml_types::ir_type::UnionTypeViewGeneric::OneOf(_) => {
                    make_union_variant(union_type, meta)
                }
baml_types::ir_type::UnionTypeViewGeneric::OneOfOptional(
_
) => wrap_in_optional(CffiFieldTypeHolder {
    r#type: Some(make_union_variant(union_type, meta)),
}),
                    }
                }
            }
        };

        CffiFieldTypeHolder {
            r#type: Some(c_type),
        }
    }
}

impl Encode<cffi_field_type_holder::Type> for &baml_types::TypeValue {
    fn encode(self) -> cffi_field_type_holder::Type {
        use cffi_field_type_holder::Type as cType;
        match self {
            baml_types::TypeValue::String => cType::StringType(Default::default()),
            baml_types::TypeValue::Int => cType::IntType(Default::default()),
            baml_types::TypeValue::Float => cType::FloatType(Default::default()),
            baml_types::TypeValue::Bool => cType::BoolType(Default::default()),
            baml_types::TypeValue::Null => cType::NullType(Default::default()),
            baml_types::TypeValue::Media(baml_media_type) => {
                cType::MediaType(baml_media_type.encode())
            }
        }
    }
}

impl Encode<CffiFieldTypeMedia> for &baml_types::BamlMediaType {
    fn encode(self) -> CffiFieldTypeMedia {
        CffiFieldTypeMedia {
            media: match self {
                baml_types::BamlMediaType::Image => MediaTypeEnum::Image,
                baml_types::BamlMediaType::Audio => MediaTypeEnum::Audio,
                baml_types::BamlMediaType::Pdf => MediaTypeEnum::Pdf,
                baml_types::BamlMediaType::Video => MediaTypeEnum::Video,
            }
            .into(),
        }
    }
}

impl Encode<CffiFieldTypeLiteral> for &baml_types::LiteralValue {
    fn encode(self) -> CffiFieldTypeLiteral {
        use cffi_field_type_literal::Literal;
        let literal = match self {
            baml_types::LiteralValue::String(val) => {
                Literal::StringLiteral(CffiLiteralString { value: val.clone() })
            }
            baml_types::LiteralValue::Int(val) => {
                Literal::IntLiteral(CffiLiteralInt { value: *val })
            }
            baml_types::LiteralValue::Bool(val) => {
                Literal::BoolLiteral(CffiLiteralBool { value: *val })
            }
        };

        CffiFieldTypeLiteral {
            literal: Some(literal),
        }
    }
}
