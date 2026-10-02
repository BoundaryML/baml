//! Private bindings injected into the generated Python facades.
use bridge_ctypes::baml_bridge::cffi::BamlHandleType;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::gen_stub_pyfunction;

use crate::py_handle::BamlPyHandle;

#[gen_stub_pyfunction]
#[pyfunction]
pub fn _trace_selection(
    handle: &BamlPyHandle,
    call_id: u64,
) -> PyResult<(Vec<u8>, Option<BamlPyHandle>)> {
    let (wire, reservation) =
        bridge_cffi::control_projection::trace_selection(call_id, handle.handle_key)
            .map_err(crate::errors::bridge_error_to_sdk_panic)?;
    Ok((
        wire,
        reservation.map(|key| BamlPyHandle::new(key, BamlHandleType::TraceReservation as u64)),
    ))
}

#[gen_stub_pyfunction]
#[pyfunction]
pub fn _invocation_context(handle: Option<&BamlPyHandle>) -> PyResult<Vec<u8>> {
    bridge_cffi::control_projection::invocation_context(
        handle.map_or(0, |handle| handle.handle_key),
    )
    .map_err(crate::errors::bridge_error_to_sdk_panic)
}
