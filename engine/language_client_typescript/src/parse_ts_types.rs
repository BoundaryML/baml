use baml_types::BamlMap;
use baml_types::BamlValue;
use napi::bindgen_prelude::*;
use napi::JsBoolean;

use napi::JsDate;
use napi::JsExternal;
use napi::JsNumber;
use napi::JsObject;
use napi::JsString;
use napi::JsUnknown;
use napi::NapiRaw;

use crate::types::image::BamlImage;

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

// use the FromNapiValue implementation for serde_json::Number here
// https://github.com/napi-rs/napi-rs/blob/b2239fd880fa40fa98d206d8f31aec1bb8a0ce12/crates/napi/src/bindgen_runtime/js_values/serde.rs#L147
fn from_napi_number(env: Env, napi_val: JsNumber) -> Result<BamlValue> {
    let n = unsafe { f64::from_napi_value(env.raw(), napi_val.raw())? };
    // Try to auto-convert to integers
    let n = if n.trunc() == n {
        if n >= 0.0f64 && n <= u32::MAX as f64 {
            // This can be represented as u32
            Some(BamlValue::Int(n as i64))
        } else if n < 0.0f64 && n >= i32::MIN as f64 {
            Some(BamlValue::Int(n as i64))
        } else {
            // must be a float
            Some(BamlValue::Float(n))
        }
    } else {
        // must be a float
        Some(BamlValue::Float(n))
    };

    let n = n.ok_or_else(|| {
        Error::new(
            Status::InvalidArg,
            "Unexpected JsNumber type, expected int or float".to_owned(),
        )
    })?;

    Ok(n)
}

pub fn js_object_to_baml_value(env: Env, kwargs: JsObject) -> napi::Result<BamlValue> {
    if kwargs.is_array()? || kwargs.is_typedarray()? || kwargs.is_dataview()? {
        let len = kwargs.get_array_length()?;
        let mut args = Vec::with_capacity(len as usize);
        let mut errs = Vec::new();
        for i in 0..len {
            let item = kwargs.get_element(i)?;
            match jsunknown_to_baml_value(env, item, false) {
                Ok(Some(v)) => args.push(v),
                Ok(None) => {}
                Err(e) => errs.push(SerializationError {
                    position: vec![format!("index {}", i)],
                    message: e.to_string(),
                }),
            }
        }

        if !errs.is_empty() {
            return Err(Errors { errors: errs }.into());
        }
        Ok(BamlValue::List(args))
    } else if kwargs.is_date()? {
        let date: JsDate = unsafe { kwargs.into_unknown().cast() };
        let timestamp = date.value_of()?;
        // TODO: Convert timestamp to a DateTime
        Ok(BamlValue::Float(timestamp))
    } else {
        let mut args = BamlMap::new();

        // Use the defined serialization method if it exists
        // if let Ok(to_json) = kwargs.get_named_property::<JsFunction>("toJSON") {
        //     let json = to_json.call_without_args(Some(&kwargs))?;
        //     if let Ok(Some(v)) = jsunknown_to_baml_value(env, json, false) {
        //         return Ok(v);
        //     }
        // }

        let keys = kwargs.get_property_names()?;
        let num_keys = keys.get_array_length()?;
        let mut errs = Vec::new();

        log::trace!("Processing object with {} keys", num_keys);
        for i in 0..num_keys {
            let key = keys.get_element::<JsString>(i)?;
            let param: JsUnknown = kwargs.get_property(key)?;
            let key_as_string = key.into_utf8()?.as_str()?.to_string();

            log::trace!("Processing key: {}", key_as_string);
            match jsunknown_to_baml_value(env, param, true) {
                Ok(Some(v)) => {
                    args.insert(key_as_string, v);
                }
                Ok(None) => {}
                Err(e) => errs.push(SerializationError {
                    position: vec![key_as_string],
                    message: e.to_string(),
                }),
            };
        }

        if !errs.is_empty() {
            return Err(Errors { errors: errs }.into());
        }

        Ok(BamlValue::Map(args))
    }
}

pub fn jsunknown_to_baml_value(
    env: Env,
    item: JsUnknown,
    skip_unsupported: bool,
) -> napi::Result<Option<BamlValue>> {
    let item_type = item.get_type()?;
    log::trace!("Processing item of type: {:?}", item_type);
    Ok(Some(match item_type {
        ValueType::Boolean => {
            let b: JsBoolean = unsafe { item.cast() };
            BamlValue::Bool(b.get_value()?)
        }
        ValueType::Number => {
            let n: JsNumber = unsafe { item.cast() };
            from_napi_number(env, n)?
        }
        ValueType::String => {
            let s: JsString = unsafe { item.cast() };
            BamlValue::String(s.into_utf8()?.as_str()?.to_string())
        }
        ValueType::Object => {
            let obj: JsObject = unsafe { item.cast() };
            js_object_to_baml_value(env, obj)?
        }
        ValueType::Undefined | ValueType::Null => BamlValue::Null,
        ValueType::Symbol => {
            if skip_unsupported {
                return Ok(None);
            }
            return Err(napi::Error::from_reason(
                "JsSymbol cannot be passed to BAML methods",
            ));
        }
        ValueType::Function => {
            if skip_unsupported {
                return Ok(None);
            }
            return Err(napi::Error::from_reason(
                "JsFunction cannot be passed to BAML methods",
            ));
        }
        ValueType::External => {
            let external = unsafe { item.cast::<JsExternal>() };
            if let Ok(img) = env.get_value_external::<BamlImage>(&external) {
                BamlValue::Image(img.inner.clone())
            } else {
                if skip_unsupported {
                    return Ok(None);
                }
                return Err(napi::Error::from_reason(
                    "JsExternal cannot be passed to BAML methods",
                ));
            }
        }
        ValueType::Unknown => {
            if skip_unsupported {
                return Ok(None);
            }
            return Err(napi::Error::from_reason(
                "JsUnknown cannot be passed to BAML methods",
            ));
        }
    }))
}
