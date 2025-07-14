use baml_types::{ir_type::TypeGeneric, ToUnionName};

use crate::{
    baml::cffi::*,
    ctypes::{
        self,
        utils::{Encode, WithIr},
    },
};

impl<'a, TypeLookups, T> Encode<CffiFieldTypeHolder> for WithIr<'a, TypeGeneric<T>, TypeLookups>
where
    TypeLookups: baml_types::baml_value::TypeLookups + 'a,
    T: std::hash::Hash + std::cmp::Eq,
{
    fn encode(self) -> CffiFieldTypeHolder {
        let WithIr { value, lookup } = self;

        use cffi_field_type_holder::Type as cType;

        let type_value = match value {
            TypeGeneric::Primitive(type_value, _) => type_value.encode(),
            TypeGeneric::Enum { name, .. } => {
                cType::EnumType(CffiFieldTypeEnum { name: name.clone() })
            }
            TypeGeneric::Literal(literal_value, _) => cType::LiteralType(literal_value.encode()),
            TypeGeneric::Class { name, mode, .. } => cType::ClassType(CffiFieldTypeClass {
                name: Some(CffiTypeName {
                    namespace: match mode {
                        baml_types::StreamingMode::NonStreaming => CffiTypeNamespace::Types.into(),
                        baml_types::StreamingMode::Streaming => {
                            CffiTypeNamespace::StreamTypes.into()
                        }
                    },
                    name: name.clone(),
                }),
            }),
            TypeGeneric::List(type_generic, _) => {
                let element = WithIr {
                    value: type_generic.as_ref(),
                    lookup,
                }
                .encode();
                cType::ListType(Box::new(CffiFieldTypeList {
                    element: Some(Box::new(element)),
                }))
            }
            TypeGeneric::Map(type_generic, type_generic1, _) => {
                let key = WithIr {
                    value: type_generic.as_ref(),
                    lookup,
                }
                .encode();
                let value = WithIr {
                    value: type_generic1.as_ref(),
                    lookup,
                }
                .encode();
                cType::MapType(Box::new(CffiFieldTypeMap {
                    key: Some(Box::new(key)),
                    value: Some(Box::new(value)),
                }))
            }
            TypeGeneric::RecursiveTypeAlias { name, mode, .. } => {
                cType::TypeAliasType(CffiFieldTypeTypeAlias {
                    name: Some(CffiTypeName {
                        namespace: match mode {
                            baml_types::StreamingMode::NonStreaming => {
                                CffiTypeNamespace::Types.into()
                            }
                            baml_types::StreamingMode::Streaming => {
                                CffiTypeNamespace::StreamTypes.into()
                            }
                        },
                        name: name.clone(),
                    }),
                })
            }
            TypeGeneric::Tuple(type_generics, _) => {
                let elements = type_generics
                    .iter()
                    .map(|t| WithIr { value: t, lookup }.encode())
                    .collect();
                cType::TupleType(CffiFieldTypeTuple { elements })
            }
            TypeGeneric::Arrow(_arrow_generic, _) => {
                unimplemented!("Arrow types are not supported in CFFI");
            }
            TypeGeneric::Union(union_type_generic, _) => {
                let view = union_type_generic.view();
                match view {
                    baml_types::ir_type::UnionTypeViewGeneric::Null => {
                        cType::NullType(CffiFieldTypeNull {})
                    }
                    baml_types::ir_type::UnionTypeViewGeneric::Optional(type_generic) => {
                        let inner = WithIr {
                            value: type_generic,
                            lookup,
                        }
                        .encode();
                        cType::OptionalType(Box::new(CffiFieldTypeOptional {
                            value: Some(Box::new(inner)),
                        }))
                    }
                    baml_types::ir_type::UnionTypeViewGeneric::OneOf(type_generics) => {
                        let elements = type_generics
                            .into_iter()
                            .map(|t| WithIr { value: t, lookup }.encode())
                            .collect();
                        cType::UnionVariantType(CffiFieldTypeUnionVariant {
                            name: Some(CffiTypeName {
                                namespace: CffiTypeNamespace::Types.into(),
                                name: value.to_union_name().to_string(),
                            }),
                            options: elements,
                        })
                    }
                    baml_types::ir_type::UnionTypeViewGeneric::OneOfOptional(type_generics) => {
                        let elements = type_generics
                            .into_iter()
                            .map(|t| WithIr { value: t, lookup }.encode())
                            .collect();
                        let inner = cType::UnionVariantType(CffiFieldTypeUnionVariant {
                            name: Some(CffiTypeName {
                                namespace: CffiTypeNamespace::Types.into(),
                                name: value.to_union_name().to_string(),
                            }),
                            options: elements,
                        });
                        let inner = CffiFieldTypeHolder {
                            r#type: Some(inner),
                        };
                        cType::OptionalType(Box::new(CffiFieldTypeOptional {
                            value: Some(Box::new(inner)),
                        }))
                    }
                }
            }
        };

        CffiFieldTypeHolder {
            r#type: Some(type_value),
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
