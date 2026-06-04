//! Native helper functions for Python BAML media wrappers.
//!
//! The public `BamlImage`/`BamlAudio`/`BamlVideo`/`BamlPdf` classes live in
//! `baml_core.media`. This module keeps only the low-level bridge operations
//! that need access to the shared CFFI handle table.

use bex_project::MediaKind;
use bridge_ctypes::baml_core::cffi::BamlHandleType;
use pyo3::{
    Bound, Py, PyErr, PyResult, Python,
    exceptions::{PyRuntimeError, PyValueError},
    prelude::{PyModule, pyfunction},
    wrap_pyfunction,
};
use pyo3_stub_gen::derive::gen_stub_pyfunction;

use crate::py_handle::BamlPyHandle;

fn handle_status_err(context: &str, status: bridge_cffi::BamlCffiStatus) -> PyErr {
    let reason = match status {
        bridge_cffi::BAML_HANDLE_INVALID_HANDLE => "invalid handle",
        bridge_cffi::BAML_HANDLE_TYPE_MISMATCH => "handle type mismatch",
        bridge_cffi::BAML_HANDLE_UNSUPPORTED_HANDLE_TYPE => "unsupported handle type",
        bridge_cffi::BAML_HANDLE_INTERNAL_ERROR => "internal handle error",
        _ => "unknown handle error",
    };
    PyRuntimeError::new_err(format!("{context}: {reason}"))
}

fn media_kind_from_handle_type(handle_type: u64) -> PyResult<MediaKind> {
    let handle_type = i32::try_from(handle_type).map_err(|_| {
        PyValueError::new_err(format!(
            "BAML media handle_type {handle_type} does not fit in int32"
        ))
    })?;
    match BamlHandleType::try_from(handle_type) {
        Ok(BamlHandleType::AdtMediaImage) => Ok(MediaKind::Image),
        Ok(BamlHandleType::AdtMediaAudio) => Ok(MediaKind::Audio),
        Ok(BamlHandleType::AdtMediaVideo) => Ok(MediaKind::Video),
        Ok(BamlHandleType::AdtMediaPdf) => Ok(MediaKind::Pdf),
        Ok(_) => Err(PyValueError::new_err(format!(
            "handle_type {handle_type} is not a typed BAML media handle"
        ))),
        Err(_) => Err(PyValueError::new_err(format!(
            "unsupported BAML media handle_type {handle_type}"
        ))),
    }
}

fn expected_handle_type_i32(expected_handle_type: u64) -> PyResult<i32> {
    let _ = media_kind_from_handle_type(expected_handle_type)?;
    i32::try_from(expected_handle_type).map_err(|_| {
        PyValueError::new_err(format!(
            "BAML media handle_type {expected_handle_type} does not fit in int32"
        ))
    })
}

fn media_value(
    pyhandle: &BamlPyHandle,
    expected_handle_type: u64,
    context: &str,
) -> PyResult<std::sync::Arc<bex_project::MediaValue>> {
    let expected_kind = media_kind_from_handle_type(expected_handle_type)?;
    let expected_handle_type = expected_handle_type_i32(expected_handle_type)?;
    bridge_cffi::media_value_impl(pyhandle.handle_key, expected_handle_type, expected_kind)
        .map_err(|status| handle_status_err(context, status))
}

#[gen_stub_pyfunction]
#[pyfunction]
pub fn _media_from_url(
    py: Python<'_>,
    media_handle_type: u64,
    url: String,
    mime_type: Option<String>,
) -> PyResult<Py<BamlPyHandle>> {
    let kind = media_kind_from_handle_type(media_handle_type)?;
    let (key, handle_type) = bridge_cffi::media_from_url_impl(kind, &url, mime_type.as_deref());
    Ok(Py::new(py, BamlPyHandle::new(key, handle_type as u64))?)
}

#[gen_stub_pyfunction]
#[pyfunction]
pub fn _media_from_file(
    py: Python<'_>,
    media_handle_type: u64,
    file: String,
    mime_type: Option<String>,
) -> PyResult<Py<BamlPyHandle>> {
    let kind = media_kind_from_handle_type(media_handle_type)?;
    let (key, handle_type) = bridge_cffi::media_from_file_impl(kind, &file, mime_type.as_deref());
    Ok(Py::new(py, BamlPyHandle::new(key, handle_type as u64))?)
}

#[gen_stub_pyfunction]
#[pyfunction]
pub fn _media_from_base64(
    py: Python<'_>,
    media_handle_type: u64,
    base64: String,
    mime_type: Option<String>,
) -> PyResult<Py<BamlPyHandle>> {
    let kind = media_kind_from_handle_type(media_handle_type)?;
    let (key, handle_type) =
        bridge_cffi::media_from_base64_impl(kind, &base64, mime_type.as_deref());
    Ok(Py::new(py, BamlPyHandle::new(key, handle_type as u64))?)
}

#[gen_stub_pyfunction]
#[pyfunction]
pub fn _media_url(pyhandle: &BamlPyHandle, expected_handle_type: u64) -> PyResult<Option<String>> {
    Ok(media_value(pyhandle, expected_handle_type, "_media_url")?.url())
}

#[gen_stub_pyfunction]
#[pyfunction]
pub fn _media_file(pyhandle: &BamlPyHandle, expected_handle_type: u64) -> PyResult<Option<String>> {
    Ok(media_value(pyhandle, expected_handle_type, "_media_file")?.file())
}

#[gen_stub_pyfunction]
#[pyfunction]
pub fn _media_base64(pyhandle: &BamlPyHandle, expected_handle_type: u64) -> PyResult<String> {
    Ok(media_value(pyhandle, expected_handle_type, "_media_base64")?.base64())
}

#[gen_stub_pyfunction]
#[pyfunction]
pub fn _media_mime_type(
    pyhandle: &BamlPyHandle,
    expected_handle_type: u64,
) -> PyResult<Option<String>> {
    Ok(media_value(pyhandle, expected_handle_type, "_media_mime_type")?.mime_type())
}

#[gen_stub_pyfunction]
#[pyfunction]
pub fn _media_validate(pyhandle: &BamlPyHandle, expected_handle_type: u64) -> PyResult<()> {
    media_value(pyhandle, expected_handle_type, "_media_validate")?;
    Ok(())
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    use pyo3::types::PyModuleMethods;
    m.add_function(wrap_pyfunction!(_media_from_url, m)?)?;
    m.add_function(wrap_pyfunction!(_media_from_file, m)?)?;
    m.add_function(wrap_pyfunction!(_media_from_base64, m)?)?;
    m.add_function(wrap_pyfunction!(_media_url, m)?)?;
    m.add_function(wrap_pyfunction!(_media_file, m)?)?;
    m.add_function(wrap_pyfunction!(_media_base64, m)?)?;
    m.add_function(wrap_pyfunction!(_media_mime_type, m)?)?;
    m.add_function(wrap_pyfunction!(_media_validate, m)?)?;
    Ok(())
}
