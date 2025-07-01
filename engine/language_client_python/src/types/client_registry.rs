use std::str::FromStr;

use baml_runtime::client_registry;
use client_registry::ClientProvider;
use pyo3::{
    prelude::{pymethods, PyResult},
    IntoPyObjectExt, PyObject, Python,
};

use crate::{errors::BamlInvalidArgumentError, parse_py_type::parse_py_type};

crate::lang_wrapper!(ClientRegistry, client_registry::ClientRegistry);

impl Default for ClientRegistry {
    fn default() -> Self {
        Self::new()
    }
}

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
        let Some(args) = parse_py_type(options.into_bound(py).into_py_any(py)?, false)? else {
            return Err(BamlInvalidArgumentError::new_err(
                "Failed to parse args, perhaps you used a non-serializable type?",
            ));
        };
        let Some(args_map) = args.as_map_owned() else {
            return Err(BamlInvalidArgumentError::new_err(
                "Failed to parse args, perhaps you used a non-serializable type?",
            ));
        };

        let provider = match ClientProvider::from_str(&provider) {
            Ok(provider) => provider,
            Err(e) => {
                return Err(BamlInvalidArgumentError::new_err(format!(
                    "Invalid provider: {e:?}"
                )));
            }
        };

        let client_property =
            client_registry::ClientProperty::new(name, provider, retry_policy, args_map);

        self.inner.add_client(client_property);
        Ok(())
    }

    pub fn set_primary(&mut self, primary: String) {
        self.inner.set_primary(primary);
    }
}
