use std::collections::HashMap;

use baml_runtime::client_builder;
use baml_types::BamlValue;
use pyo3::prelude::{pymethods, PyResult};
use pyo3::{PyObject, Python, ToPyObject};

use crate::parse_py_type::parse_py_type;
use crate::BamlError;

crate::lang_wrapper!(ClientBuilder, client_builder::ClientBuilder);

#[pymethods]
impl ClientBuilder {
    #[new]
    pub fn new() -> Self {
        Self {
            inner: client_builder::ClientBuilder::new(),
        }
    }

    #[pyo3(signature = (name, provider, options, retry_policy))]
    pub fn add_client(
        &mut self,
        py: Python<'_>,
        name: String,
        provider: String,
        options: PyObject,
        retry_policy: Option<String>,
    ) -> PyResult<()> {
        let Some(args) = parse_py_type(options.into_bound(py).to_object(py), false)? else {
            return Err(BamlError::new_err(
                "Failed to parse args, perhaps you used a non-serializable type?",
            ));
        };
        let Some(args_map) = args.as_map_owned() else {
            return Err(BamlError::new_err(
                "Failed to parse args, perhaps you used a non-serializable type?",
            ));
        };

        let client_property = baml_runtime::client_builder::ClientProperty {
            name,
            provider,
            retry_policy,
            options: args_map,
        };

        self.inner.add_client(client_property);
        Ok(())
    }

    pub fn set_primary(&mut self, primary: String) {
        self.inner.set_primary(primary);
    }
}
