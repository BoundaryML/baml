//! Private bindings injected into the generated Python facades.
use bridge_ctypes::baml_bridge::cffi::BamlHandleType;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::gen_stub_pyfunction;

use crate::py_handle::BamlPyHandle;

fn host_error(error: bridge_cffi::BridgeError) -> PyErr {
    match error {
        bridge_cffi::BridgeError::InvocationProtocol(message) => {
            pyo3::exceptions::PyTypeError::new_err(message)
        }
        other => crate::errors::bridge_error_to_sdk_panic(other),
    }
}

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

#[pyo3_stub_gen::derive::gen_stub_pyclass]
#[pyclass]
pub struct _HostExecution {
    execution: std::sync::Mutex<Option<bex_project::HostInvocation>>,
}

#[pyo3_stub_gen::derive::gen_stub_pymethods]
#[pymethods]
impl _HostExecution {
    fn finish(&self, outcome: &str, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let outcome = match outcome {
            "ok" => btel_types::InvocationOutcome::Ok,
            "error" => btel_types::InvocationOutcome::Errored,
            "cancelled" => btel_types::InvocationOutcome::Cancelled,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "invalid host outcome",
                ));
            }
        };
        if let Some(execution) = self
            .execution
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let captured = value
                .filter(|_| execution.wants_value(outcome))
                .map(crate::host_capture::capture);
            execution.finish_with_value(outcome, captured.as_ref());
        }
        Ok(())
    }
}

#[gen_stub_pyfunction]
#[pyfunction]
#[pyo3(signature = (options))]
pub fn _validate_host_options(options: Option<&BamlPyHandle>) -> PyResult<(bool, bool, bool)> {
    bridge_cffi::host_instrumentation::options(options.map_or(0, |handle| handle.handle_key))
        .map(|options| {
            (
                options.inputs == Some(true),
                options.output == Some(true),
                options.error == Some(true),
            )
        })
        .map_err(host_error)
}

#[gen_stub_pyfunction]
#[pyfunction]
#[pyo3(signature = (definition, inherited, options, caller, inputs))]
pub fn _begin_host_invocation(
    definition: (String, String, String, u32, u32, String),
    inherited: Option<&BamlPyHandle>,
    options: Option<&BamlPyHandle>,
    caller: (String, u32),
    inputs: Option<&Bound<'_, PyAny>>,
) -> PyResult<(_HostExecution, BamlPyHandle, Vec<u8>)> {
    let (module, qualified_name, source_file, definition_line, wrapper_line, display_name) =
        definition;
    let options =
        bridge_cffi::host_instrumentation::options(options.map_or(0, |handle| handle.handle_key))
            .map_err(host_error)?;
    let inputs = inputs
        .filter(|_| options.inputs == Some(true))
        .map(crate::host_capture::capture);
    let (execution, key) = bridge_cffi::host_instrumentation::begin(
        &bex_project::HostDefinition {
            language: "python".into(),
            module,
            qualified_name,
            source_file,
            definition_line,
            wrapper_line,
            display_name,
        },
        inherited.map_or(0, |handle| handle.handle_key),
        &options,
        &bex_project::HostCallSite {
            source_file: caller.0,
            line: caller.1,
        },
        inputs.as_ref(),
    )
    .map_err(host_error)?;
    let state_handle = BamlPyHandle::new(key, BamlHandleType::InvocationState as u64);
    use prost::Message;
    let cancel = bridge_ctypes::external_to_outbound(
        &execution.inherited_state().cancellation_projection(),
        &bridge_ctypes::CffiHandleTableOptions::for_wire(),
    )
    .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))?
    .encode_to_vec();
    Ok((
        _HostExecution {
            execution: std::sync::Mutex::new(Some(execution)),
        },
        state_handle,
        cancel,
    ))
}
