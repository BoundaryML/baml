//! Shared test utilities and fixtures for baml crate tests.

use baml::__internal::{
    cffi_value_holder, CffiCheckValue, CffiStreamState, CffiValueChecked, CffiValueHolder,
    CffiValueList, CffiValueStreamingState,
};

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

/// Create a `CffiValueHolder` containing a checked value with checks.
pub fn make_checked_holder(
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
pub fn make_stream_state_holder(inner: CffiValueHolder, state: CffiStreamState) -> CffiValueHolder {
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
