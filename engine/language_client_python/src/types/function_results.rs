use baml_types::{BamlValueWithMeta, ResponseCheck};
use pyo3::prelude::{pymethods, PyResult};
use pyo3::types::{PyAnyMethods, PyDict, PyModule, PyTuple, PyType};
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
        self.inner.result_with_constraints_content().is_ok()
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
            .result_with_constraints_content()
            .map_err(BamlError::from_anyhow)?;

        let parsed = pythonize_strict(py, parsed.clone(), &enum_module, &cls_module)?;

        Ok(parsed)
    }
}

fn pythonize_checks<'a>(
    py: Python<'a>,
    baml_py: &Bound<'_, PyModule>,
    checks: &Vec<ResponseCheck>,
) -> PyResult<Bound<'a, PyDict>> {
    let dict = PyDict::new_bound(py);
    let check_class: &PyType = baml_py.getattr("Check")?.extract()?;
    checks.iter().try_for_each(|ResponseCheck{name, expression, status}| {
        // Construct the Check.
        let check_properties_dict = pyo3::types::PyDict::new_bound(py);
        check_properties_dict.set_item("name", name)?;
        check_properties_dict.set_item("expression", expression)?;
        check_properties_dict.set_item("status", status)?;
        let check_instance = check_class.call_method("model_validate", (check_properties_dict,), None)?;
        dict.set_item(name, check_instance)?;
        PyResult::Ok(())
    })?;
    Ok(dict)
}

fn pythonize_strict(
    py: Python<'_>,
    parsed: BamlValueWithMeta<Vec<ResponseCheck>>,
    enum_module: &Bound<'_, PyModule>,
    cls_module: &Bound<'_, PyModule>,
) -> PyResult<PyObject> {
    let baml_py = py.import_bound("baml_py")?;
    let meta = parsed.meta().clone();
    let py_value_without_constraints = match parsed {
        BamlValueWithMeta::String(val, _) => PyResult::Ok(val.into_py(py)),
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
        BamlValueWithMeta::Enum(enum_name, ref value, _) => {
            let enum_type = match enum_module.getattr(enum_name.as_str()) {
                Ok(e) => e,
                // This can be true in the case of dynamic types.
                /*
                   tb = TypeBuilder()
                   tb.add_enum("Foo")
                */
                Err(_) => return Ok(value.into_py(py)),
            };

            // Call the constructor with the value
            let instance = match enum_type.call1((value,)) {
                Ok(instance) => instance,
                Err(_) => {
                    // This can happen if the enum value is dynamic
                    /*
                       enum Foo {
                           @@dynamic
                       }
                    */
                    return Ok(value.into_py(py));
                }
            };
            Ok(instance.into())
        }
        BamlValueWithMeta::Class(class_name, index_map, _) => {
            let properties = index_map
                .into_iter()
                .map(|(key, value)| {
                    let value = pythonize_strict(py, value, enum_module, cls_module)?;
                    Ok((key.clone(), value))
                })
                .collect::<PyResult<Vec<_>>>()?;

            let properties_dict = pyo3::types::PyDict::new_bound(py);
            for (key, value) in properties {
                properties_dict.set_item(key, value)?;
            }

            let class_type = match cls_module.getattr(class_name.as_str()) {
                Ok(class) => class,
                // This can be true in the case of dynamic types.
                /*
                    tb = TypeBuilder()
                    tb.add_class("Foo")
                */
                Err(_) => return Ok(properties_dict.into()),
            };


            let instance = class_type.call_method("model_validate", (properties_dict,), None)?;
            Ok(instance.into())
        }
        BamlValueWithMeta::Null(_) => Ok(py.None()),
    }?;

    if meta.is_empty() {
        Ok(py_value_without_constraints)
    } else {

        // Generate the Python checks
        let python_checks = pythonize_checks(py, &baml_py, &meta)?;

        // Get the type of the original value
        let value_type = py_value_without_constraints.bind(py).get_type();

        // Import the necessary modules and objects
        let typing = py.import_bound("typing")?;
        let literal = typing.getattr("Literal")?;

        // Collect check names as &str and turn them into a Python tuple
        let check_names: Vec<&str> = meta.iter().map(|check| check.name.as_str()).collect();
        let literal_args = PyTuple::new_bound(py, check_names);

        // Call Literal[...] dynamically
        let literal_check_names = literal.get_item(literal_args)?;

        // Prepare the properties dictionary
        let properties_dict = pyo3::types::PyDict::new_bound(py);
        properties_dict.set_item("value", py_value_without_constraints)?;
        properties_dict.set_item("checks", python_checks)?;

        // Import the `baml_py` module and get the `Checked` constructor
        let baml_py = py.import_bound("baml_py")?;
        let class_checked_type_constructor = baml_py.getattr("Checked")?;

        // Prepare type parameters for Checked[...]
        let type_parameters_tuple = PyTuple::new_bound(py, &[value_type.as_ref(), &literal_check_names]);

        // Create the Checked type using __class_getitem__
        let class_checked_type = class_checked_type_constructor
            .call_method1("__class_getitem__", (type_parameters_tuple,))?;

        // Validate the model with the constructed type
        let checked_instance = class_checked_type.call_method("model_validate", (properties_dict,), None)?;

        Ok(checked_instance.into())
    }

}
