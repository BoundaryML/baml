use napi::bindgen_prelude::*;
use napi::JsBoolean;

use napi::JsUnknown;

struct SerializationError {
    position: Vec<String>,
    message: String,
}

impl SerializationError {
    fn to_string(&self) -> String {
        if self.position.is_empty() {
            return self.message.clone();
        } else {
            format!("{}: {}", self.position.join("."), self.message)
        }
    }
}

struct Errors {
    errors: Vec<SerializationError>,
}

impl Into<napi::Error> for Errors {
    fn into(self) -> napi::Error {
        let errs = self.errors;
        match errs.len() {
            0 => napi::Error::from_reason(
                "Unexpected error! Report this bug to github.com/boundaryml/baml (code: napi-zero)",
            ),
            1 => napi::Error::from_reason(errs.get(0).unwrap().to_string()),
            _ => {
                let mut message = format!("{} errors occurred:\n", errs.len());
                for err in errs {
                    message.push_str(&format!(" - {}\n", err.to_string()));
                }
                napi::Error::from_reason(message)
            }
        }
    }
}

pub fn to_baml_arg_json(
    value: JsUnknown,
    position: Vec<String>,
) -> core::result::Result<serde_json::Value, Vec<SerializationError>> {
    let value_type = match value.get_type() {
        Err(e) => {
            return Err(vec![SerializationError {
                position: position.clone(),
                message: format!("Failed to resolve value type: {}", e),
            }])
        }
        Ok(value_type) => value_type,
    };

    let errs = vec![];

    use napi::ValueType;
    let _to_baml_arg = match value_type {
        ValueType::Undefined => todo!(),
        ValueType::Null => serde_json::Value::Null,
        ValueType::Boolean => match unsafe { value.cast::<JsBoolean>() }.get_value() {
            Ok(bool) => serde_json::Value::Bool(bool),
            Err(e) => {
                return Err(vec![SerializationError {
                    position: position.clone(),
                    message: format!("{}", e),
                }])
            }
        },
        ValueType::Number => {
            // use the FromNapiValue implementation for serde_json::Number here
            // https://github.com/napi-rs/napi-rs/blob/b2239fd880fa40fa98d206d8f31aec1bb8a0ce12/crates/napi/src/bindgen_runtime/js_values/serde.rs#L147
            todo!()
        }
        ValueType::Number => todo!(),
        ValueType::String => todo!(),
        ValueType::Object => todo!(),
        ValueType::External => todo!(),
        // -------------------
        ValueType::Symbol => todo!(),
        ValueType::Function => todo!(),
        ValueType::Unknown => todo!(),
    };

    if !errs.is_empty() {
        return Err(errs);
    }

    todo!()
    //Ok(to_baml_arg)
}
