use baml_runtime::TypeIR;
use baml_types::{ir_type::UnionConstructor, BamlMediaType, BamlValue};

use crate::{
    baml::cffi::{
        cffi_field_type_holder, cffi_value_holder, CffiFieldTypeAny, CffiFieldTypeHolder,
        CffiFieldTypeList, CffiFieldTypeMap, CffiMapEntry, CffiTypeName, CffiTypeNamespace,
        CffiValueClass, CffiValueEnum, CffiValueHolder, CffiValueList, CffiValueMap, CffiValueNull,
    },
    ctypes::utils::{Encode, WithIr},
};

impl<'a, TypeLookups> Encode<CffiValueHolder> for WithIr<'a, BamlValue, TypeLookups>
where
    TypeLookups: baml_types::baml_value::TypeLookups + 'a,
{
    fn encode(self) -> CffiValueHolder {
        use cffi_value_holder::Value as cValue;

        let type_ir = self.value.to_type_ir();
        let value = match self.value {
            BamlValue::Null => cValue::NullValue(CffiValueNull::default()),
            BamlValue::Bool(b) => cValue::BoolValue(*b),
            BamlValue::Int(i) => cValue::IntValue(*i),
            BamlValue::Float(f) => cValue::FloatValue(*f),
            BamlValue::String(s) => cValue::StringValue(s.clone()),
            BamlValue::Map(map) => {
                let entries = map
                    .iter()
                    .map(|(key, value)| CffiMapEntry {
                        key: key.clone(),
                        value: Some(
                            WithIr {
                                value,
                                lookup: self.lookup,
                            }
                            .encode(),
                        ),
                    })
                    .collect();
                cValue::MapValue(CffiValueMap {
                    key_type: None,
                    value_type: None,
                    entries,
                })
            }
            BamlValue::List(list) => {
                let mut cffi_list = CffiValueList::default();
                for value in list {
                    cffi_list.values.push(
                        WithIr {
                            value,
                            lookup: self.lookup,
                        }
                        .encode(),
                    );
                }
                cValue::ListValue(cffi_list)
            }
            BamlValue::Media(_) => {
                panic!("Unsupported BamlValue::Media is not supported")
            }
            BamlValue::Enum(name, value) => cValue::EnumValue(CffiValueEnum {
                name: Some(CffiTypeName {
                    namespace: CffiTypeNamespace::Internal.into(),
                    name: name.clone(),
                }),
                value: value.clone(),
                is_dynamic: false,
            }),
            BamlValue::Class(name, fields) => cValue::ClassValue(CffiValueClass {
                name: Some(CffiTypeName {
                    namespace: CffiTypeNamespace::Internal.into(),
                    name: name.clone(),
                }),
                dynamic_fields: vec![],
                fields: fields
                    .iter()
                    .map(|(name, value)| CffiMapEntry {
                        key: name.clone(),
                        value: Some(
                            WithIr {
                                value,
                                lookup: self.lookup,
                            }
                            .encode(),
                        ),
                    })
                    .collect(),
            }),
        };

        CffiValueHolder {
            r#type: Some(
                WithIr {
                    value: &type_ir,
                    lookup: self.lookup,
                }
                .encode(),
            ),
            value: Some(value),
        }
    }
}

trait ToTypeIR {
    fn to_type_ir(&self) -> TypeIR;
}

impl ToTypeIR for BamlValue {
    fn to_type_ir(&self) -> TypeIR {
        match self {
            BamlValue::Null => TypeIR::null(),
            BamlValue::Bool(_) => TypeIR::bool(),
            BamlValue::Int(_) => TypeIR::int(),
            BamlValue::Float(_) => TypeIR::float(),
            BamlValue::String(_) => TypeIR::string(),
            BamlValue::Map(index_map) => TypeIR::map(
                TypeIR::string(),
                TypeIR::union(index_map.values().map(|v| v.to_type_ir()).collect()),
            ),
            BamlValue::List(baml_values) => TypeIR::list(TypeIR::union(
                baml_values.iter().map(|v| v.to_type_ir()).collect(),
            )),
            BamlValue::Media(baml_media) => match baml_media.media_type {
                BamlMediaType::Image => TypeIR::image(),
                BamlMediaType::Audio => TypeIR::audio(),
            },
            BamlValue::Enum(name, _) => TypeIR::r#enum(name),
            BamlValue::Class(name, _) => TypeIR::class(name),
        }
    }
}

fn encode_type_ir_no_unions<'a, TypeLookups>(
    type_ir: &TypeIR,
    lookup: &'a TypeLookups,
) -> CffiFieldTypeHolder
where
    TypeLookups: baml_types::baml_value::TypeLookups + 'a,
{
    match type_ir {
        TypeIR::Union(union, _) => match union.view() {
            baml_types::ir_type::UnionTypeViewGeneric::Null
            | baml_types::ir_type::UnionTypeViewGeneric::Optional(_) => WithIr {
                value: type_ir,
                lookup,
            }
            .encode(),
            baml_types::ir_type::UnionTypeViewGeneric::OneOf(_)
            | baml_types::ir_type::UnionTypeViewGeneric::OneOfOptional(_) => CffiFieldTypeHolder {
                r#type: Some(cffi_field_type_holder::Type::AnyType(
                    CffiFieldTypeAny::default(),
                )),
            },
        },
        other => WithIr {
            value: other,
            lookup,
        }
        .encode(),
    }
}

impl<'a, TypeLookups> Encode<CffiFieldTypeHolder> for WithIr<'a, BamlValue, TypeLookups>
where
    TypeLookups: baml_types::baml_value::TypeLookups + 'a,
{
    fn encode(self) -> CffiFieldTypeHolder {
        match self.value {
            BamlValue::Map(_) => {
                let TypeIR::Map(key_type, value_type, _) = self.value.to_type_ir() else {
                    panic!("Expected map type ir");
                };
                let value_type = encode_type_ir_no_unions(value_type.as_ref(), self.lookup);
                let key_type = WithIr {
                    value: key_type.as_ref(),
                    lookup: self.lookup,
                }
                .encode();
                CffiFieldTypeHolder {
                    r#type: Some(cffi_field_type_holder::Type::MapType(Box::new(
                        CffiFieldTypeMap {
                            key: Some(Box::new(key_type)),
                            value: Some(Box::new(value_type)),
                        },
                    ))),
                }
            }
            BamlValue::List(items) => {
                let TypeIR::List(item_type, _) = self.value.to_type_ir() else {
                    panic!("Expected list type ir");
                };
                let item_type = encode_type_ir_no_unions(item_type.as_ref(), self.lookup);
                CffiFieldTypeHolder {
                    r#type: Some(cffi_field_type_holder::Type::ListType(Box::new(
                        CffiFieldTypeList {
                            element: Some(Box::new(item_type)),
                        },
                    ))),
                }
            }
            other => WithIr {
                value: &other.to_type_ir(),
                lookup: self.lookup,
            }
            .encode(),
        }
    }
}
