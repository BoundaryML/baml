use baml_runtime::client_registry;
use pyo3::prelude::{pymethods, PyResult};
use pyo3::{PyObject, Python, ToPyObject};

use crate::errors::BamlInvalidArgumentError;
use crate::parse_py_type::parse_py_type;

crate::lang_wrapper!(ClientRegistry, client_registry::ClientRegistry);

#[pymethods]
impl ClientRegistry {
    #[new]
    pub fn new() -> Self {
        Self {
            inner: client_registry::ClientRegistry::new(),
        }
    }

    #[pyo3(signature = (name, provider, options, retry_policy = None))]
    pub fn add_llm_client(
        &mut self,
        py: Python<'_>,
        name: String,
        provider: String,
        options: PyObject,
        retry_policy: Option<String>,
    ) -> PyResult<()> {
        let Some(args) = parse_py_type(options.into_bound(py).to_object(py), false)? else {
            return Err(BamlInvalidArgumentError::new_err(
                "Failed to parse args, perhaps you used a non-serializable type?",
            ));
        };
        let Some(args_map) = args.as_map_owned() else {
            return Err(BamlInvalidArgumentError::new_err(
                "Failed to parse args, perhaps you used a non-serializable type?",
            ));
        };

        let client_property = baml_runtime::client_registry::ClientProperty {
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
