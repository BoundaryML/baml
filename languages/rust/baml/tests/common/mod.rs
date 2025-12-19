//! Shared test utilities and fixtures for baml crate tests.

use baml::__internal::{cffi_value_holder, CffiValueHolder, CffiValueList};

/// Create a `CffiValueHolder` containing a string value.
pub fn make_string_holder(s: &str) -> CffiValueHolder {
    CffiValueHolder {
        value: Some(cffi_value_holder::Value::StringValue(s.to_string())),
    }
}

/// Create a `CffiValueHolder` containing an integer value.
pub fn make_int_holder(i: i64) -> CffiValueHolder {
    CffiValueHolder {
        value: Some(cffi_value_holder::Value::IntValue(i)),
    }
}

/// Create a `CffiValueHolder` containing a float value.
pub fn make_float_holder(f: f64) -> CffiValueHolder {
    CffiValueHolder {
        value: Some(cffi_value_holder::Value::FloatValue(f)),
    }
}

/// Create a `CffiValueHolder` containing a boolean value.
pub fn make_bool_holder(b: bool) -> CffiValueHolder {
    CffiValueHolder {
        value: Some(cffi_value_holder::Value::BoolValue(b)),
    }
}

/// Create a `CffiValueHolder` containing a list of values.
pub fn make_list_holder(items: Vec<CffiValueHolder>) -> CffiValueHolder {
    CffiValueHolder {
        value: Some(cffi_value_holder::Value::ListValue(CffiValueList {
            item_type: None,
            items,
        })),
    }
}

/// Create an empty/null `CffiValueHolder`.
pub fn make_null_holder() -> CffiValueHolder {
    CffiValueHolder { value: None }
}
