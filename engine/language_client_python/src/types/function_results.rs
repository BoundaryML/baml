use baml_types::BamlValue;
use pyo3::prelude::{pymethods, PyResult};
use pyo3::types::{PyAnyMethods, PyModule};
use pyo3::{Bound, IntoPy, PyObject, Python};

use crate::errors::BamlError;

use super::{BamlAudioPy, BamlImagePy};

crate::lang_wrapper!(FunctionResult, baml_runtime::FunctionResult);

#[pymethods]
impl FunctionResult {
    fn __str__(&self) -> String {
        format!("{:#}", self.inner)
    }

    fn is_ok(&self) -> bool {
        self.inner.parsed_content().is_ok()
    }

    /// This is a debug function that returns the internal representation of the response
    /// This is not to be relied upon and is subject to change
    fn unstable_internal_repr(&self) -> String {
        serde_json::json!(self.inner.llm_response()).to_string()
    }

    // Cast the parsed value to a specific type
    // the module is the module that the type is defined in
    fn cast_to(
        &self,
        py: Python<'_>,
        enum_module: Bound<'_, PyModule>,
        cls_module: Bound<'_, PyModule>,
    ) -> PyResult<PyObject> {
        let parsed = self
            .inner
            .parsed_content()
            .map_err(BamlError::from_anyhow)?;

        let parsed = BamlValue::from(parsed);
        let parsed = pythonize_strict(py, &parsed, &enum_module, &cls_module)?;

        Ok(parsed)
    }
}

fn pythonize_strict(
    py: Python<'_>,
    parsed: &BamlValue,
    enum_module: &Bound<'_, PyModule>,
    cls_module: &Bound<'_, PyModule>,
) -> PyResult<PyObject> {
    match parsed {
        BamlValue::String(val) => Ok(val.into_py(py)),
        BamlValue::Int(val) => Ok(val.into_py(py)),
        BamlValue::Float(val) => Ok(val.into_py(py)),
        BamlValue::Bool(val) => Ok(val.into_py(py)),
        BamlValue::Map(index_map) => {
            let dict = pyo3::types::PyDict::new_bound(py);
            for (key, value) in index_map {
                let key = key.into_py(py);
                let value = pythonize_strict(py, value, enum_module, cls_module)?;
                dict.set_item(key, value)?;
            }
            Ok(dict.into())
        }
        BamlValue::List(vec) => Ok(pyo3::types::PyList::new_bound(
            py,
            vec.iter()
                .map(|v| pythonize_strict(py, v, enum_module, cls_module))
                .collect::<PyResult<Vec<_>>>()?,
        )
        .into()),
        BamlValue::Media(baml_media) => match baml_media.media_type {
            baml_types::BamlMediaType::Image => {
                Ok(BamlImagePy::from(baml_media.clone()).into_py(py))
            }
            baml_types::BamlMediaType::Audio => {
                Ok(BamlAudioPy::from(baml_media.clone()).into_py(py))
            }
        },
        BamlValue::Enum(enum_name, value) => {
            let enum_type = match enum_module.getattr(enum_name.as_str()) {
                Ok(e) => e,
                // This can be true in the case of dynamic types.
                Err(_) => return Ok(value.into_py(py)),
            };

            // Call the constructor with the value
            let instance = enum_type.call1((value,))?;
            Ok(instance.into())
        }
        BamlValue::Class(class_name, index_map) => {
            let properties = index_map
                .iter()
                .map(|(key, value)| {
                    let key = key.as_str();
                    let value = pythonize_strict(py, value, enum_module, cls_module)?;
                    Ok((key, value))
                })
                .collect::<PyResult<Vec<_>>>()?;

            let properties_dict = pyo3::types::PyDict::new_bound(py);
            for (key, value) in properties {
                properties_dict.set_item(key, value)?;
            }

            let class_type = match cls_module.getattr(class_name.as_str()) {
                Ok(class) => class,
                // This can be true in the case of dynamic types.
                Err(_) => return Ok(properties_dict.into()),
            };
            let instance = class_type.call_method("model_validate", (properties_dict,), None)?;

            Ok(instance.into())
        }
        BamlValue::Null => Ok(py.None()),
    }
}
