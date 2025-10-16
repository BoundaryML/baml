use baml_types::{BamlMap, BamlValue};
use napi::{bindgen_prelude::*, JsDate, JsExternal, JsNumber, JsString, Unknown};

use crate::types::{audio::BamlAudio, image::BamlImage, pdf::BamlPdf, video::BamlVideo};

struct SerializationError {
    position: Vec<String>,
    message: String,
}

impl std::fmt::Display for SerializationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.position.is_empty() {
            f.write_str(&self.message)
        } else {
            write!(f, "{}: {}", self.position.join("."), self.message)
        }
    }
}

struct Errors {
    errors: Vec<SerializationError>,
}

impl From<Errors> for napi::Error {
    fn from(errors: Errors) -> Self {
        let errs = errors.errors;
        match errs.len() {
            0 => napi::Error::from_reason(
                "Unexpected error! Report this bug to github.com/boundaryml/baml (code: napi-zero)",
            ),
            1 => napi::Error::from_reason(errs.first().unwrap().to_string()),
            _ => {
                let mut message = format!("{} errors occurred:\n", errs.len());
                for err in errs {
                    message.push_str(&format!(" - {err}\n"));
                }
                napi::Error::from_reason(message)
            }
        }
    }
}

pub fn js_object_to_baml_value(env: &Env, kwargs: Object) -> napi::Result<BamlValue> {
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
        let date: JsDate =
            unsafe { JsDate::from_napi_value(env.raw(), kwargs.into_unknown(env)?.raw())? };
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

        log::trace!("Processing object with {num_keys} keys");
        for i in 0..num_keys {
            let key = keys.get_element::<JsString>(i)?;
            let param: Unknown = kwargs.get_property(key)?;
            let key_as_string = key.into_utf8()?.as_str()?.to_string();

            log::trace!("Processing key: {key_as_string}");
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
    env: &Env,
    item: Unknown,
    skip_unsupported: bool,
) -> napi::Result<Option<BamlValue>> {
    let item_type = item.get_type()?;
    log::trace!("Processing item of type: {item_type:?}");
    Ok(Some(match item_type {
        ValueType::Boolean => {
            let b: bool = unsafe { bool::from_napi_value(env.raw(), item.raw())? };
            BamlValue::Bool(b)
        }
        ValueType::Number => {
            let n: f64 = unsafe { f64::from_napi_value(env.raw(), item.raw())? };
            // Try to auto-convert to integers
            if n.trunc() == n {
                if n >= 0.0f64 && n <= u32::MAX as f64 {
                    // This can be represented as u32
                    BamlValue::Int(n as i64)
                } else if n < 0.0f64 && n >= i32::MIN as f64 {
                    BamlValue::Int(n as i64)
                } else {
                    // must be a float
                    BamlValue::Float(n)
                }
            } else {
                // must be a float
                BamlValue::Float(n)
            }
        }
        ValueType::String => {
            let s: String = unsafe { String::from_napi_value(env.raw(), item.raw())? };
            BamlValue::String(s)
        }
        ValueType::Object => {
            let obj: Object = unsafe { Object::from_napi_value(env.raw(), item.raw())? };
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
            let external = unsafe { JsExternal::from_napi_value(env.raw(), item.raw())? };

            if let Ok(img) = external.get_value::<BamlImage>() {
                BamlValue::Media(img.inner.clone())
            } else if let Ok(audio) = external.get_value::<BamlAudio>() {
                BamlValue::Media(audio.inner.clone())
            } else if let Ok(pdf) = external.get_value::<BamlPdf>() {
                BamlValue::Media(pdf.inner.clone())
            } else if let Ok(video) = external.get_value::<BamlVideo>() {
                BamlValue::Media(video.inner.clone())
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
