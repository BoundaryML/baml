//! Host adapters copy bounded observations before native recording.
use btel_snapshot::host::HostValue;
use pyo3::{prelude::*, types::PyModule};

pub fn capture(value: &Bound<'_, PyAny>) -> HostValue {
    PyModule::import(value.py(), "baml_bridge._host_capture")
        .and_then(|module| module.getattr("capture"))
        .and_then(|capture| capture.call1((value,)))
        .and_then(|wire| wire.extract::<String>())
        .map_or(HostValue::Unavailable, |wire| {
            bridge_cffi::host_capture::decode(&wire)
        })
}
