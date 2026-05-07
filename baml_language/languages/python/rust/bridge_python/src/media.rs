//! PyO3 types for BAML media (`baml.media.{Image,Video,Audio,Pdf}`).
//!
//! Per 15b: each Python media class is a re-export of a Rust PyO3 type
//! that wraps a `BamlPyHandle` whose backing `HANDLE_TABLE` row is a
//! `CffiHandleTableEntry::Adt(BexExternalAdt::Media(arc))`. Static and
//! instance methods dispatch natively here instead of round-tripping
//! through the BAML engine.
//!
//! Hand-written for the four kinds; an IR-driven generator that produces
//! these from `baml_builtins2` is tracked separately. The duplication
//! across four `#[pymethods]` blocks is intentional and gives the
//! generator a concrete target to lower into.

use std::sync::Arc;

use bex_project::{BexExternalAdt, MediaKind, MediaValue};
use bridge_ctypes::{CffiHandleTableEntry, HANDLE_TABLE, baml_core::cffi::BamlHandleType};
use pyo3::{
    Bound, Py, PyAny, PyResult, Python,
    exceptions::{PyRuntimeError, PyTypeError},
    prelude::{PyModule, pyclass, pymethods},
    types::PyAnyMethods,
};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::py_handle::BamlPyHandle;

// `core_schema.is_instance_schema(cls)` produced via PyO3.
//
// Pydantic v2 looks for `__get_pydantic_core_schema__` on user-supplied
// types. Returning `is_instance_schema(cls)` tells Pydantic to validate
// the field by `isinstance(value, cls)` — exactly what we want for a
// PyO3 class that's already its own runtime check.
fn pydantic_is_instance_schema<'py>(
    py: Python<'py>,
    cls: Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let core_schema = py.import("pydantic_core")?.getattr("core_schema")?;
    core_schema.call_method1("is_instance_schema", (cls,))
}

// ---------------------------------------------------------------------------
// Per-kind PyO3 types. Four hand-written blocks; the duplication is
// intentional — see module doc.
// ---------------------------------------------------------------------------

macro_rules! define_media_pyclass {
    ($name:ident, $kind:expr, $expected_ht:expr) => {
        #[gen_stub_pyclass]
        #[pyclass]
        pub struct $name {
            pub(crate) handle: Py<BamlPyHandle>,
        }

        #[gen_stub_pymethods]
        #[pymethods]
        impl $name {
            #[staticmethod]
            #[pyo3(signature = (url, mime_type=None))]
            fn from_url(py: Python<'_>, url: String, mime_type: Option<String>) -> PyResult<Self> {
                let inner = MediaValue::from_url($kind, &url, mime_type.as_deref());
                let entry = CffiHandleTableEntry::Adt(BexExternalAdt::Media(inner));
                let key = HANDLE_TABLE.insert(entry);
                Ok(Self {
                    handle: Py::new(py, BamlPyHandle::new(key, $expected_ht))?,
                })
            }

            #[staticmethod]
            #[pyo3(signature = (file, mime_type=None))]
            fn from_file(
                py: Python<'_>,
                file: String,
                mime_type: Option<String>,
            ) -> PyResult<Self> {
                let inner = MediaValue::from_file($kind, &file, mime_type.as_deref());
                let entry = CffiHandleTableEntry::Adt(BexExternalAdt::Media(inner));
                let key = HANDLE_TABLE.insert(entry);
                Ok(Self {
                    handle: Py::new(py, BamlPyHandle::new(key, $expected_ht))?,
                })
            }

            #[staticmethod]
            #[pyo3(signature = (base64, mime_type=None))]
            fn from_base64(
                py: Python<'_>,
                base64: String,
                mime_type: Option<String>,
            ) -> PyResult<Self> {
                let inner = MediaValue::from_base64($kind, &base64, mime_type.as_deref());
                let entry = CffiHandleTableEntry::Adt(BexExternalAdt::Media(inner));
                let key = HANDLE_TABLE.insert(entry);
                Ok(Self {
                    handle: Py::new(py, BamlPyHandle::new(key, $expected_ht))?,
                })
            }

            fn url(&self, py: Python<'_>) -> PyResult<Option<String>> {
                Ok(self.media_arc(py)?.url())
            }

            fn file(&self, py: Python<'_>) -> PyResult<Option<String>> {
                Ok(self.media_arc(py)?.file())
            }

            fn base64(&self, py: Python<'_>) -> PyResult<String> {
                Ok(self.media_arc(py)?.base64())
            }

            fn mime_type(&self, py: Python<'_>) -> PyResult<Option<String>> {
                Ok(self.media_arc(py)?.mime_type())
            }

            /// Internal: build a `$name` from a `BamlPyHandle`. Used by
            /// `_decode_handle`. Validates the handle's `handle_type`
            /// tag matches the expected media kind.
            #[classmethod]
            fn _from_pyhandle(
                _cls: &Bound<'_, pyo3::types::PyType>,
                pyhandle: Py<BamlPyHandle>,
            ) -> PyResult<Self> {
                Python::attach(|py| {
                    let pyh = pyhandle.borrow(py);
                    if pyh.handle_type != $expected_ht {
                        return Err(PyTypeError::new_err(format!(
                            "BamlPyHandle.handle_type is {}, expected {} for {}",
                            pyh.handle_type,
                            $expected_ht,
                            stringify!($name),
                        )));
                    }
                    drop(pyh);
                    Ok(Self { handle: pyhandle })
                })
            }

            /// Internal: expose the inner `BamlPyHandle` for inbound encode.
            fn _to_pyhandle(&self, py: Python<'_>) -> Py<BamlPyHandle> {
                self.handle.clone_ref(py)
            }

            // Pydantic v2 hook so user models can declare fields like
            // `my_pdf: Pdf` without `arbitrary_types_allowed=True`.
            #[classmethod]
            fn __get_pydantic_core_schema__<'py>(
                cls: &Bound<'py, pyo3::types::PyType>,
                _source_type: &Bound<'py, PyAny>,
                _handler: &Bound<'py, PyAny>,
            ) -> PyResult<Bound<'py, PyAny>> {
                pydantic_is_instance_schema(cls.py(), cls.clone().into_any())
            }
        }

        impl $name {
            fn media_arc(&self, py: Python<'_>) -> PyResult<Arc<MediaValue>> {
                let pyh = self.handle.borrow(py);
                let entry = HANDLE_TABLE.resolve(pyh.handle_key).ok_or_else(|| {
                    PyRuntimeError::new_err(format!(
                        "media handle key {} no longer in HANDLE_TABLE",
                        pyh.handle_key
                    ))
                })?;
                match &*entry {
                    CffiHandleTableEntry::Adt(BexExternalAdt::Media(arc)) if arc.kind == $kind => {
                        Ok(Arc::clone(arc))
                    }
                    _ => Err(PyRuntimeError::new_err(
                        "media handle no longer points to a media value of the expected kind",
                    )),
                }
            }
        }
    };
}

define_media_pyclass!(
    BamlImage,
    MediaKind::Image,
    BamlHandleType::AdtMediaImage as u64
);
define_media_pyclass!(
    BamlAudio,
    MediaKind::Audio,
    BamlHandleType::AdtMediaAudio as u64
);
define_media_pyclass!(
    BamlVideo,
    MediaKind::Video,
    BamlHandleType::AdtMediaVideo as u64
);
define_media_pyclass!(BamlPdf, MediaKind::Pdf, BamlHandleType::AdtMediaPdf as u64);

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    use pyo3::types::PyModuleMethods;
    m.add_class::<BamlImage>()?;
    m.add_class::<BamlAudio>()?;
    m.add_class::<BamlVideo>()?;
    m.add_class::<BamlPdf>()?;
    Ok(())
}
