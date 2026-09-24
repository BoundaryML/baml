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

use bex_project::MediaKind;
use bridge_cffi::handle::{self as handle_core, HandleError, HandleParts};
use bridge_ctypes::baml_bridge::cffi::BamlHandleType;
use pyo3::{
    Bound, Py, PyAny, PyResult, Python,
    exceptions::{PyTypeError, PyValueError},
    prelude::{PyModule, pyclass, pymethods},
    types::PyAnyMethods,
};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::py_handle::{BamlPyHandle, handle_clone, status_to_pyerr};

type MediaConstructor = fn(MediaKind, &str, Option<&str>) -> Result<HandleParts, HandleError>;
type MediaAccessor<T> = fn(u64, i32) -> Result<T, HandleError>;

fn create_media(
    constructor: MediaConstructor,
    media_kind: MediaKind,
    value: String,
    mime_type: Option<String>,
    context: &str,
) -> PyResult<(u64, u64)> {
    // Keep each language's existing argument error and field name.
    for (field, input) in [
        (context, value.as_str()),
        ("mime_type", mime_type.as_deref().unwrap_or_default()),
    ] {
        if input.contains('\0') {
            return Err(PyValueError::new_err(format!("{field} cannot contain NUL")));
        }
    }
    let parts = constructor(media_kind, &value, mime_type.as_deref())
        .map_err(|error| status_to_pyerr(context, error.into()))?;
    Ok((parts.key, parts.handle_type as u64))
}

fn media_string<T>(
    key: u64,
    handle_type: u64,
    accessor: MediaAccessor<T>,
    context: &str,
) -> PyResult<T> {
    accessor(key, handle_type as i32).map_err(|error| status_to_pyerr(context, error.into()))
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
    ($name:ident, $media_kind:expr, $expected_ht:expr) => {
        // `module` sets the class's reported `__module__`. PyO3 defaults
        // it to `"builtins"`, but these types are imported from
        // `baml_bridge.baml_py` (where the extension `.so` lives), and the
        // typemap reverse-map seed in `baml_bridge/typemap.py` keys media
        // identity on `("baml_bridge.baml_py", "Baml{Image,…}")`. Declaring
        // the honest module makes `type(value).__module__` match that
        // seed, so `py_type_to_baml_type` resolves media values on the
        // encode path instead of returning `""` (35b "Bug B").
        #[gen_stub_pyclass]
        #[pyclass(module = "baml_bridge.baml_py")]
        pub struct $name {
            pub(crate) handle: Py<BamlPyHandle>,
        }

        #[gen_stub_pymethods]
        #[pymethods]
        impl $name {
            #[staticmethod]
            #[pyo3(signature = (url, mime_type=None))]
            fn from_url(py: Python<'_>, url: String, mime_type: Option<String>) -> PyResult<Self> {
                let (key, handle_type) = create_media(
                    handle_core::media_from_url,
                    $media_kind,
                    url,
                    mime_type,
                    "from_url",
                )?;
                Ok(Self {
                    handle: Py::new(py, BamlPyHandle::new(key, handle_type))?,
                })
            }

            #[staticmethod]
            #[pyo3(signature = (file, mime_type=None))]
            fn from_file(
                py: Python<'_>,
                file: String,
                mime_type: Option<String>,
            ) -> PyResult<Self> {
                let (key, handle_type) = create_media(
                    handle_core::media_from_file,
                    $media_kind,
                    file,
                    mime_type,
                    "from_file",
                )?;
                Ok(Self {
                    handle: Py::new(py, BamlPyHandle::new(key, handle_type))?,
                })
            }

            #[staticmethod]
            #[pyo3(signature = (base64, mime_type=None))]
            fn from_base64(
                py: Python<'_>,
                base64: String,
                mime_type: Option<String>,
            ) -> PyResult<Self> {
                let (key, handle_type) = create_media(
                    handle_core::media_from_base64,
                    $media_kind,
                    base64,
                    mime_type,
                    "from_base64",
                )?;
                Ok(Self {
                    handle: Py::new(py, BamlPyHandle::new(key, handle_type))?,
                })
            }

            fn url(&self, py: Python<'_>) -> PyResult<Option<String>> {
                self.access_optional_string(py, handle_core::media_url, "url")
            }

            fn file(&self, py: Python<'_>) -> PyResult<Option<String>> {
                self.access_optional_string(py, handle_core::media_file, "file")
            }

            fn base64(&self, py: Python<'_>) -> PyResult<String> {
                self.access_string(py, handle_core::media_base64, "base64")
            }

            fn mime_type(&self, py: Python<'_>) -> PyResult<Option<String>> {
                self.access_optional_string(py, handle_core::media_mime_type, "mime_type")
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
            fn _to_pyhandle(&self, py: Python<'_>) -> PyResult<Py<BamlPyHandle>> {
                let pyh = self.handle.borrow(py);
                let new_key = handle_clone(pyh.handle_key, "_to_pyhandle")?;
                Py::new(py, BamlPyHandle::new(new_key, pyh.handle_type))
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
            fn access_optional_string(
                &self,
                py: Python<'_>,
                accessor: MediaAccessor<Option<String>>,
                context: &str,
            ) -> PyResult<Option<String>> {
                let pyh = self.handle.borrow(py);
                media_string(pyh.handle_key, pyh.handle_type, accessor, context)
            }

            fn access_string(
                &self,
                py: Python<'_>,
                accessor: MediaAccessor<String>,
                context: &str,
            ) -> PyResult<String> {
                let pyh = self.handle.borrow(py);
                media_string(pyh.handle_key, pyh.handle_type, accessor, context)
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
