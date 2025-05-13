use pyo3::prelude::{pymethods, PyResult};
use pyo3::{IntoPyObjectExt, PyObject, Python};

use crate::errors::BamlError;
use crate::parse_py_type::parse_py_type;

crate::lang_wrapper!(RuntimeContextManager, baml_runtime::RuntimeContextManager);

#[pymethods]
impl RuntimeContextManager {
    #[pyo3()]
    fn upsert_tags(&self, py: Python<'_>, tags: PyObject) -> PyResult<bool> {
        let Some(tags) = parse_py_type(tags.into_bound(py).into_py_any(py)?, true)? else {
            // No tags to process
            return Ok(true);
        };
        let Some(tags) = tags.as_map_owned() else {
            return Err(BamlError::new_err("Failed to parse tags"));
        };
        self.inner.upsert_tags(tags.into_iter().collect());
        Ok(true)
    }

    #[pyo3()]
    fn update_env_vars(&self, py: Python<'_>, env_vars: PyObject) -> PyResult<bool> {
        let Some(env_vars) = parse_py_type(env_vars.into_bound(py).into_py_any(py)?, true)? else {
            // No env vars to process
            return Ok(true);
        };
        let Some(env_vars) = env_vars.as_map_owned() else {
            return Err(BamlError::new_err("Failed to parse environment variables"));
        };
        let _ =      self.inner.update_env_vars(
            env_vars
                .into_iter()
                .map(|(k, v)| (k, v.to_string()))
                .collect()
        );
        Ok(true)
    }

    #[pyo3()]
    fn deep_clone(&self) -> Self {
        RuntimeContextManager {
            inner: self.inner.deep_clone(),
        }
    }

    #[pyo3()]
    fn context_depth(&self) -> usize {
        self.inner.context_depth()
    }
}
