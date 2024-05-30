use pyo3::prelude::{pymethods, PyResult};
use pyo3::{PyObject, Python, ToPyObject};

use crate::parse_py_type::parse_py_type;
use crate::BamlError;

crate::lang_wrapper!(RuntimeContextManager, baml_runtime::RuntimeContextManager);

#[pymethods]
impl RuntimeContextManager {
    #[pyo3()]
    fn upsert_tags(&self, py: Python<'_>, tags: PyObject) -> PyResult<bool> {
        let Some(tags) = parse_py_type(tags.into_bound(py).to_object(py), true)? else {
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
    fn deep_clone(&self) -> Self {
        RuntimeContextManager {
            inner: self.inner.deep_clone(),
        }
    }
}
