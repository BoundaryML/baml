//! WASM `baml.time` namespace implementation.
//!
//! `Instant.now()` reads the wall clock directly via `web_time`, which maps to
//! the browser's `Date.now()` / `performance.now()` on wasm targets — the same
//! clock source `baml.sys.now_ms` uses in the VM. No JS callback is needed, so
//! `WasmTime` is a unit struct.

use std::sync::Arc;

use sys_ops::io::{self, CallId, SysOpContext, SysOpOutput, owned};
use sys_types::BexHeap;

/// WASM implementation of the `baml.time` namespace.
pub(crate) struct WasmTime;

impl io::IoClassTimeInstant for WasmTime {
    fn now(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::time::Instant> {
        // Wall-clock time as nanoseconds since the UNIX epoch. Per the
        // `Instant.now()` contract, an unavailable/pre-epoch clock panics.
        let nanos = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .expect("system clock is set before the UNIX epoch")
            .as_nanos();
        SysOpOutput::ok(owned::time::Instant {
            _nanoseconds: Arc::new(num_bigint::BigInt::from(nanos)),
        })
    }
}

impl io::IoNamespaceTime for WasmTime {}
