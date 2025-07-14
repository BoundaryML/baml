use baml_types::{
    baml_value::TypeQuery, ir_type::TypeGeneric, type_meta, BamlValueWithMeta, HasType, ToUnionName,
};

use crate::{
    baml::cffi::*,
    ctypes::utils::{Encode, WithIr},
};

fn create_cffi_type_name(name: &str, namespace: CffiTypeNamespace) -> CffiTypeName {
    CffiTypeName {
        name: name.to_string(),
        namespace: namespace.into(),
    }
}

pub struct Meta<'a, T> {
    pub field_type: TypeGeneric<T>,
    pub checks: &'a Vec<baml_types::ResponseCheck>,
}

impl<T> HasType<T> for Meta<'_, T> {
    fn field_type(&self) -> &TypeGeneric<T> {
        &self.field_type
    }
}

trait MaybeWrapUnion<TypeLookups>
where
    TypeLookups: baml_types::baml_value::TypeLookups,
{
    fn maybe_wrap_union(&self, holder: CffiValueHolder, lookup: &TypeLookups) -> CffiValueHolder;
    fn maybe_wrap_stream_state(
        &self,
        holder: CffiValueHolder,
        lookup: &TypeLookups,
    ) -> CffiValueHolder;
}

fn maybe_wrap_union_impl<TypeLookups, T>(
    value: &BamlValueWithMeta<Meta<'_, T>>,
    holder: CffiValueHolder,
    lookup: &TypeLookups,
    get_target_type: impl Fn(&TypeGeneric<T>) -> TypeGeneric<T>,
    get_namespace: impl Fn(&TypeGeneric<T>) -> CffiTypeNamespace,
) -> CffiValueHolder
where
    TypeLookups: baml_types::baml_value::TypeLookups,
    for<'a> BamlValueWithMeta<Meta<'a, T>>: HasType<T> + TypeQuery<T>,
    T: std::hash::Hash + std::cmp::Eq,
{
    let target_type = &get_target_type(value.field_type());

    if let baml_types::ir_type::TypeGeneric::Union(union_type_generic, _) = target_type {
        let real_type = value.real_type(lookup);
        if real_type.is_null() {
            return holder;
        }

        let options = match union_type_generic.view() {
            baml_types::ir_type::UnionTypeViewGeneric::Null
            | baml_types::ir_type::UnionTypeViewGeneric::Optional(..) => return holder,
            baml_types::ir_type::UnionTypeViewGeneric::OneOf(type_generics) => type_generics,
            baml_types::ir_type::UnionTypeViewGeneric::OneOfOptional(type_generics) => {
                type_generics
            }
        };

        let value_type_index = options
            .iter()
            .position(|t| real_type == **t)
            .expect("Failed to find target_type in options");

        let variant_name = options[value_type_index].to_union_name();

        let union_variant = CffiValueUnionVariant {
            name: Some(create_cffi_type_name(
                target_type.to_union_name().as_str(),
                get_namespace(value.field_type()),
            )),
            variant_name,
            field_types: options
                .into_iter()
                .map(|t| WithIr { value: t, lookup }.encode())
                .collect(),
            value_type_index: value_type_index as i32,
            value: Some(Box::new(holder)),
        };

        CffiValueHolder {
            r#type: Some(
                WithIr {
                    value: target_type,
                    lookup,
                }
                .encode(),
            ),
            value: Some(cffi_value_holder::Value::UnionVariantValue(Box::new(
                union_variant,
            ))),
        }
    } else {
        holder
    }
}

impl<TypeLookups> MaybeWrapUnion<TypeLookups>
    for BamlValueWithMeta<Meta<'_, type_meta::NonStreaming>>
where
    TypeLookups: baml_types::baml_value::TypeLookups,
{
    fn maybe_wrap_union(&self, holder: CffiValueHolder, lookup: &TypeLookups) -> CffiValueHolder {
        maybe_wrap_union_impl(
            self,
            holder,
            lookup,
            |field_type| match field_type {
                baml_types::ir_type::TypeGeneric::RecursiveTypeAlias { name, .. } => lookup
                    .expand_recursive_type(name)
                    .unwrap_or_else(|_| panic!("Failed to expand recursive type alias: {name}"))
                    .to_non_streaming_type(lookup),
                other => other.clone(),
            },
            |_| CffiTypeNamespace::Types,
        )
    }

    fn maybe_wrap_stream_state(
        &self,
        holder: CffiValueHolder,
        _lookup: &TypeLookups,
    ) -> CffiValueHolder {
        holder
    }
}

impl<TypeLookups> MaybeWrapUnion<TypeLookups> for BamlValueWithMeta<Meta<'_, type_meta::Streaming>>
where
    TypeLookups: baml_types::baml_value::TypeLookups,
{
    fn maybe_wrap_union(&self, holder: CffiValueHolder, lookup: &TypeLookups) -> CffiValueHolder {
        maybe_wrap_union_impl(
            self,
            holder,
            lookup,
            |field_type| match field_type {
                baml_types::ir_type::TypeGeneric::RecursiveTypeAlias { name, .. } => lookup
                    .expand_recursive_type(name)
                    .unwrap_or_else(|_| panic!("Failed to expand recursive type alias: {name}"))
                    .to_streaming_type(lookup),
                other => other.clone(),
            },
            |field_type| match field_type {
                TypeGeneric::Class { mode, .. } => match mode {
                    baml_types::StreamingMode::NonStreaming => CffiTypeNamespace::Types,
                    baml_types::StreamingMode::Streaming => CffiTypeNamespace::StreamTypes,
                },
                _ => CffiTypeNamespace::StreamTypes,
            },
        )
    }

    fn maybe_wrap_stream_state(
        &self,
        holder: CffiValueHolder,
        lookup: &TypeLookups,
    ) -> CffiValueHolder {
        if self.field_type().meta().streaming_behavior.state {
            let stream_state = CffiValueStreamingState {
                value: Some(Box::new(holder)),
                state: CffiStreamState::Pending.into(),
            };
            CffiValueHolder {
                // Not present for Streaming State
                r#type: None,
                value: Some(cffi_value_holder::Value::StreamingStateValue(Box::new(
                    stream_state,
                ))),
            }
        } else {
            holder
        }
    }
}

impl<'a, TypeLookups, T> Encode<CffiValueHolder>
    for WithIr<'a, BamlValueWithMeta<Meta<'_, T>>, TypeLookups>
where
    TypeLookups: baml_types::baml_value::TypeLookups + 'a,
    for<'b> BamlValueWithMeta<Meta<'b, T>>: TypeQuery<T> + MaybeWrapUnion<TypeLookups>,
    TypeGeneric<T>: std::fmt::Display,
    T: std::hash::Hash + std::cmp::Eq,
{
    fn encode(self) -> CffiValueHolder {
        use cffi_value_holder::Value;
        let WithIr { value, lookup } = self;

        let holder = {
            let encoded_value = match value {
                BamlValueWithMeta::String(val, _) => Value::StringValue(val.clone()),
                BamlValueWithMeta::Int(val, _) => Value::IntValue(*val),
                BamlValueWithMeta::Float(val, _) => Value::FloatValue(*val),
                BamlValueWithMeta::Bool(val, _) => Value::BoolValue(*val),
                BamlValueWithMeta::Map(index_map, _) => {
                    let TypeGeneric::Map(key_type, value_type, _) = value.real_type(lookup) else {
                        panic!("Map type is not a map");
                    };

                    let map = CffiValueMap {
                        entries: index_map
                            .iter()
                            .map(|(key, value)| CffiMapEntry {
                                key: key.clone(),
                                value: Some(WithIr { value, lookup }.encode()),
                            })
                            .collect(),
                        key_type: Some(
                            WithIr {
                                value: key_type.as_ref(),
                                lookup,
                            }
                            .encode(),
                        ),
                        value_type: Some(
                            WithIr {
                                value: value_type.as_ref(),
                                lookup,
                            }
                            .encode(),
                        ),
                    };
                    Value::MapValue(map)
                }
                BamlValueWithMeta::List(items, ..) => {
                    let TypeGeneric::List(value_type, _) = value.real_type(lookup) else {
                        panic!("List type is not a list");
                    };

                    let value_type = WithIr {
                        value: value_type.as_ref(),
                        lookup,
                    }
                    .encode();

                    Value::ListValue(CffiValueList {
                        value_type: Some(value_type),
                        values: items
                            .iter()
                            .map(|item| {
                                WithIr {
                                    value: item,
                                    lookup,
                                }
                                .encode()
                            })
                            .collect(),
                    })
                }
                BamlValueWithMeta::Media(_baml_media, _) => {
                    unimplemented!(
                        "BAML doesn't yet support emitting media values to external runtimes"
                    )
                }
                BamlValueWithMeta::Enum(name, value, _) => Value::EnumValue(CffiValueEnum {
                    name: Some(create_cffi_type_name(name, CffiTypeNamespace::Types)),
                    value: value.clone(),
                    is_dynamic: false,
                }),
                BamlValueWithMeta::Class(name, index_map, _) => {
                    let TypeGeneric::Class { mode, .. } = value.real_type(lookup) else {
                        panic!("Class type is not a class");
                    };

                    Value::ClassValue(CffiValueClass {
                        name: Some(create_cffi_type_name(
                            name,
                            match mode {
                                baml_types::StreamingMode::NonStreaming => CffiTypeNamespace::Types,
                                baml_types::StreamingMode::Streaming => {
                                    CffiTypeNamespace::StreamTypes
                                }
                            },
                        )),
                        fields: index_map
                            .iter()
                            .map(|(key, value)| CffiMapEntry {
                                key: key.clone(),
                                value: Some(WithIr { value, lookup }.encode()),
                            })
                            .collect(),
                        dynamic_fields: vec![],
                    })
                }
                BamlValueWithMeta::Null(_) => Value::NullValue(CffiValueNull {}),
            };
            println!("encoded_value: {}", value.field_type());
            CffiValueHolder {
                r#type: Some(
                    WithIr {
                        value: value.field_type(),
                        lookup,
                    }
                    .encode(),
                ),
                value: Some(encoded_value),
            }
        };

        let meta = value.meta().checks;
        let checks = meta.iter().map(|f| f.encode()).collect::<Vec<_>>();

        let holder = if !checks.is_empty() {
            let checked_value = CffiValueChecked {
                value: Some(Box::new(holder)),
                checks,
            };

            CffiValueHolder {
                // Checks don't have a type
                r#type: None,
                value: Some(Value::CheckedValue(Box::new(checked_value))),
            }
        } else {
            holder
        };

        let holder = value.maybe_wrap_union(holder, lookup);
        value.maybe_wrap_stream_state(holder, lookup)
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
