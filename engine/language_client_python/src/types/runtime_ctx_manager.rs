use pyo3::prelude::{pymethods, PyResult};
use pyo3::{PyObject, Python, ToPyObject};

use crate::parse_py_type::parse_py_type;
use crate::BamlError;

crate::lang_wrapper!(RuntimeContextManagerPy, baml_runtime::RuntimeContextManager);

#[pymethods]
impl RuntimeContextManagerPy {
    #[pyo3()]
    fn upsert_tags(&self, py: Python<'_>, tags: PyObject) -> PyResult<bool> {
        let tags = parse_py_type(tags.into_bound(py).to_object(py))?;
        let Some(tags) = tags.as_map_owned() else {
            return Err(BamlError::new_err("Failed to parse tags"));
        };
        self.inner.upsert_tags(tags.into_iter().collect());
        Ok(true)
    }

    #[pyo3()]
    fn deep_clone(&self) -> Self {
        RuntimeContextManagerPy {
            inner: self.inner.deep_clone(),
        }
    }
}
