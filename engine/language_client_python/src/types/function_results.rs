use anyhow::Context;
use baml_types::{BamlValue, BamlValueWithMeta, ResponseCheck};
use pyo3::prelude::{pymethods, PyResult};
use pyo3::types::{PyAnyMethods, PyListMethods, PyModule};
use pyo3::{Bound, IntoPy, PyObject, Python};
use pythonize::pythonize;

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

        let parsed = pythonize_strict(py, parsed.clone(), &enum_module, &cls_module)?;

        Ok(parsed)
    }
}

fn pythonize_checks(
    py: Python<'_>,
    cls_module: &Bound<'_, PyModule>,
    checks: &Vec<ResponseCheck>
) -> PyResult<PyObject> {

    fn type_name_for_checks(checks: &Vec<ResponseCheck>) -> String {
        let mut name = "Checks".to_string();
        let mut check_names: Vec<String> = checks.iter().map(|ResponseCheck{name,..}| name).cloned().collect();
        check_names.sort();
        for check_name in check_names.iter() {
            name.push_str("__");
            name.push_str(check_name);
        }
        name
    }

    let checks_class_name = type_name_for_checks(checks);
    let checks_class = cls_module.getattr(checks_class_name.as_str())?;
    let properties_dict = pyo3::types::PyDict::new_bound(py);
    checks.iter().try_for_each(|ResponseCheck{name, expression, status}| {
        // Construct the Check.
        let check_class = cls_module.getattr("Check")?;
        let check_properties_dict = pyo3::types::PyDict::new_bound(py);
        check_properties_dict.set_item("name", name)?;
        check_properties_dict.set_item("expr", expression)?;
        check_properties_dict.set_item("status", status)?;
        let check_instance = check_class.call_method("model_validate", (check_properties_dict,), None)?;

        // Set the Checked__* field Check.
        properties_dict.set_item(name, check_instance)?;
        PyResult::Ok(())
    })?;

    let checks_instance = checks_class.call_method("model_validate", (properties_dict,), None)?;
    Ok(checks_instance.into())
}

fn pythonize_strict(
    py: Python<'_>,
    mut parsed: BamlValueWithMeta<Vec<ResponseCheck>>,
    enum_module: &Bound<'_, PyModule>,
    cls_module: &Bound<'_, PyModule>,
) -> PyResult<PyObject> {
    if !parsed.meta().is_empty() {
        // We are parsing the value into a Checked { value:, checks: } class.
        //
        // First construct `checks`.
        let meta = parsed.meta();
        let python_checks = pythonize_checks(py, cls_module, meta)?;

        // Second, strip off the `Checks` and pass the remaining value
        // to `python_strict` to compute `value`.
        *parsed.meta_mut() = vec![];
        let python_value = pythonize_strict(py, parsed, enum_module, cls_module)?;

        let properties_dict = pyo3::types::PyDict::new_bound(py);
        properties_dict.set_item("value", python_value)?;
        properties_dict.set_item("checks", python_checks)?;

        let class_checked_type = cls_module.getattr("Checked")?;
        let checked_instance = class_checked_type.call_method("model_validate", (properties_dict,), None)?;

        Ok(checked_instance.into())
    } else {
        match parsed {
            BamlValueWithMeta::String(val,_) => Ok(val.into_py(py)),
            BamlValueWithMeta::Int(val, _) => Ok(val.into_py(py)),
            BamlValueWithMeta::Float(val, _) => Ok(val.into_py(py)),
            BamlValueWithMeta::Bool(val, _) => Ok(val.into_py(py)),
            BamlValueWithMeta::Map(index_map, _) => {
                let dict = pyo3::types::PyDict::new_bound(py);
                for (key, value) in index_map {
                    let key = key.into_py(py);
                    let value = pythonize_strict(py, value, enum_module, cls_module)?;
                    dict.set_item(key, value)?;
                }
                Ok(dict.into())
            }
            BamlValueWithMeta::List(vec, _) => Ok(pyo3::types::PyList::new_bound(
                py,
                vec.into_iter()
                    .map(|v| pythonize_strict(py, v, enum_module, cls_module))
                    .collect::<PyResult<Vec<_>>>()?,
            )
            .into()),
            BamlValueWithMeta::Media(baml_media, _) => match baml_media.media_type {
                baml_types::BamlMediaType::Image => {
                    Ok(BamlImagePy::from(baml_media.clone()).into_py(py))
                }
                baml_types::BamlMediaType::Audio => {
                    Ok(BamlAudioPy::from(baml_media.clone()).into_py(py))
                }
            },
            BamlValueWithMeta::Enum(enum_name, value, _) => {
                let enum_type = match enum_module.getattr(enum_name.as_str()) {
                    Ok(e) => e,
                    // This can be true in the case of dynamic types.
                    Err(_) => return Ok(enum_name.into_py(py)),
                };

                // Call the constructor with the value
                let instance = enum_type.call1((value,))?;
                Ok(instance.into())
            }
            BamlValueWithMeta::Class(class_name, index_map, _) => {
                let properties = index_map
                    .into_iter()
                    .map(|(key, value)| {
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
            BamlValueWithMeta::Null(_) => Ok(py.None()),
        }
    }
}
