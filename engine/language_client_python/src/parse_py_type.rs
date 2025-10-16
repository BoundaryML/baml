use std::collections::{BTreeMap, HashMap};

use anyhow::Result;
use baml_types::{BamlMap, BamlValue};
use indexmap::IndexMap;
use pyo3::{
    exceptions::{PyRuntimeError, PyTypeError},
    prelude::{PyAnyMethods, PyTypeMethods},
    types::{PyBool, PyBoolMethods, PyDict, PyDictMethods, PyList},
    IntoPyObjectExt, PyErr, PyObject, PyResult, Python,
};

use crate::types::{BamlAudioPy, BamlImagePy, BamlPdfPy, BamlVideoPy};

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

impl From<Errors> for PyErr {
    fn from(errors: Errors) -> Self {
        let errs = errors.errors;
        match errs.len() {
            0 => PyRuntimeError::new_err(
                "Unexpected error! Report this bug to github.com/boundaryml/baml (code: pyo3-zero)",
            ),
            1 => PyTypeError::new_err(errs.first().unwrap().to_string()),
            _ => {
                let mut message = format!("{} errors occurred:\n", errs.len());
                for err in errs {
                    message.push_str(&format!(" - {err}\n"));
                }
                PyTypeError::new_err(message)
            }
        }
    }
}

enum MappedPyType {
    Enum(String, String),
    Class(String, IndexMap<String, PyObject>),
    Map(HashMap<String, PyObject>), // TODO: Does this need to maintain order?
    List(Vec<PyObject>),
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    None,
    BamlMedia(baml_types::BamlMedia),
    Unsupported(String),
}

// impl TryFrom<BamlImagePy> for BamlMedia {
//     type Error = &'static str;

//     fn try_from(value: BamlImagePy) -> Result<Self, Self::Error> {
//         Ok()
//     }
// }

// impl TryFrom<BamlAudioPy> for BamlMedia {
//     type Error = &'static str;

//     fn try_from(value: BamlAudioPy) -> Result<Self, Self::Error> {
//         Ok(Self {
//             media_type: baml_types::BamlMediaType::Audio,
//             content: value.inner.clone(),
//         })
//     }
// }

enum UnknownTypeHandler {
    Ignore,
    SerializeAsStr,
    Error,
}

fn pyobject_to_json<'py, F>(
    any: PyObject,
    py: Python<'py>,
    to_type: &mut F,
    prefix: Vec<String>,
    handle_unknown_types: &UnknownTypeHandler,
) -> Result<Option<BamlValue>, Vec<SerializationError>>
where
    F: FnMut(Python<'py>, PyObject, &UnknownTypeHandler) -> Result<MappedPyType>,
{
    let infered = match to_type(py, any, handle_unknown_types) {
        Ok(infered) => infered,
        Err(e) => {
            return Err(vec![SerializationError {
                position: vec![],
                message: format!("Failed to parse type: {e}"),
            }])
        }
    };
    Ok(Some(match infered {
        MappedPyType::Enum(e, value) => BamlValue::Enum(e, value),
        MappedPyType::Class(c, kvs) => {
            let mut errs = vec![];
            let mut obj = BamlMap::new();
            for (k, v) in kvs {
                let mut prefix = prefix.clone();
                prefix.push(k.clone());
                match pyobject_to_json(
                    v,
                    py,
                    to_type,
                    prefix,
                    match handle_unknown_types {
                        UnknownTypeHandler::Error => &UnknownTypeHandler::Ignore,
                        t => t,
                    },
                ) {
                    Ok(Some(v)) => {
                        obj.insert(k, v);
                    }
                    Ok(None) => {}
                    Err(e) => errs.extend(e),
                };
            }
            if !errs.is_empty() {
                return Err(errs);
            } else {
                BamlValue::Class(c, obj)
            }
        }
        MappedPyType::Map(kvs) => {
            let mut errs = vec![];
            let mut obj = BamlMap::new();
            for (k, v) in kvs {
                let mut prefix = prefix.clone();
                prefix.push(k.clone());
                match pyobject_to_json(
                    v,
                    py,
                    to_type,
                    prefix,
                    match handle_unknown_types {
                        UnknownTypeHandler::Error => &UnknownTypeHandler::Ignore,
                        t => t,
                    },
                ) {
                    Ok(Some(v)) => {
                        obj.insert(k, v);
                    }
                    Ok(None) => {}
                    Err(e) => errs.extend(e),
                };
            }
            if !errs.is_empty() {
                return Err(errs);
            } else {
                BamlValue::Map(obj)
            }
        }
        MappedPyType::List(items) => {
            let mut errs = vec![];
            let mut arr = vec![];
            for (count, item) in items.into_iter().enumerate() {
                let mut prefix = prefix.clone();
                prefix.push(count.to_string());
                match pyobject_to_json(item, py, to_type, prefix, handle_unknown_types) {
                    Ok(Some(v)) => arr.push(v),
                    Ok(None) => {}
                    Err(e) => errs.extend(e),
                }
            }
            if !errs.is_empty() {
                return Err(errs);
            } else {
                BamlValue::List(arr)
            }
        }
        MappedPyType::String(v) => BamlValue::String(v),
        MappedPyType::Int(v) => BamlValue::Int(v),
        MappedPyType::Float(v) => BamlValue::Float(v),
        MappedPyType::Bool(v) => BamlValue::Bool(v),
        MappedPyType::BamlMedia(media) => BamlValue::Media(media),
        MappedPyType::None => BamlValue::Null,
        MappedPyType::Unsupported(r#type) => {
            return if matches!(handle_unknown_types, UnknownTypeHandler::Ignore) {
                Ok(None)
            } else {
                Err(vec![SerializationError {
                    position: prefix,
                    message: format!("Unsupported type: {type}"),
                }])
            }
        }
    }))
}

pub fn parse_py_type(
    any: PyObject,
    serialize_unknown_types_as_str: bool,
) -> PyResult<Option<BamlValue>> {
    Python::with_gil(|py| {
        let enum_type = py.import("enum").and_then(|m| m.getattr("Enum"))?;
        let pydantic = py.import("pydantic")?;
        let base_model = pydantic.getattr("BaseModel")?;
        let is_pydantic_2 = {
            let pydantic_version = pydantic.getattr("version")?;
            let pydantic_version = pydantic_version.getattr("VERSION")?;
            // call the __str__ method on the object
            let pydantic_version = pydantic_version.call_method("__str__", (), None)?;
            let pydantic_version = pydantic_version.extract::<String>()?;
            pydantic_version.split(".").next().unwrap_or("0") >= "2"
        };

        let mut get_type = |py: Python<'_>,
                            any: PyObject,
                            unknown_type_handler: &UnknownTypeHandler|
         -> Result<MappedPyType> {
            // Call the type() function on the object
            let t = any.bind(py).get_type();
            // let t = any.bind_borrowed(py).get_type();
            // let t = t.as_gil_ref();

            if t.is_subclass(&enum_type).unwrap_or(false) {
                let name = t
                    .name()
                    .map(|n| {
                        let n = n.to_string();
                        match n.strip_prefix("baml_client.types.") {
                            Some(s) => s.to_string(),
                            None => n,
                        }
                    })
                    .unwrap_or("<UnnamedEnum>".to_string());
                let value = any.getattr(py, "value")?;
                let value = value.extract::<String>(py)?;
                Ok(MappedPyType::Enum(name, value))
            } else if t.is_subclass(&base_model).unwrap_or(false) {
                let name = t
                    .name()
                    .map(|n| {
                        let n = n.to_string();
                        match n.strip_prefix("baml_client.types.") {
                            Some(s) => s.to_string(),
                            None => n,
                        }
                    })
                    .unwrap_or("<UnnamedBaseModel>".to_string());
                let mut fields = IndexMap::new();
                // Get regular fields. Maintain order, no HashMap.
                if let Ok(model_fields) = if is_pydantic_2 {
                    t.getattr("model_fields")?
                        .extract::<BTreeMap<String, PyObject>>()
                } else {
                    let res = any.call_method0(py, "dict")?;
                    res.extract::<BTreeMap<String, PyObject>>(py)
                } {
                    for (key, _) in model_fields {
                        if let Ok(value) = any.getattr(py, key.as_str()) {
                            fields.insert(key, value.into_py_any(py)?);
                        }
                    }
                }

                // Get extra fields (like if this is a @@dynamic class)
                if let Ok(extra) = any.getattr(py, "__pydantic_extra__") {
                    if let Ok(extra_dict) = extra.downcast_bound::<PyDict>(py) {
                        for (key, value) in extra_dict.iter() {
                            if let (Ok(key), value) = (key.extract::<String>(), value) {
                                fields.insert(key, value.into_py_any(py)?);
                            }
                        }
                    }
                }

                Ok(MappedPyType::Class(name, fields))
                // use downcast only
            } else if let Ok(list) = any.downcast_bound::<PyList>(py) {
                let mut items = vec![];
                let len = list.len()?;
                for idx in 0..len {
                    items.push(list.get_item(idx)?.into_py_any(py)?);
                }
                Ok(MappedPyType::List(items))
            } else if let Ok(kv) = any.extract::<HashMap<String, PyObject>>(py) {
                Ok(MappedPyType::Map(kv))
            } else if let Ok(b) = any.downcast_bound::<PyBool>(py) {
                Ok(MappedPyType::Bool(b.is_true()))
            } else if let Ok(i) = any.extract::<i64>(py) {
                Ok(MappedPyType::Int(i))
            } else if let Ok(i) = any.extract::<u64>(py) {
                Ok(MappedPyType::Int(i as i64))
            } else if let Ok(f) = any.extract::<f64>(py) {
                Ok(MappedPyType::Float(f))
            } else if let Ok(s) = any.extract::<String>(py) {
                Ok(MappedPyType::String(s))
            } else if any.is_none(py) {
                Ok(MappedPyType::None)
            } else if let Ok(b) = any.downcast_bound::<BamlImagePy>(py) {
                let b = b.borrow();
                Ok(MappedPyType::BamlMedia(b.inner.clone()))
            } else if let Ok(b) = any.downcast_bound::<BamlAudioPy>(py) {
                let b = b.borrow();
                Ok(MappedPyType::BamlMedia(b.inner.clone()))
            } else if let Ok(b) = any.downcast_bound::<BamlPdfPy>(py) {
                let b = b.borrow();
                Ok(MappedPyType::BamlMedia(b.inner.clone()))
            } else if let Ok(b) = any.downcast_bound::<BamlVideoPy>(py) {
                let b = b.borrow();
                Ok(MappedPyType::BamlMedia(b.inner.clone()))
            } else if matches!(unknown_type_handler, UnknownTypeHandler::SerializeAsStr) {
                // Call the __str__ method on the object
                // Call the type() function on the object
                let t = any.bind(py).get_type();
                Ok(MappedPyType::String(format!("{t}: {any}")))
            } else {
                Ok(MappedPyType::Unsupported(format!("{t:?}")))
            }
        };

        let serialize_mode = if serialize_unknown_types_as_str {
            UnknownTypeHandler::SerializeAsStr
        } else {
            UnknownTypeHandler::Error
        };
        match pyobject_to_json(any, py, &mut get_type, vec![], &serialize_mode) {
            Ok(v) => Ok(v),
            Err(errors) => Err((Errors { errors }).into()),
        }
    })
}
