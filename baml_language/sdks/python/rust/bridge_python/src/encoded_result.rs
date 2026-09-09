//! Python owns this result until decoding adopts its provisional handles.
//! PyO3 future conversion/cancellation can drop it without losing the receipt.

use crate::py_handle::{BamlPyHandle, HandleTransferState, TRANSFER_ADOPTED};
use bex_project::Bex;
use bridge_ctypes::baml_bridge::cffi::BamlHandleType;
use bridge_ctypes::{EncodedTransfer, PendingDelivery, TransferSession};
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
use std::sync::{Arc, atomic::Ordering};

#[gen_stub_pyclass]
#[pyclass]
pub struct BamlEncodedResult {
    pending: Option<PendingDelivery<'static, Vec<u8>>>,
    session: TransferSession<'static>,
    state: Option<Arc<HandleTransferState>>,
    handles: Vec<Py<BamlPyHandle>>,
    claims: Vec<u64>,
}

impl BamlEncodedResult {
    pub(crate) fn new(
        encoded: EncodedTransfer<'static, Vec<u8>>,
        session: TransferSession<'static>,
        runtime: Option<Arc<dyn Bex>>,
    ) -> PyResult<Self> {
        Self::with_invocation_session(encoded, session.clone(), runtime, session)
    }

    /// Error reporting can arrive after issuer shutdown. Its receipt must
    /// remain adoptable, while retained references still use the issuer's
    /// closed admission session rather than reopening invocation authority.
    pub(crate) fn with_invocation_session(
        encoded: EncodedTransfer<'static, Vec<u8>>,
        session: TransferSession<'static>,
        runtime: Option<Arc<dyn Bex>>,
        invocation_session: TransferSession<'static>,
    ) -> PyResult<Self> {
        let pending = session
            .stage(encoded)
            .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))?;
        Ok(Self {
            pending: Some(pending),
            session: session.clone(),
            state: Some(Arc::new(HandleTransferState::new(
                runtime,
                invocation_session,
            ))),
            handles: vec![],
            claims: vec![],
        })
    }

    fn discard_inner(&mut self) {
        let Some(state) = self.state.take() else {
            return;
        };
        state.discard();
        let pending = self.pending.take();
        self.handles.clear();
        self.claims.clear();
        drop(pending);
    }
}

// Receipt cleanup is a native safepoint: no engine permit or registry lock is
// held. Even a failed adoption/discard must deliver queued host releases.
struct DrainHostReleases;
impl Drop for DrainHostReleases {
    fn drop(&mut self) {
        bex_project::host_release_dispatch::drain();
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl BamlEncodedResult {
    #[getter]
    fn payload<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.pending
            .as_ref()
            .map(|pending| PyBytes::new(py, pending.payload()))
            .ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("BAML result was already consumed")
            })
    }

    fn _wrap_handle(&mut self, py: Python<'_>, key: u64, kind: u64) -> PyResult<Py<BamlPyHandle>> {
        let state = self.state.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("BAML result was already consumed")
        })?;
        // Reserve before construction so no allocation failure leaves a
        // provisional handle outside this aggregate's strong root set.
        self.handles.reserve(1);
        self.claims.reserve(1);
        let handle = Py::new(py, BamlPyHandle::provisional(key, kind, state.clone()))?;
        self.handles.push(handle.clone_ref(py));
        if kind != BamlHandleType::HostValueCallable as u64
            && kind != BamlHandleType::HostValueOpaque as u64
        {
            self.claims.push(key);
        }
        Ok(handle)
    }

    fn _adopt(&mut self) -> PyResult<()> {
        let _drain = DrainHostReleases;
        let pending = self.pending.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("BAML result was already consumed")
        })?;
        self.session
            .adopt(pending.receipt(), &self.claims)
            .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))?;
        if let Some(state) = self.state.take() {
            state.status.store(TRANSFER_ADOPTED, Ordering::Release);
        }
        self.pending.take();
        self.handles.clear();
        self.claims.clear();
        Ok(())
    }

    fn _discard(&mut self) {
        let _drain = DrainHostReleases;
        self.discard_inner();
    }
}

impl Drop for BamlEncodedResult {
    fn drop(&mut self) {
        let _drain = DrainHostReleases;
        // A native destructor must not unwind through Python's deallocator.
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.discard_inner())).is_err()
        {
            log::error!("resource cleanup panicked while discarding a Python BAML result");
        }
    }
}
