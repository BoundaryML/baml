use baml_types::{BamlValueWithMeta, ResponseCheck};
use jsonish::ResponseBamlValue;
use pyo3::prelude::{pymethods, PyResult};
use pyo3::types::{PyAnyMethods, PyDict, PyModule, PyTuple, PyType};
use pyo3::{Bound, IntoPyObject, IntoPyObjectExt, Py, PyAny, PyErr, PyObject, Python};

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
        partial_cls_module: Bound<'_, PyModule>,
        allow_partials: bool,
    ) -> PyResult<PyObject> {
        let parsed = self
            .inner
            .result_with_constraints_content()
            .map_err(BamlError::from_anyhow)?;

        let parsed = pythonize_strict(py, parsed.clone(), &enum_module, &cls_module, &partial_cls_module, allow_partials);

        Ok(parsed?)
    }
}

fn pythonize_checks<'a>(
    py: Python<'a>,
    types_module: &Bound<'_, PyModule>,
    checks: &[ResponseCheck],
) -> PyResult<Bound<'a, PyDict>> {
    let dict = PyDict::new(py);
    let check_class = types_module.getattr("Check")?;
    let check_class = check_class.downcast::<PyType>()?;
    checks.iter().try_for_each(
        |ResponseCheck {
             name,
             expression,
             status,
         }| {
            // Construct the Check.
            let check_properties_dict = pyo3::types::PyDict::new(py);
            check_properties_dict.set_item("name", name)?;
            check_properties_dict.set_item("expression", expression)?;
            check_properties_dict.set_item("status", status)?;
            let check_instance =
                check_class.call_method("model_validate", (check_properties_dict,), None)?;
            dict.set_item(name, check_instance)?;
            PyResult::Ok(())
        },
    )?;
    Ok(dict)
}

fn pythonize_strict(
    py: Python<'_>,
    parsed: ResponseBamlValue,
    enum_module: &Bound<'_, PyModule>,
    cls_module: &Bound<'_, PyModule>,
    partial_cls_module: &Bound<'_, PyModule>,
    allow_partials: bool,
) -> PyResult<PyObject> {
    let allow_partials = allow_partials && !parsed.0.meta().2.required_done;
    let meta = parsed.0.meta().clone();
    let py_value_without_constraints = match parsed.0 {
        BamlValueWithMeta::String(val, _) => val.into_py_any(py),
        BamlValueWithMeta::Int(val, _) => val.into_py_any(py),
        BamlValueWithMeta::Float(val, _) => val.into_py_any(py),
        BamlValueWithMeta::Bool(val, _) => val.into_py_any(py),
        BamlValueWithMeta::Map(index_map, _) => {
            let dict = pyo3::types::PyDict::new(py);
            for (key, value) in index_map {
                let key = key.into_pyobject(py)?;
                let value = pythonize_strict(py, ResponseBamlValue(value), enum_module, cls_module, partial_cls_module, allow_partials)?;
                dict.set_item(key, value)?;
            }
            Ok(dict.into())
        }
        BamlValueWithMeta::List(vec, _) => pyo3::types::PyList::new(
            py,
            vec.into_iter()
                .map(|v| pythonize_strict(py, ResponseBamlValue(v), enum_module, cls_module, partial_cls_module, allow_partials))
                .collect::<PyResult<Vec<_>>>()?,
        )?
        .into_py_any(py),
        BamlValueWithMeta::Media(baml_media, _) => match baml_media.media_type {
            baml_types::BamlMediaType::Image => {
                BamlImagePy::from(baml_media.clone()).into_py_any(py)
            }
            baml_types::BamlMediaType::Audio => {
                BamlAudioPy::from(baml_media.clone()).into_py_any(py)
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
                Err(_) => return value.into_py_any(py),
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
                    return value.into_py_any(py);
                }
            };
            Ok(instance.into())
        }
        BamlValueWithMeta::Class(class_name, index_map, _) => {
            let properties = index_map
                .into_iter()
                .map(|(key, value)| {
                    let subvalue_allow_partials = allow_partials && !value.meta().2.required_done;
                    let value = pythonize_strict(py, ResponseBamlValue(value), enum_module, cls_module, partial_cls_module, subvalue_allow_partials)?;
                    Ok((key.clone(), value))
                })
                .collect::<PyResult<Vec<_>>>()?;

            let properties_dict = pyo3::types::PyDict::new(py);
            for (key, value) in properties {
                // For each field, try to call pydantic's `model_dump` on the
                // field. This is necessary in case the field is `Checked[_,_]`,
                // because pydantic requires to parse such fields from json,
                // rather than from a Python object. The python object is an
                // untyped Dict, but python expects a typed `Checked`.
                // By turning such values into `json`, we allow pydantic's
                // parser to more flexibly accept input at its expected
                // type.
                //
                // This has the side-effect of calling `model_dump` on all
                // classes inheriting `BaseModel`, which probably incurs some
                // performance penalty. So we should consider testing whether
                // the field is a `Checked` before doing a `model_dump`.
                let value_model = value.call_method0(py, "model_dump");
                match value_model {
                    Err(_) => {
                        properties_dict.set_item(key, value)?;
                    }
                    Ok(m) => {
                        properties_dict.set_item(key, m)?;
                    }
                }
            }

            let target_class = if allow_partials { partial_cls_module } else { cls_module };
            let backup_class = if allow_partials { cls_module } else {partial_cls_module};
            let class_type = match target_class.getattr(class_name.as_str()) {
                Ok(class) => class,
                // This can be true in the case of dynamic types.
                /*
                    tb = TypeBuilder()
                    tb.add_class("Foo")
                */
                Err(_) => return Ok(properties_dict.into()),
            };

            let backup_class_type = match backup_class.getattr(class_name.as_str()) {
                Ok(class) => class,
                Err(_) => unreachable!("The return value for the Err case in class_type would have triggered before we reached this line."),
            };

            let instance = match class_type.call_method("model_validate", (properties_dict.clone(),), None) {
                Ok(x) => Ok(x),
                Err(original_error) => match backup_class_type.call_method("model_validate", (properties_dict.clone(),), None) {
                    Ok(x) => Ok(x),
                    Err(_) => Err(original_error)
                }
            }?;

            Ok(instance.into())
        }
        BamlValueWithMeta::Null(_) => Ok(py.None()),
    }?;

    let (_, checks, completion_state) = meta;
    if checks.is_empty() && !completion_state.display {
        Ok(py_value_without_constraints)
    } else {

        // Import the necessary modules and objects
        let typing = py.import("typing").expect("typing");
        let literal = typing.getattr("Literal").expect("Literal");
        let value_with_possible_checks = if !checks.is_empty() {

            // Generate the Python checks
            let python_checks = pythonize_checks(py, cls_module, &checks).expect("pythonize_checks");

            // Get the type of the original value
            let value_type = py_value_without_constraints.bind(py).get_type();


            // Collect check names as &str and turn them into a Python tuple
            let check_names: Vec<&str> = checks.iter().map(|check| check.name.as_str()).collect();
            let literal_args = PyTuple::new_bound(py, check_names);

            // Call Literal[...] dynamically
            let literal_check_names = literal.get_item(literal_args).expect("get_item");


            let class_checked_type_constructor = cls_module.getattr("Checked").expect("getattr(Checked)");

            // Prepare type parameters for Checked[...]
            let type_parameters_tuple = PyTuple::new(py, [value_type.as_ref(), &literal_check_names]).expect("PyTuple::new");

            // Create the Checked type using __class_getitem__
            let class_checked_type: Bound<'_, PyAny> = class_checked_type_constructor
                .call_method1("__class_getitem__", (type_parameters_tuple,)).expect("__class_getitem__");

            // Prepare the properties dictionary
            let properties_dict = pyo3::types::PyDict::new(py);
            properties_dict.set_item("value", py_value_without_constraints)?;
            if !checks.is_empty() {
                properties_dict.set_item("checks", python_checks)?;
            }

            // Validate the model with the constructed type
            let checked_instance =
                class_checked_type.call_method("model_validate", (properties_dict.clone(),), None).expect("model_validate");

            Ok::<Py<PyAny>, PyErr>(checked_instance.into())
        } else {
            Ok(py_value_without_constraints)
        }?;
    
        let value_with_possible_completion_state = if completion_state.display && allow_partials {
            let value_type = value_with_possible_checks.bind(py).get_type();

            // Prepare the properties dictionary
            let properties_dict = pyo3::types::PyDict::new(py);
            properties_dict.set_item("value", value_with_possible_checks)?;
            properties_dict.set_item("state", format!("{:?}", completion_state.state))?;

            // Prepare type parameters for StreamingState[...]
            let type_parameters_tuple = PyTuple::new(py, [value_type.as_ref()]).expect("PyTuple::new");

            let class_streaming_state_type_constructor = partial_cls_module.getattr("StreamState").expect("getattr(StreamState)");
            let class_completion_state_type: Bound<'_, PyAny> = class_streaming_state_type_constructor
                .call_method1("__class_getitem__", (type_parameters_tuple,))
                .expect("__class_getitem__ for streaming");

            let streaming_state_instance = class_completion_state_type
                .call_method("model_validate", (properties_dict.clone(),), None)
                .expect("model_validate for streaming");

            Ok::<Py<PyAny>, PyErr>(streaming_state_instance.into())
        } else {
            Ok(value_with_possible_checks)
        }?;

        Ok(value_with_possible_completion_state)
    }
}
