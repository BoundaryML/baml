use baml_runtime::TypeIR;
use baml_types::{ir_type::UnionConstructor, type_meta, BamlMediaType, BamlValue};

use crate::{
    baml::cffi::{
        cffi_field_type_holder, cffi_value_holder, CffiFieldTypeHolder, CffiFieldTypeList,
        CffiFieldTypeMap, CffiMapEntry, CffiTypeName, CffiTypeNamespace, CffiValueClass,
        CffiValueEnum, CffiValueHolder, CffiValueList, CffiValueMap, CffiValueNull,
    },
    ctypes::utils::{Encode, UnionAllowance, WithIr},
    raw_ptr_wrapper::RawPtrWrapper,
};

impl<'a, TypeLookups> Encode<CffiValueHolder> for WithIr<'a, BamlValue, TypeLookups, type_meta::IR>
where
    TypeLookups: baml_types::baml_value::TypeLookups + 'a,
{
    fn encode(self) -> CffiValueHolder {
        use cffi_value_holder::Value as cValue;

        let value = match self.value {
            BamlValue::Null => cValue::NullValue(CffiValueNull::default()),
            BamlValue::Bool(b) => cValue::BoolValue(*b),
            BamlValue::Int(i) => cValue::IntValue(*i),
            BamlValue::Float(f) => cValue::FloatValue(*f),
            BamlValue::String(s) => cValue::StringValue(s.clone()),
            BamlValue::Map(map) => {
                let TypeIR::Map(key_type, value_type, _) = self.value.to_type_ir() else {
                    panic!("Expected map type ir");
                };
                let entries = map
                    .iter()
                    .map(|(key, value)| CffiMapEntry {
                        key: key.clone(),
                        value: Some(
                            WithIr {
                                value,
                                lookup: self.lookup,
                                mode: self.mode,
                                curr_type: value_type.as_ref().clone(),
                            }
                            .encode(),
                        ),
                    })
                    .collect();
                let key_type = WithIr {
                    value: &(key_type.as_ref(), UnionAllowance::Disallow),
                    lookup: self.lookup,
                    mode: self.mode,
                    curr_type: *key_type.clone(),
                }
                .encode();
                let value_type = WithIr {
                    value: &(value_type.as_ref(), UnionAllowance::Disallow),
                    lookup: self.lookup,
                    mode: self.mode,
                    curr_type: *value_type.clone(),
                }
                .encode();
                cValue::MapValue(CffiValueMap {
                    key_type: Some(key_type),
                    value_type: Some(value_type),
                    entries,
                })
            }
            BamlValue::List(list) => {
                let TypeIR::List(value_type, _) = self.value.to_type_ir() else {
                    panic!("Expected list type ir");
                };
                let mut values = Vec::new();
                for value in list {
                    values.push(
                        WithIr {
                            value,
                            lookup: self.lookup,
                            mode: self.mode,
                            curr_type: value_type.as_ref().clone(),
                        }
                        .encode(),
                    );
                }
                let value_type = WithIr {
                    value: &(value_type.as_ref(), UnionAllowance::Disallow),
                    lookup: self.lookup,
                    mode: self.mode,
                    curr_type: *value_type.clone(),
                }
                .encode();
                cValue::ListValue(CffiValueList {
                    item_type: Some(value_type),
                    items: values,
                })
            }
            BamlValue::Media(media) => {
                let media_object = crate::raw_ptr_wrapper::RawPtrType::Media(
                    RawPtrWrapper::from_object(media.clone()),
                );
                let media_object = crate::raw_ptr_wrapper::RawPtrType::encode(media_object);
                cValue::ObjectValue(crate::baml::cffi::CffiValueRawObject {
                    object: Some(crate::baml::cffi::cffi_value_raw_object::Object::Media(
                        media_object,
                    )),
                })
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
                // dynamic_fields: vec![],
                fields: fields
                    .iter()
                    .map(|(name, value)| CffiMapEntry {
                        key: name.clone(),
                        value: Some(
                            WithIr {
                                value,
                                lookup: self.lookup,
                                mode: self.mode,
                                curr_type: value.to_type_ir(),
                            }
                            .encode(),
                        ),
                    })
                    .collect(),
            }),
        };

        let value = CffiValueHolder { value: Some(value) };

        // if type is optional, wrap in optional
        if self.curr_type.is_optional() && !self.curr_type.is_null() {
            CffiValueHolder {
                value: Some(cValue::UnionVariantValue(Box::new(
                    crate::baml::cffi::CffiValueUnionVariant {
                        name: None,
                        is_optional: true,
                        is_single_pattern: true,
                        self_type: Some(
                            WithIr {
                                value: &(&self.curr_type, UnionAllowance::Allow),
                                lookup: self.lookup,
                                mode: self.mode,
                                curr_type: self.curr_type.clone(),
                            }
                            .encode(),
                        ),
                        value_option_name: "null".to_string(),
                        value: Some(Box::new(value)),
                    },
                ))),
            }
        } else {
            value
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
                BamlMediaType::Pdf => TypeIR::pdf(),
                BamlMediaType::Video => TypeIR::video(),
            },
            BamlValue::Enum(name, _) => TypeIR::r#enum(name),
            BamlValue::Class(name, _) => TypeIR::class(name),
        }
    }
}

impl<'a, TypeLookups> Encode<CffiFieldTypeHolder>
    for WithIr<'a, BamlValue, TypeLookups, type_meta::IR>
where
    TypeLookups: baml_types::baml_value::TypeLookups + 'a,
{
    fn encode(self) -> CffiFieldTypeHolder {
        match self.value {
            BamlValue::Map(_) => {
                let TypeIR::Map(key_type, value_type, _) = self.value.to_type_ir() else {
                    panic!("Expected map type ir");
                };
                let key_type = WithIr {
                    value: &(key_type.as_ref(), UnionAllowance::Disallow),
                    lookup: self.lookup,
                    mode: self.mode,
                    curr_type: *key_type.clone(),
                }
                .encode();
                let value_type = WithIr {
                    value: &(value_type.as_ref(), UnionAllowance::Disallow),
                    lookup: self.lookup,
                    mode: self.mode,
                    curr_type: *value_type.clone(),
                }
                .encode();
                CffiFieldTypeHolder {
                    r#type: Some(cffi_field_type_holder::Type::MapType(Box::new(
                        CffiFieldTypeMap {
                            key_type: Some(Box::new(key_type)),
                            value_type: Some(Box::new(value_type)),
                        },
                    ))),
                }
            }
            BamlValue::List(_) => {
                let TypeIR::List(item_type, _) = self.value.to_type_ir() else {
                    panic!("Expected list type ir");
                };
                let item_type = WithIr {
                    value: &(item_type.as_ref(), UnionAllowance::Disallow),
                    lookup: self.lookup,
                    mode: self.mode,
                    curr_type: *item_type.clone(),
                }
                .encode();
                CffiFieldTypeHolder {
                    r#type: Some(cffi_field_type_holder::Type::ListType(Box::new(
                        CffiFieldTypeList {
                            item_type: Some(Box::new(item_type)),
                        },
                    ))),
                }
            }
            other => WithIr {
                value: &(&other.to_type_ir(), UnionAllowance::Disallow),
                lookup: self.lookup,
                mode: self.mode,
                curr_type: other.to_type_ir(),
            }
            .encode(),
        }
    }
}
