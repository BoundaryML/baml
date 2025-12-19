use baml_types::{
    baml_value::TypeQuery, ir_type::TypeGeneric, type_meta, BamlValueWithMeta, HasType, ToUnionName,
};

use crate::{
    baml::cffi::*,
    ctypes::{
        baml_type_encode::create_cffi_type_name,
        utils::{Encode, IsChecked, UnionAllowance, WithIr},
    },
};

pub struct Meta<'a, T> {
    pub field_type: TypeGeneric<T>,
    pub checks: &'a Vec<baml_types::ResponseCheck>,
}

impl<T> HasType<T> for Meta<'_, T> {
    fn field_type(&self) -> &TypeGeneric<T> {
        &self.field_type
    }
}

impl<'a, TypeLookups, T: IsChecked + type_meta::MayHaveMeta + baml_types::ir_type::MetaSuffix>
    Encode<CffiValueHolder> for WithIr<'a, BamlValueWithMeta<Meta<'_, T>>, TypeLookups, T>
where
    TypeLookups: baml_types::baml_value::TypeLookupsMeta<T> + 'a,
    for<'b> BamlValueWithMeta<Meta<'b, T>>: TypeQuery<T>,
    TypeGeneric<T>: std::fmt::Display,
    T: std::hash::Hash + std::cmp::Eq + Clone,
{
    fn encode(self) -> CffiValueHolder {
        use cffi_value_holder::Value;
        let WithIr {
            value,
            lookup,
            mode,
            curr_type,
        } = self;

        if curr_type.meta().stream_with_state() {
            let mut inner_type = curr_type.clone();
            inner_type.meta_mut().pop_stream_state();

            let inner_holder = WithIr {
                value,
                lookup,
                mode,
                curr_type: inner_type.clone(),
            }
            .encode();

            return CffiValueHolder {
                value: Some(cffi_value_holder::Value::StreamingStateValue(Box::new(
                    CffiValueStreamingState {
                        name: Some(create_cffi_type_name(
                            inner_type.to_union_name(true).as_str(),
                            CffiTypeNamespace::StreamStateTypes,
                        )),
                        value: Some(Box::new(inner_holder)),
                        // TODO: This should be the actual stream state as this is completely incorrect
                        // we don't currently plumb this through BamlValueWithMeta. To fix this, we need to
                        // add a new field to BamlValueWithMeta that stores the stream state.
                        state: CffiStreamState::Pending.into(),
                    },
                ))),
            };
        }

        if let Some(checks) = curr_type.meta().checks() {
            let mut inner_type = curr_type.clone();
            inner_type.meta_mut().pop_checks();

            let inner_holder = WithIr {
                value,
                lookup,
                mode,
                curr_type: inner_type.clone(),
            }
            .encode();

            let check_result = value.meta().checks.iter().filter_map(|c| {
                if checks.contains(&c.name.as_str()) {
                    Some(c.encode())
                } else {
                    None
                }
            });

            return CffiValueHolder {
                value: Some(cffi_value_holder::Value::CheckedValue(Box::new(
                    CffiValueChecked {
                        name: Some(create_cffi_type_name(
                            inner_type.to_union_name(true).as_str(),
                            CffiTypeNamespace::CheckedTypes,
                        )),
                        value: Some(Box::new(inner_holder)),
                        checks: check_result.collect(),
                    },
                ))),
            };
        }

        let curr_type = match curr_type {
            TypeGeneric::RecursiveTypeAlias { name, .. } => {
                let expanded_type =
                    baml_types::baml_value::TypeLookupsMeta::<T>::expand_recursive_type(
                        lookup, &name,
                    )
                    .unwrap_or_else(|_| panic!("Failed to expand recursive type alias {name}"));
                expanded_type
            }
            other => other,
        };

        if let TypeGeneric::Union(u, _) = &curr_type {
            let real_type = value.real_type(lookup);

            let inner_value = WithIr {
                value,
                lookup,
                mode,
                curr_type: real_type.clone(),
            }
            .encode();

            let baml_types::ir_type::SelectedTypeIndexResult {
                index: value_type_index,
                options,
            } = u
                .selected_type_index(&real_type, lookup)
                .unwrap_or_else(|_| {
                    panic!("Failed to find target_type in options: {real_type} -> {curr_type}")
                });

            if options.len() == 1 {
                if !real_type.is_null() {
                    panic!("Union has only one option and value is not null: {real_type} -> {curr_type}");
                }
                return inner_value;
            }

            let variant_name = options[value_type_index].to_union_name(false);
            let union_variant = CffiValueUnionVariant {
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
                is_optional: curr_type.is_optional(),
                is_single_pattern: matches!(
                    u.view(),
                    baml_types::ir_type::UnionTypeViewGeneric::Optional(_)
                ),
                self_type: Some(
                    WithIr {
                        value: &(&curr_type, UnionAllowance::Allow),
                        lookup,
                        mode,
                        curr_type: curr_type.clone(),
                    }
                    .encode(),
                ),
                value_option_name: variant_name,
                value: Some(Box::new(inner_value)),
            };
            return CffiValueHolder {
                value: Some(cffi_value_holder::Value::UnionVariantValue(Box::new(
                    union_variant,
                ))),
            };
        };

        let encoded_value = {
            match value {
                BamlValueWithMeta::String(val, _) => {
                    if curr_type.is_literal() {
                        Value::LiteralValue(CffiFieldTypeLiteral {
                            literal: Some(cffi_field_type_literal::Literal::StringLiteral(
                                CffiLiteralString { value: val.clone() },
                            )),
                        })
                    } else {
                        Value::StringValue(val.clone())
                    }
                }
                BamlValueWithMeta::Bool(val, _) => {
                    if curr_type.is_literal() {
                        Value::LiteralValue(CffiFieldTypeLiteral {
                            literal: Some(cffi_field_type_literal::Literal::BoolLiteral(
                                CffiLiteralBool { value: *val },
                            )),
                        })
                    } else {
                        Value::BoolValue(*val)
                    }
                }
                BamlValueWithMeta::Int(val, _) => {
                    if curr_type.is_literal() {
                        Value::LiteralValue(CffiFieldTypeLiteral {
                            literal: Some(cffi_field_type_literal::Literal::IntLiteral(
                                CffiLiteralInt { value: *val },
                            )),
                        })
                    } else {
                        Value::IntValue(*val)
                    }
                }
                BamlValueWithMeta::Float(val, _) => Value::FloatValue(*val),
                BamlValueWithMeta::Map(index_map, _) => {
                    let curr_type = curr_type.resolve_map(lookup).unwrap();
                    let TypeGeneric::Map(key_type, value_type, _) = &curr_type else {
                        panic!("resolve_map somehow returned a non-map type: {curr_type}");
                    };
                    let encoded_key_type = WithIr {
                        value: &(key_type.as_ref(), UnionAllowance::Allow),
                        lookup,
                        mode,
                        curr_type: key_type.as_ref().clone(),
                    }
                    .encode();
                    let encoded_value_type = WithIr {
                        value: &(value_type.as_ref(), UnionAllowance::Allow),
                        lookup,
                        mode,
                        curr_type: value_type.as_ref().clone(),
                    }
                    .encode();
                    let entries = index_map
                        .iter()
                        .map(|(key, value)| CffiMapEntry {
                            key: key.clone(),
                            value: Some(
                                WithIr {
                                    value,
                                    lookup,
                                    mode,
                                    curr_type: value_type.as_ref().clone(),
                                }
                                .encode(),
                            ),
                        })
                        .collect();
                    Value::MapValue(CffiValueMap {
                        key_type: Some(encoded_key_type),
                        value_type: Some(encoded_value_type),
                        entries,
                    })
                }
                BamlValueWithMeta::List(baml_value_with_metas, _) => {
                    let curr_type = curr_type.resolve_list(lookup).unwrap();
                    let TypeGeneric::List(item_type, _) = &curr_type else {
                        panic!("resolve_list somehow returned a non-list type: {curr_type}");
                    };
                    let encoded_item_type = WithIr {
                        value: &(item_type.as_ref(), UnionAllowance::Allow),
                        lookup,
                        mode,
                        curr_type: item_type.as_ref().clone(),
                    }
                    .encode();
                    let items = baml_value_with_metas
                        .iter()
                        .map(|bvm| {
                            WithIr {
                                value: bvm,
                                lookup,
                                mode,
                                curr_type: item_type.as_ref().clone(),
                            }
                            .encode()
                        })
                        .collect();
                    Value::ListValue(CffiValueList {
                        item_type: Some(encoded_item_type),
                        items,
                    })
                }
                BamlValueWithMeta::Media(media, _) => {
                    let media_object = crate::raw_ptr_wrapper::RawPtrType::Media(
                        crate::raw_ptr_wrapper::RawPtrWrapper::from_object(media.clone()),
                    );
                    let media_object = crate::raw_ptr_wrapper::RawPtrType::encode(media_object);
                    Value::ObjectValue(CffiValueRawObject {
                        object: Some(crate::baml::cffi::cffi_value_raw_object::Object::Media(
                            media_object,
                        )),
                    })
                }
                BamlValueWithMeta::Enum(_, value, _) => {
                    let curr_type = curr_type.resolve_enum(lookup).unwrap();
                    let TypeGeneric::Enum { name, dynamic, .. } = &curr_type else {
                        panic!("resolve_enum somehow returned a non-enum type: {curr_type}");
                    };
                    Value::EnumValue(CffiValueEnum {
                        name: Some(create_cffi_type_name(
                            name,
                            match curr_type
                                .mode(&mode, lookup, 0)
                                .expect("Failed to get mode for field type")
                            {
                                baml_types::StreamingMode::NonStreaming => CffiTypeNamespace::Types,
                                baml_types::StreamingMode::Streaming => {
                                    CffiTypeNamespace::StreamTypes
                                }
                            },
                        )),
                        value: value.clone(),
                        is_dynamic: *dynamic,
                    })
                }
                BamlValueWithMeta::Class(_, index_map, _) => {
                    let curr_type = curr_type.resolve_class(lookup).unwrap();
                    let TypeGeneric::Class { name, .. } = &curr_type else {
                        panic!("resolve_class somehow returned a non-class type: {curr_type}");
                    };
                    let fields = index_map
                        .iter()
                        .map(|(key, value)| CffiMapEntry {
                            key: key.clone(),
                            value: Some(
                                WithIr {
                                    value,
                                    lookup,
                                    mode,
                                    curr_type: value.field_type().clone(),
                                }
                                .encode(),
                            ),
                        })
                        .collect();
                    Value::ClassValue(CffiValueClass {
                        name: Some(create_cffi_type_name(
                            name,
                            match curr_type
                                .mode(&mode, lookup, 0)
                                .expect("Failed to get mode for field type")
                            {
                                baml_types::StreamingMode::NonStreaming => CffiTypeNamespace::Types,
                                baml_types::StreamingMode::Streaming => {
                                    CffiTypeNamespace::StreamTypes
                                }
                            },
                        )),
                        fields,
                    })
                }
                BamlValueWithMeta::Null(_) => Value::NullValue(CffiValueNull {}),
            }
        };

        CffiValueHolder {
            value: Some(encoded_value),
        }
    }
}

impl Encode<CffiCheckValue> for &baml_types::ResponseCheck {
    fn encode(self) -> CffiCheckValue {
        CffiCheckValue {
            name: self.name.clone(),
            expression: self.expression.clone(),
            status: self.status.clone(),
            value: None,
        }
    }
}
