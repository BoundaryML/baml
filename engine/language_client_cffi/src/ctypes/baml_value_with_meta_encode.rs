use baml_runtime::TypeIR;
use baml_types::{
    baml_value::TypeQuery, ir_type::TypeGeneric, type_meta, BamlValueWithMeta, HasType, ToUnionName,
};

use crate::{
    baml::cffi::*,
    ctypes::utils::{Encode, IsChecked, UnionAllowance, WithIr},
};

fn create_cffi_type_name(name: impl ToString, namespace: CffiTypeNamespace) -> CffiTypeName {
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

// Encode for Types (moved from baml_type_encode.rs)
impl<'a, TypeLookups, T: IsChecked> Encode<CffiFieldTypeHolder>
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
            TypeGeneric::Union(_, _) => cType::UnionVariantType(CffiFieldTypeUnionVariant {
                name: todo!(),
                options: todo!(),
            }),
        }
        };

        CffiFieldTypeHolder {
            r#type: Some(c_type),
        }

        // let checks = curr_type.meta().checks();

        // TypeGeneric::Union(union_type_generic, _) => {
        //     let view = union_type_generic.view();
        //     match view {
        //         baml_types::ir_type::UnionTypeViewGeneric::Null => {
        //             cType::NullType(CffiFieldTypeNull {})
        //         }
        //         baml_types::ir_type::UnionTypeViewGeneric::Optional(type_generic) => {
        //             if matches!(allow_user_defined_unions, UnionAllowance::Disallow) {
        //                 cType::AnyType(CffiFieldTypeAny::default())
        //             } else {
        //                 let inner = WithIr {
        //                     value: &(type_generic, allow_user_defined_unions),
        //                     lookup,
        //                     mode,
        //                     curr_type: type_generic.clone(),
        //                 }
        //                 .encode();
        //                 cType::OptionalType(Box::new(CffiFieldTypeOptional {
        //                     value: Some(Box::new(inner)),
        //                 }))
        //             }
        //         }
        //         baml_types::ir_type::UnionTypeViewGeneric::OneOf(type_generics) => {
        //             if matches!(allow_user_defined_unions, UnionAllowance::Disallow) {
        //                 cType::AnyType(CffiFieldTypeAny::default())
        //             } else {
        //                 let elements = type_generics
        //                     .into_iter()
        //                     .map(|t| {
        //                         WithIr {
        //                             value: &(t, allow_user_defined_unions),
        //                             lookup,
        //                             mode,
        //                             curr_type: t.clone(),
        //                         }
        //                         .encode()
        //                     })
        //                     .collect();
        //                 cType::UnionVariantType(CffiFieldTypeUnionVariant {
        //                     name: Some(CffiTypeName {
        //                         namespace: match value.mode(&mode, lookup) {
        //                             Ok(baml_types::StreamingMode::NonStreaming) => {
        //                                 CffiTypeNamespace::Types.into()
        //                             }
        //                             Ok(baml_types::StreamingMode::Streaming) => {
        //                                 CffiTypeNamespace::StreamTypes.into()
        //                             }
        //                             Err(e) => {
        //                                 panic!("Failed to get mode for field type: {e}");
        //                             }
        //                         },
        //                         name: value.to_union_name().to_string(),
        //                     }),
        //                     options: elements,
        //                 })
        //             }
        //         }
        //         baml_types::ir_type::UnionTypeViewGeneric::OneOfOptional(type_generics) => {
        //             if matches!(allow_user_defined_unions, UnionAllowance::Disallow) {
        //                 cType::AnyType(CffiFieldTypeAny::default())
        //             } else {
        //                 let elements = type_generics
        //                     .into_iter()
        //                     .map(|t| {
        //                         WithIr {
        //                             value: &(t, allow_user_defined_unions),
        //                             lookup,
        //                             mode,
        //                             curr_type: t.clone(),
        //                         }
        //                         .encode()
        //                     })
        //                     .collect();
        //                 let inner = cType::UnionVariantType(CffiFieldTypeUnionVariant {
        //                     name: Some(CffiTypeName {
        //                         namespace: match value.mode(&mode, lookup) {
        //                             Ok(baml_types::StreamingMode::NonStreaming) => {
        //                                 CffiTypeNamespace::Types.into()
        //                             }
        //                             Ok(baml_types::StreamingMode::Streaming) => {
        //                                 CffiTypeNamespace::StreamTypes.into()
        //                             }
        //                             Err(e) => {
        //                                 panic!("Failed to get mode for field type: {e}");
        //                             }
        //                         },
        //                         name: value.to_union_name().to_string(),
        //                     }),
        //                     options: elements,
        //                 });
        //                 let inner = CffiFieldTypeHolder {
        //                     r#type: Some(inner),
        //                 };
        //                 cType::OptionalType(Box::new(CffiFieldTypeOptional {
        //                     value: Some(Box::new(inner)),
        //                 }))
        //             }
        //         }
        //     }
        // }
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

impl<'a, TypeLookups, T: IsChecked + type_meta::MayHaveMeta> Encode<CffiValueHolder>
    for WithIr<'a, BamlValueWithMeta<Meta<'_, T>>, TypeLookups, T>
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

            let encoded_type = WithIr {
                value: &(&curr_type, UnionAllowance::Allow),
                lookup,
                mode,
                curr_type: inner_type.clone(),
            }
            .encode();
            return CffiValueHolder {
                value: Some(cffi_value_holder::Value::StreamingStateValue(Box::new(
                    CffiValueStreamingState {
                        value_type: Some(encoded_type),
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

            let encoded_type = WithIr {
                value: &(&curr_type, UnionAllowance::Allow),
                lookup,
                mode,
                curr_type: inner_type,
            }
            .encode();
            return CffiValueHolder {
                value: Some(cffi_value_holder::Value::CheckedValue(Box::new(
                    CffiValueChecked {
                        value_type: Some(encoded_type),
                        value: Some(Box::new(inner_holder)),
                        checks: check_result.collect(),
                    },
                ))),
            };
        }

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
            } = u.selected_type_index(&real_type, lookup).expect(&format!(
                "Failed to find target_type in options: {real_type} -> {curr_type}"
            ));

            if options.len() == 1 {
                if !real_type.is_null() {
                    panic!("Union has only one option and value is not null: {real_type} -> {curr_type}");
                }
                return inner_value;
            }

            let variant_name = options[value_type_index].to_union_name();
            let union_variant = CffiValueUnionVariant {
                name: Some(create_cffi_type_name(
                    curr_type.to_union_name().as_str(),
                    match curr_type
                        .mode(&mode, lookup, 1)
                        .expect("Failed to get mode for field type")
                    {
                        baml_types::StreamingMode::NonStreaming => CffiTypeNamespace::Types,
                        baml_types::StreamingMode::Streaming => CffiTypeNamespace::StreamTypes,
                    },
                )),
                option_types: options
                    .into_iter()
                    .map(|t| {
                        WithIr {
                            value: &(t, UnionAllowance::Allow),
                            lookup,
                            mode,
                            curr_type: t.clone(),
                        }
                        .encode()
                    })
                    .collect(),
                value_option_type_index: value_type_index as i32,
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
                    let TypeGeneric::Map(key_type, value_type, _) = &curr_type else {
                        panic!("Expected map type ir");
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
                    let TypeGeneric::List(item_type, _) = &curr_type else {
                        panic!("Expected list type ir");
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
                    let TypeGeneric::Enum { name, dynamic, .. } = &curr_type else {
                        panic!("Expected enum type ir");
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
                    let TypeGeneric::Class { name, .. } = &curr_type else {
                        panic!("Expected class type ir");
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

        return CffiValueHolder {
            value: Some(encoded_value),
        };
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
