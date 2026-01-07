//! Shared test utilities and fixtures for baml crate tests.
#![allow(dead_code)]

use baml::__internal::{
    cffi_value_holder, CffiCheckValue, CffiFieldTypeLiteral, CffiLiteralBool, CffiLiteralInt,
    CffiLiteralString, CffiMapEntry, CffiStreamState, CffiTypeName, CffiValueChecked,
    CffiValueClass, CffiValueEnum, CffiValueHolder, CffiValueList, CffiValueMap,
    CffiValueStreamingState, CffiValueUnionVariant,
};

/// Create a `CffiValueHolder` containing a string value.
pub(crate) fn make_string_holder(s: &str) -> CffiValueHolder {
    CffiValueHolder {
        value: Some(cffi_value_holder::Value::StringValue(s.to_string())),
    }
}

/// Create a `CffiValueHolder` containing an integer value.
pub(crate) fn make_int_holder(i: i64) -> CffiValueHolder {
    CffiValueHolder {
        value: Some(cffi_value_holder::Value::IntValue(i)),
    }
}

/// Create a `CffiValueHolder` containing a float value.
pub(crate) fn make_float_holder(f: f64) -> CffiValueHolder {
    CffiValueHolder {
        value: Some(cffi_value_holder::Value::FloatValue(f)),
    }
}

/// Create a `CffiValueHolder` containing a boolean value.
pub(crate) fn make_bool_holder(b: bool) -> CffiValueHolder {
    CffiValueHolder {
        value: Some(cffi_value_holder::Value::BoolValue(b)),
    }
}

/// Create a `CffiValueHolder` containing a list of values.
pub(crate) fn make_list_holder(items: Vec<CffiValueHolder>) -> CffiValueHolder {
    CffiValueHolder {
        value: Some(cffi_value_holder::Value::ListValue(CffiValueList {
            item_type: None,
            items,
        })),
    }
}

/// Create an empty/null `CffiValueHolder`.
pub(crate) fn make_null_holder() -> CffiValueHolder {
    CffiValueHolder { value: None }
}

/// Create a `CffiValueHolder` containing a checked value with checks.
pub(crate) fn make_checked_holder(
    inner: CffiValueHolder,
    checks: Vec<(&str, &str, &str)>,
) -> CffiValueHolder {
    let check_values = checks
        .into_iter()
        .map(|(name, expression, status)| CffiCheckValue {
            name: name.to_string(),
            expression: expression.to_string(),
            status: status.to_string(),
            value: None,
        })
        .collect();

    CffiValueHolder {
        value: Some(cffi_value_holder::Value::CheckedValue(Box::new(
            CffiValueChecked {
                name: None,
                value: Some(Box::new(inner)),
                checks: check_values,
            },
        ))),
    }
}

/// Create a `CffiValueHolder` containing a streaming state value.
pub(crate) fn make_stream_state_holder(
    inner: CffiValueHolder,
    state: CffiStreamState,
) -> CffiValueHolder {
    CffiValueHolder {
        value: Some(cffi_value_holder::Value::StreamingStateValue(Box::new(
            CffiValueStreamingState {
                value: Some(Box::new(inner)),
                state: state.into(),
                name: None,
            },
        ))),
    }
}

/// Create a `CffiValueHolder` containing a map value.
pub(crate) fn make_map_holder(entries: Vec<(&str, CffiValueHolder)>) -> CffiValueHolder {
    let map_entries = entries
        .into_iter()
        .map(|(key, value)| CffiMapEntry {
            key: key.to_string(),
            value: Some(value),
        })
        .collect();

    CffiValueHolder {
        value: Some(cffi_value_holder::Value::MapValue(CffiValueMap {
            key_type: None,
            value_type: None,
            entries: map_entries,
        })),
    }
}

/// Create a `CffiValueHolder` containing a class value.
pub(crate) fn make_class_holder(
    name: &str,
    fields: Vec<(&str, CffiValueHolder)>,
) -> CffiValueHolder {
    let field_entries = fields
        .into_iter()
        .map(|(key, value)| CffiMapEntry {
            key: key.to_string(),
            value: Some(value),
        })
        .collect();

    CffiValueHolder {
        value: Some(cffi_value_holder::Value::ClassValue(CffiValueClass {
            name: Some(CffiTypeName {
                namespace: 0,
                name: name.to_string(),
            }),
            fields: field_entries,
        })),
    }
}

/// Create a `CffiValueHolder` containing an enum value.
pub(crate) fn make_enum_holder(enum_name: &str, value: &str) -> CffiValueHolder {
    CffiValueHolder {
        value: Some(cffi_value_holder::Value::EnumValue(CffiValueEnum {
            name: Some(CffiTypeName {
                namespace: 0,
                name: enum_name.to_string(),
            }),
            value: value.to_string(),
            is_dynamic: false,
        })),
    }
}

/// Create a `CffiValueHolder` containing a union variant value.
pub(crate) fn make_union_holder(
    union_name: &str,
    variant_name: &str,
    inner: CffiValueHolder,
) -> CffiValueHolder {
    CffiValueHolder {
        value: Some(cffi_value_holder::Value::UnionVariantValue(Box::new(
            CffiValueUnionVariant {
                name: Some(CffiTypeName {
                    namespace: 0,
                    name: union_name.to_string(),
                }),
                is_optional: false,
                is_single_pattern: false,
                self_type: None,
                value_option_name: variant_name.to_string(),
                value: Some(Box::new(inner)),
            },
        ))),
    }
}

/// Create a `CffiValueHolder` containing a string literal value.
pub(crate) fn make_literal_string_holder(s: &str) -> CffiValueHolder {
    CffiValueHolder {
        value: Some(cffi_value_holder::Value::LiteralValue(
            CffiFieldTypeLiteral {
                literal: Some(cffi_field_type_literal::Literal::StringLiteral(
                    CffiLiteralString {
                        value: s.to_string(),
                    },
                )),
            },
        )),
    }
}

/// Create a `CffiValueHolder` containing an int literal value.
pub(crate) fn make_literal_int_holder(i: i64) -> CffiValueHolder {
    CffiValueHolder {
        value: Some(cffi_value_holder::Value::LiteralValue(
            CffiFieldTypeLiteral {
                literal: Some(cffi_field_type_literal::Literal::IntLiteral(
                    CffiLiteralInt { value: i },
                )),
            },
        )),
    }
}

/// Create a `CffiValueHolder` containing a bool literal value.
pub(crate) fn make_literal_bool_holder(b: bool) -> CffiValueHolder {
    CffiValueHolder {
        value: Some(cffi_value_holder::Value::LiteralValue(
            CffiFieldTypeLiteral {
                literal: Some(cffi_field_type_literal::Literal::BoolLiteral(
                    CffiLiteralBool { value: b },
                )),
            },
        )),
    }
}

use baml::__internal::cffi_field_type_literal;
