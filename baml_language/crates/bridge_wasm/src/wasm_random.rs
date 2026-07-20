//! WASM `baml.random` namespace implementation.
//!
//! `SystemRandom` draws fresh entropy from the host CSPRNG. On wasm that is the
//! Web Crypto API (`crypto.getRandomValues`), reached through `getrandom`'s
//! `wasm_js` backend — no JS callback is needed, so `WasmRandom` is a unit
//! struct (mirroring `WasmTime`).

use std::sync::Arc;

use sys_ops::io::{self, CallId, SysOpContext, SysOpOutput, VmPanic};
use sys_types::BexHeap;

/// WASM implementation of the `baml.random` namespace.
pub(crate) struct WasmRandom;

impl io::IoClassRandomSystemRandom for WasmRandom {
    fn random(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        bytes: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        let Ok(n) = usize::try_from(bytes) else {
            return SysOpOutput::err(VmPanic::UserPanic {
                message: format!("Rng.random: byte count must be non-negative, got {bytes}"),
            });
        };
        // Allocate fallibly so an unsatisfiable request is a catchable
        // `AllocFailure` panic, not a wasm-instance trap.
        let mut buf = Vec::new();
        if buf.try_reserve(n).is_err() {
            return SysOpOutput::err(VmPanic::AllocFailure {
                message: format!("Rng.random: allocation of {n} bytes failed"),
            });
        }
        buf.resize(n, 0u8);
        match getrandom_03::fill(&mut buf) {
            Ok(()) => SysOpOutput::ok(buf),
            Err(e) => SysOpOutput::err(VmPanic::HostUnavailable {
                resource: "randomness".to_string(),
                message: format!("SystemRandom.random: system entropy unavailable: {e}"),
            }),
        }
    }

    fn random_int(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        let mut buf = [0u8; 8];
        match getrandom_03::fill(&mut buf) {
            // Arithmetic shift right by one maps the uniform 64-bit draw onto
            // the BAML i63 range `[INT_MIN, INT_MAX]`.
            Ok(()) => SysOpOutput::ok(i64::from_le_bytes(buf) >> 1),
            Err(e) => SysOpOutput::err(VmPanic::HostUnavailable {
                resource: "randomness".to_string(),
                message: format!("SystemRandom.random_int: system entropy unavailable: {e}"),
            }),
        }
    }
}

impl io::IoNamespaceRandom for WasmRandom {}
