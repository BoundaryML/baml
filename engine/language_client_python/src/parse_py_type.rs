use std::collections::HashMap;

use crate::python_types::BamlImagePy;
use anyhow::{bail, Result};
use baml_types::{BamlImage, BamlMap, BamlValue};
use pyo3::{
    exceptions::{PyRuntimeError, PyTypeError},
    types::{PyDict, PyList},
    PyErr, PyObject, PyResult, Python, ToPyObject,
};

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

impl Into<PyErr> for Errors {
    fn into(self) -> PyErr {
        let errs = self.errors;
        match errs.len() {
            0 => PyRuntimeError::new_err(
                "Unexpected error! Report this bug to github.com/boundaryml/baml (code: pyo3-zero)",
            ),
            1 => PyTypeError::new_err(errs.get(0).unwrap().to_string()),
            _ => {
                let mut message = format!("{} errors occurred:\n", errs.len());
                for err in errs {
                    message.push_str(&format!(" - {}\n", err.to_string()));
                }
                PyTypeError::new_err(message)
            }
        }
    }
}

enum MappedPyType {
    Enum(String, String),
    Class(String, HashMap<String, PyObject>),
    Map(HashMap<String, PyObject>),
    List(Vec<PyObject>),
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    None,
    BamlImage(BamlImagePy),
    Unsupported(String),
}

impl TryFrom<BamlImagePy> for BamlImage {
    type Error = &'static str;

    fn try_from(value: BamlImagePy) -> Result<Self, Self::Error> {
        match value {
            BamlImagePy {
                url: Some(url),
                base64: None,
                media_type: None,
            } => Ok(BamlImage::url(url)),
            BamlImagePy {
                url: None,
                base64: Some(base64),
                media_type: Some(media_type),
            } => Ok(BamlImage::base64(base64, media_type)),
            _ => Err("Invalid BamlImagePy"),
        }
    }
}
fn pyobject_to_json<'py, F>(
    any: PyObject,
    py: Python<'py>,
    to_type: &mut F,
    prefix: Vec<String>,
) -> Result<BamlValue, Vec<SerializationError>>
where
    F: FnMut(Python<'py>, PyObject) -> Result<MappedPyType>,
{
    let infered = match to_type(py, any) {
        Ok(infered) => infered,
        Err(e) => {
            return Err(vec![SerializationError {
                position: vec![],
                message: format!("Failed to parse type: {}", e),
            }])
        }
    };
    match infered {
        MappedPyType::Enum(e, value) => Ok(BamlValue::Enum(e, value)),
        MappedPyType::Class(c, kvs) => {
            let mut errs = vec![];
            let mut obj = BamlMap::new();
            for (k, v) in kvs {
                let mut prefix = prefix.clone();
                prefix.push(k.clone());
                match pyobject_to_json(v, py, to_type, prefix) {
                    Ok(v) => {
                        obj.insert(k, v);
                    }
                    Err(e) => errs.extend(e),
                };
            }
            if !errs.is_empty() {
                Err(errs)
            } else {
                Ok(BamlValue::Class(c, obj))
            }
        }
        MappedPyType::Map(kvs) => {
            let mut errs = vec![];
            let mut obj = BamlMap::new();
            for (k, v) in kvs {
                let mut prefix = prefix.clone();
                prefix.push(k.clone());
                match pyobject_to_json(v, py, to_type, prefix) {
                    Ok(v) => {
                        obj.insert(k, v);
                    }
                    Err(e) => errs.extend(e),
                };
            }
            if !errs.is_empty() {
                Err(errs)
            } else {
                Ok(BamlValue::Map(obj))
            }
        }
        MappedPyType::List(items) => {
            let mut errs = vec![];
            let mut arr = vec![];
            let mut count = 0;
            for item in items {
                let mut prefix = prefix.clone();
                prefix.push(count.to_string());
                match pyobject_to_json(item, py, to_type, prefix) {
                    Ok(v) => arr.push(v),
                    Err(e) => errs.extend(e),
                }
                count += 1;
            }
            if !errs.is_empty() {
                Err(errs)
            } else {
                Ok(BamlValue::List(arr))
            }
        }
        MappedPyType::String(v) => Ok(BamlValue::String(v)),
        MappedPyType::Int(v) => Ok(BamlValue::Int(v)),
        MappedPyType::Float(v) => Ok(BamlValue::Float(v)),
        MappedPyType::Bool(v) => Ok(BamlValue::Bool(v)),
        MappedPyType::BamlImage(v) => {
            if let Ok(v) = BamlImage::try_from(v) {
                Ok(BamlValue::Image(v))
            } else {
                Err(vec![SerializationError {
                    position: prefix,
                    message: "Invalid BamlImagePy".to_string(),
                }])
            }
        }
        MappedPyType::None => Ok(BamlValue::Null),
        MappedPyType::Unsupported(r#type) => Err(vec![SerializationError {
            position: prefix,
            message: format!("Unsupported type: {}", r#type),
        }]),
    }
}

pub fn parse_py_type(any: PyObject) -> PyResult<BamlValue> {
    Python::with_gil(|py| {
        let enum_type = py.import("enum").and_then(|m| m.getattr("Enum"))?;
        let base_model = py.import("pydantic").and_then(|m| m.getattr("BaseModel"))?;

        let mut get_type = |py: Python<'_>, any: PyObject| -> Result<MappedPyType> {
            // Call the type() function on the object
            let t = any.as_ref(py).get_type();
            // let t = any.bind_borrowed(py).get_type();
            // let t = t.as_gil_ref();

            if t.is_subclass(enum_type).unwrap_or(false) {
                let name = t
                    .name()
                    .map(|n| n.to_string())
                    .unwrap_or("<UnnamedEnum>".to_string());
                let value = any.getattr(py, "value")?;
                let value = value.extract::<String>(py)?;
                Ok(MappedPyType::Enum(name, value))
            } else if t.is_subclass(base_model).unwrap_or(false) {
                let name = t
                    .name()
                    .map(|n| n.to_string())
                    .unwrap_or("<UnnamedBaseModel>".to_string());
                let fields = match t
                    .getattr("model_fields")?
                    .extract::<HashMap<String, PyObject>>()
                {
                    Ok(fields) => fields
                        .keys()
                        .filter_map(|k| {
                            let v = any.getattr(py, k.as_str());
                            if let Ok(v) = v {
                                Some((k.clone(), v))
                            } else {
                                None
                            }
                        })
                        .collect::<HashMap<_, _>>(),
                    Err(_) => {
                        bail!("model_fields is not a dict")
                    }
                };
                Ok(MappedPyType::Class(name, fields))
                // use downcast only
            } else if let Ok(list) = any.downcast::<PyList>(py) {
                let mut items = vec![];
                let len = list.len();
                for idx in 0..len {
                    items.push(list.get_item(idx)?.to_object(py));
                }
                Ok(MappedPyType::List(items))
            } else if let Ok(dict) = any.downcast::<PyDict>(py) {
                let kv = dict.extract()?;
                Ok(MappedPyType::Map(kv))
            } else if let Ok(s) = any.extract::<String>(py) {
                Ok(MappedPyType::String(s))
            } else if let Ok(i) = any.extract::<i64>(py) {
                Ok(MappedPyType::Int(i))
            } else if let Ok(f) = any.extract::<f64>(py) {
                Ok(MappedPyType::Float(f))
            } else if let Ok(b) = any.extract::<bool>(py) {
                Ok(MappedPyType::Bool(b))
            } else if any.is_none(py) {
                Ok(MappedPyType::None)
            } else if let Ok(b) = any.extract::<BamlImagePy>(py) {
                Ok(MappedPyType::BamlImage(b))
            } else {
                Ok(MappedPyType::Unsupported(format!("{:?}", t)))
            }
        };

        match pyobject_to_json(any, py, &mut get_type, vec![]) {
            Ok(v) => Ok(v),
            Err(errors) => Err((Errors { errors }).into()),
        }
    })
}

// fn pyobject_to_baml_arg_type<'py, F>(
//     any: PyObject,
//     py: Python<'py>,
//     to_type: &mut F,
//     prefix: Vec<String>,
// ) -> Result<BamlArgType, Vec<SerializationError>>
// where
//     F: FnMut(Python<'py>, PyObject) -> Result<MappedPyType>,
// {
//     let infered = match to_type(py, any) {
//         Ok(infered) => infered,
//         Err(e) => {
//             return Err(vec![SerializationError {
//                 position: vec![],
//                 message: format!("Failed to parse type: {}", e),
//             }])
//         }
//     };
//     match infered {
//         MappedPyType::Enum(name, value) => Ok(BamlArgType::Enum(name, value)),
//         MappedPyType::Class(name, kvs) => {
//             let mut errs = vec![];
//             let mut obj = IndexMap::new();
//             for (k, v) in kvs {
//                 let mut prefix = prefix.clone();
//                 prefix.push(k.clone());
//                 match pyobject_to_baml_arg_type(v, py, to_type, prefix) {
//                     Ok(v) => {
//                         obj.insert(k, v);
//                     }
//                     Err(e) => errs.extend(e),
//                 };
//             }
//             if !errs.is_empty() {
//                 Err(errs)
//             } else {
//                 Ok(BamlArgType::Class(name, obj))
//             }
//         }
//         MappedPyType::Map(kvs) => {
//             let mut errs = vec![];
//             let mut obj = IndexMap::new();
//             for (k, v) in kvs {
//                 let mut prefix = prefix.clone();
//                 prefix.push(k.clone());
//                 match pyobject_to_baml_arg_type(v, py, to_type, prefix) {
//                     Ok(v) => {
//                         obj.insert(k, v);
//                     }
//                     Err(e) => errs.extend(e),
//                 };
//             }
//             if !errs.is_empty() {
//                 Err(errs)
//             } else {
//                 Ok(BamlArgType::Map(obj))
//             }
//         }
//         MappedPyType::List(items) => {
//             let mut errs = vec![];
//             let mut arr = vec![];
//             let mut count = 0;
//             for item in items {
//                 let mut prefix = prefix.clone();
//                 prefix.push(count.to_string());
//                 match pyobject_to_baml_arg_type(item, py, to_type, prefix) {
//                     Ok(v) => arr.push(v),
//                     Err(e) => errs.extend(e),
//                 }
//                 count += 1;
//             }
//             if !errs.is_empty() {
//                 Err(errs)
//             } else {
//                 Ok(BamlArgType::List(arr))
//             }
//         }
//         MappedPyType::String(v) => Ok(BamlArgType::String(v)),
//         MappedPyType::Int(v) => Ok(BamlArgType::Int(v)),
//         MappedPyType::Float(v) => Ok(BamlArgType::Float(v)),
//         MappedPyType::Bool(v) => Ok(BamlArgType::Bool(v)),
//         MappedPyType::BamlImage(v) => Ok(internal_baml_jinja::BamlArgType::Image(v.into())),
//         MappedPyType::Unsupported(r#type) => Err(vec![SerializationError {
//             position: prefix,
//             message: format!("Unsupported type: {}", r#type),
//         }]),
//         MappedPyType::None => Ok(internal_baml_jinja::BamlArgType::None),
//     }
// }
