//! PyO3 types for BAML media (`baml.media.{Image,Video,Audio,Pdf}`).
//!
//! Per 15b: each Python media class is a re-export of a Rust PyO3 type
//! holding `Arc<MediaValue>`. Static and instance methods dispatch
//! natively here instead of round-tripping through the BAML engine.
//!
//! Hand-written for the four kinds; an IR-driven generator that produces
//! these from `baml_builtins2` is tracked separately. The duplication
//! across four `#[pymethods]` blocks is intentional and gives the
//! generator a concrete target to lower into.

use std::sync::Arc;

use bex_project::{BexExternalAdt, MediaKind, MediaValue};
use bridge_ctypes::{CffiHandleTableEntry, HANDLE_TABLE, baml::cffi::BamlHandleType};
use pyo3::{
    Bound, PyAny, PyResult, Python,
    exceptions::{PyRuntimeError, PyTypeError},
    prelude::{PyModule, pyclass, pymethods},
    types::PyAnyMethods,
};

use crate::handle::BamlHandle;

// Resolve a BamlHandle whose `handle_type` matches `expected_kind`,
// pull the inner Arc<MediaValue> out of the table, release the
// temporary slot, and hand the Arc back to the caller.
//
// The release on success transfers ownership: the temporary HANDLE_TABLE
// slot inserted on the engine side is no longer needed once we hold a
// strong PyO3-owned Arc. On mismatch we leave the slot intact so callers
// can diagnose without losing data.
fn arc_from_handle(handle: &BamlHandle, expected_kind: MediaKind) -> PyResult<Arc<MediaValue>> {
    let arc_value = HANDLE_TABLE
        .resolve(handle.key())
        .ok_or_else(|| PyRuntimeError::new_err("media handle is no longer valid"))?;
    let result = match &*arc_value {
        CffiHandleTableEntry::Adt(BexExternalAdt::Media(media_arc))
            if media_arc.kind == expected_kind =>
        {
            Ok(Arc::clone(media_arc))
        }
        CffiHandleTableEntry::Adt(BexExternalAdt::Media(media_arc)) => {
            Err(PyTypeError::new_err(format!(
                "media handle kind mismatch: expected {:?}, got {:?}",
                expected_kind, media_arc.kind
            )))
        }
        _ => Err(PyTypeError::new_err(
            "handle does not point to a media value",
        )),
    };
    if result.is_ok() {
        HANDLE_TABLE.release(handle.key());
    }
    result
}

fn handle_type_for(kind: MediaKind) -> BamlHandleType {
    match kind {
        MediaKind::Image => BamlHandleType::AdtMediaImage,
        MediaKind::Audio => BamlHandleType::AdtMediaAudio,
        MediaKind::Video => BamlHandleType::AdtMediaVideo,
        MediaKind::Pdf => BamlHandleType::AdtMediaPdf,
        MediaKind::Generic => BamlHandleType::AdtMediaGeneric,
    }
}

// Insert an `Arc<MediaValue>` into the global handle table and return a
// fresh `BamlHandle` pointing at it. Used by the inbound encoder so a
// Python-owned media value survives a round-trip into the engine.
fn arc_to_handle(arc: &Arc<MediaValue>) -> BamlHandle {
    let table_value = CffiHandleTableEntry::Adt(BexExternalAdt::Media(Arc::clone(arc)));
    let key = HANDLE_TABLE.insert(table_value);
    let handle_type = handle_type_for(arc.kind) as i32;
    BamlHandle::new(key, handle_type)
}

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
    ($name:ident, $kind:expr) => {
        #[pyclass]
        pub struct $name {
            pub(crate) inner: Arc<MediaValue>,
        }

        #[pymethods]
        impl $name {
            #[staticmethod]
            #[pyo3(signature = (url, mime_type=None))]
            fn from_url(url: String, mime_type: Option<String>) -> Self {
                Self {
                    inner: MediaValue::from_url($kind, &url, mime_type.as_deref()),
                }
            }

            #[staticmethod]
            #[pyo3(signature = (file, mime_type=None))]
            fn from_file(file: String, mime_type: Option<String>) -> Self {
                Self {
                    inner: MediaValue::from_file($kind, &file, mime_type.as_deref()),
                }
            }

            #[staticmethod]
            #[pyo3(signature = (base64, mime_type=None))]
            fn from_base64(base64: String, mime_type: Option<String>) -> Self {
                Self {
                    inner: MediaValue::from_base64($kind, &base64, mime_type.as_deref()),
                }
            }

            fn url(&self) -> Option<String> {
                self.inner.url()
            }

            fn file(&self) -> Option<String> {
                self.inner.file()
            }

            fn base64(&self) -> String {
                self.inner.base64()
            }

            fn mime_type(&self) -> Option<String> {
                self.inner.mime_type()
            }

            // Internal: resolve a BamlHandle from the handle table into a
            // fresh PyO3 instance owning the underlying Arc. Used by the
            // outbound decoder (`proto.py`'s `_decode_handle`).
            #[classmethod]
            fn _from_handle(
                _cls: &Bound<'_, pyo3::types::PyType>,
                handle: &BamlHandle,
            ) -> PyResult<Self> {
                let arc = arc_from_handle(handle, $kind)?;
                Ok(Self { inner: arc })
            }

            // Internal: borrow this value into the handle table and return
            // a fresh BamlHandle pointing at it. Used by the inbound encoder
            // (`proto.py`'s `_set_inbound_value`).
            fn _to_handle(&self) -> BamlHandle {
                arc_to_handle(&self.inner)
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
    };
}

define_media_pyclass!(BamlImage, MediaKind::Image);
define_media_pyclass!(BamlAudio, MediaKind::Audio);
define_media_pyclass!(BamlVideo, MediaKind::Video);
define_media_pyclass!(BamlPdf, MediaKind::Pdf);

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    use pyo3::types::PyModuleMethods;
    m.add_class::<BamlImage>()?;
    m.add_class::<BamlAudio>()?;
    m.add_class::<BamlVideo>()?;
    m.add_class::<BamlPdf>()?;
    Ok(())
}
