//! `BamlPyHandle` — Python wrapper for a `HANDLE_TABLE` row.
//!
//! Holds a `(handle_key, handle_type)` pair. The actual
//! `CffiHandleTableEntry` lives in `bridge_ctypes::HANDLE_TABLE`; the
//! Python object is just a token. This matches what Go/Rust client
//! bindings will look like — they can only see `u64` handles across the
//! CFFI boundary, so Python uses the same model for uniformity.
//!
//! Lifecycle:
//!  - Construct (decode side): `BamlPyHandle::new(key, ht)`. The wire
//!    encoder is responsible for having inserted under `key`. The Python
//!    object now owns the row; nobody else may drain it.
//!  - `__copy__` / `__deepcopy__`: take one more OWNERSHIP via
//!    `baml_handle_clone(key, out_key)` and wrap the returned key. For an
//!    identity-free row (media, function refs) that is a fresh key sharing
//!    the row's `Arc`; for an engine-heap handle it is the SAME key with its
//!    refcount bumped (one key per heap object, so host-side key equality is
//!    object identity). Either way each `BamlPyHandle` owes exactly one
//!    release — two objects may legitimately share a key.
//!  - Drop: `baml_handle_release(key)`, exactly once. Without it every
//!    handle leaks one ownership; a second release on a shared key steals a
//!    live co-owner's ownership (the table cannot tell owners apart).
//!
//! A handle to a host value (`HOST_VALUE_CALLABLE` / `HOST_VALUE_OPAQUE`)
//! names a key of the host-value registry instead: it holds one reference to
//! that key, taken when it is constructed or copied and given back on drop,
//! so the key stays registered for as long as Python can send it again.
//!
//! There is no public `handle_type()` method. The wire transmits
//! `BamlHandle.handle_type`, the Python object stores it on construction,
//! and Rust-internal callers (media class validation) read the field directly.

use bridge_cffi::{BamlCffiStatus, handle_cffi};
use bridge_ctypes::{TableKey, WireHandle};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyfunction, gen_stub_pymethods};

pub(crate) fn status_to_pyerr(context: &str, status: BamlCffiStatus) -> PyErr {
    let detail = status.description();
    pyo3::exceptions::PyRuntimeError::new_err(format!("{context}: {detail}"))
}

pub(crate) fn handle_clone(key: u64, context: &str) -> PyResult<u64> {
    handle_cffi::clone_handle(key).map_err(|error| status_to_pyerr(context, error.into()))
}

pub(crate) fn release_wire_handle(key: u64, context: &str) -> PyResult<()> {
    handle_cffi::release_handle(key).map_err(|error| status_to_pyerr(context, error.into()))
}

#[gen_stub_pyclass]
#[pyclass]
pub struct BamlPyHandle {
    pub(crate) handle_key: u64,
    /// `BamlHandleType` as u64 — same width as `handle_key` for uniformity
    /// across the CFFI surface. Set at construction from the wire field
    /// (decode path) or from the entry's intrinsic kind (encode/seed
    /// paths). Read by inbound encode (`_clone_key_for_wire`) and by media
    /// `_from_pyhandle` validation.
    pub(crate) handle_type: u64,
}

#[gen_stub_pymethods]
#[pymethods]
impl BamlPyHandle {
    /// Adopt the engine reference `handle_key` transfers, or take a reference
    /// to the host value `handle_key` names, which must still be registered.
    #[new]
    fn py_new(handle_key: u64, handle_type: u64) -> PyResult<Self> {
        let handle = Self {
            handle_key,
            handle_type,
        };
        if let Ok(WireHandle::Host(key, _)) = handle.wire_handle() {
            crate::host_value::send_again(key)?;
        }
        Ok(handle)
    }

    fn __copy__(&self) -> PyResult<Self> {
        Ok(Self {
            handle_key: wire_key(self.wire_reference()?),
            handle_type: self.handle_type,
        })
    }

    fn __deepcopy__(&self, _memo: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.__copy__()
    }

    /// Clone this handle for inbound wire ownership: the key carries one
    /// reference of its own, which the receiver takes over.
    fn _clone_key_for_wire(&self) -> PyResult<(u64, u64)> {
        Ok((wire_key(self.wire_reference()?), self.handle_type))
    }

    fn _key_for_invocation(&self) -> PyResult<u64> {
        if self.handle_type
            != bridge_ctypes::baml_bridge::cffi::BamlHandleType::InvocationState as u64
        {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "expected an invocation-state handle",
            ));
        }
        Ok(self.handle_key)
    }

    /// Borrow this handle key as a call target without transferring ownership.
    fn _key_for_call(&self) -> PyResult<u64> {
        use bridge_ctypes::baml_bridge::cffi::BamlHandleType;

        if self.handle_type != BamlHandleType::FunctionRef as u64 {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "handle does not reference a BAML callable",
            ));
        }
        Ok(self.handle_key)
    }
}

impl BamlPyHandle {
    /// Adopt the engine reference `handle_key`, which the caller holds.
    pub fn new(handle_key: u64, handle_type: u64) -> Self {
        let handle = Self {
            handle_key,
            handle_type,
        };
        debug_assert!(
            !matches!(handle.wire_handle(), Ok(WireHandle::Host(..))),
            "a handle to a host value takes its reference through `py_new`"
        );
        handle
    }

    /// One more reference to what this handle names, for a wire or another
    /// handle to own: a clone of the engine reference, or one more reference
    /// to the host value's key.
    pub(crate) fn wire_reference(&self) -> PyResult<WireHandle> {
        match self.wire_handle()? {
            WireHandle::Host(key, kind) => {
                crate::host_value::send_again(key)?;
                Ok(WireHandle::Host(key, kind))
            }
            WireHandle::Engine(_) => {
                let cloned = handle_clone(self.handle_key, "BamlPyHandle wire reference")?;
                TableKey::new(cloned)
                    .map(WireHandle::Engine)
                    .ok_or_else(|| {
                        pyo3::exceptions::PyRuntimeError::new_err("a cloned handle key is 0")
                    })
            }
        }
    }

    /// Which reference this handle is: the host's reference to an engine
    /// object, or the engine's reference to a host object.
    pub(crate) fn wire_handle(&self) -> PyResult<WireHandle> {
        let handle_type = i32::try_from(self.handle_type).map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err("handle type does not fit the wire")
        })?;
        WireHandle::from_parts(self.handle_key, handle_type)
            .map_err(|error| pyo3::exceptions::PyTypeError::new_err(error.to_string()))
    }
}

/// The key a wire handle carries, in whichever table it names.
fn wire_key(handle: WireHandle) -> u64 {
    match handle {
        WireHandle::Engine(key) => key.get(),
        WireHandle::Host(key, _) => key.get(),
    }
}

impl Drop for BamlPyHandle {
    fn drop(&mut self) {
        // Key 0 names nothing.
        match self.wire_handle() {
            Ok(WireHandle::Engine(key)) => {
                let _ = handle_cffi::release_handle(key.get());
            }
            Ok(WireHandle::Host(key, _)) => crate::host_value::release(key, 1),
            Err(_) => {}
        }
    }
}

/// Test-only: seed an `Adt(Media(image))` entry through the shared CFFI API.
#[gen_stub_pyfunction]
#[pyfunction]
pub fn _seed_media_handle() -> PyResult<(u64, u64)> {
    let parts = handle_cffi::seed_media_handle();
    Ok((parts.key, parts.handle_type as u64))
}

/// Test-only: seed an engine-heap (`BexHeapHandle`) entry through the shared
/// CFFI API — the identity-bearing, deduplicating arm. Two seeds of one
/// `slab_key` share a key.
#[gen_stub_pyfunction]
#[pyfunction]
pub fn _seed_heap_handle(slab_key: u64) -> PyResult<(u64, u64)> {
    let parts = handle_cffi::seed_heap_handle(slab_key);
    Ok((parts.key, parts.handle_type as u64))
}

/// Release a handle cloned for wire ownership when encoding aborts before the
/// engine can consume it.
#[gen_stub_pyfunction]
#[pyfunction]
pub fn _release_wire_handle(key: u64) -> PyResult<()> {
    release_wire_handle(key, "BamlPyHandle wire-encode rollback")
}

/// Test-only: return the number of live ordinary HANDLE_TABLE rows (a
/// refcounted engine-heap row counts once however many owners it has).
#[gen_stub_pyfunction]
#[pyfunction]
pub fn _live_handle_count() -> usize {
    bridge_cffi::handle_cffi::live_handle_count()
}

/// Test-only: the outstanding ownership count of a live key — the releases it
/// still owes — or `None` for a dead/unknown key. Lets an audit see an
/// exactly-once imbalance on a shared engine-heap key, which row counts hide.
#[gen_stub_pyfunction]
#[pyfunction]
pub fn _handle_refcount(key: u64) -> Option<u64> {
    bridge_cffi::handle_cffi::handle_refcount(key)
}
