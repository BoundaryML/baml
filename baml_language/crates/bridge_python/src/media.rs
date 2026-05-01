//! PyO3 types for BAML media (`baml.media.{Image,Video,Audio,Pdf}`).
//!
//! Per 15b: each Python media class is a re-export of a Rust PyO3 type
//! holding `Arc<MediaValue>`. Static and instance methods dispatch
//! natively here instead of round-tripping through the BAML engine.
//!
//! These classes interact with the global `HANDLE_TABLE` directly —
//! they do *not* go through `BamlHandle`. The proto encoder/decoder on
//! the Python side reads the raw u64 key produced by
//! `_insert_into_handle_table` and writes it (alongside the
//! statically-known `_handle_type`) into the proto, and on the way back
//! resolves the key via `_take_from_handle_table`. `BamlHandle` is
//! scheduled for removal in a later phase.
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

fn handle_type_for(kind: MediaKind) -> BamlHandleType {
    match kind {
        MediaKind::Image => BamlHandleType::AdtMediaImage,
        MediaKind::Audio => BamlHandleType::AdtMediaAudio,
        MediaKind::Video => BamlHandleType::AdtMediaVideo,
        MediaKind::Pdf => BamlHandleType::AdtMediaPdf,
        MediaKind::Generic => BamlHandleType::AdtMediaGeneric,
    }
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

            // Internal: borrow this value into the global handle table and
            // return the raw u64 key. Used by the inbound encoder
            // (`proto.py`'s `_set_inbound_value`). The handle-table slot
            // is owned by the engine for the duration of the call — once
            // `convert_external_to_vm_value` allocates a `RustData` on
            // the VM heap (which holds its own `Arc::clone`), the slot
            // can be released.
            //
            // Bypasses `BamlHandle` entirely: the proto's `handle_type`
            // field is populated separately via `_handle_type` (a
            // statically-known per-class constant).
            fn _insert_into_handle_table(&self) -> u64 {
                let entry =
                    CffiHandleTableEntry::Adt(BexExternalAdt::Media(Arc::clone(&self.inner)));
                HANDLE_TABLE.insert(entry)
            }

            // Internal: resolve a raw key from the handle table into a
            // fresh PyO3 instance owning the underlying Arc. Used by the
            // outbound decoder (`proto.py`'s `_decode_handle`).
            //
            // On success the slot is released — ownership transfers to
            // the returned PyO3 instance. On kind mismatch the slot is
            // left intact so callers can diagnose without losing data.
            #[classmethod]
            fn _take_from_handle_table(
                _cls: &Bound<'_, pyo3::types::PyType>,
                key: u64,
            ) -> PyResult<Self> {
                let arc_value = HANDLE_TABLE
                    .resolve(key)
                    .ok_or_else(|| PyRuntimeError::new_err("media handle is no longer valid"))?;
                let arc = match &*arc_value {
                    CffiHandleTableEntry::Adt(BexExternalAdt::Media(media_arc))
                        if media_arc.kind == $kind =>
                    {
                        Arc::clone(media_arc)
                    }
                    CffiHandleTableEntry::Adt(BexExternalAdt::Media(media_arc)) => {
                        return Err(PyTypeError::new_err(format!(
                            "media handle kind mismatch: expected {:?}, got {:?}",
                            $kind, media_arc.kind
                        )));
                    }
                    _ => {
                        return Err(PyTypeError::new_err(
                            "handle does not point to a media value",
                        ));
                    }
                };
                HANDLE_TABLE.release(key);
                Ok(Self { inner: arc })
            }

            // Internal: the proto `BamlHandleType` tag for this class,
            // as i32 (matching the proto enum's encoding). Statically
            // known per kind; exposed as a classmethod so the Python
            // proto encoder can populate `handle.handle_type` without
            // instantiating a `BamlHandle`.
            #[classmethod]
            fn _handle_type(_cls: &Bound<'_, pyo3::types::PyType>) -> i32 {
                handle_type_for($kind) as i32
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
