//! Minimal CFFI surface compiled into the first browser bridge.
//!
//! This is intentionally a runtime-failing scaffold: it proves that the web
//! SDK crosses into a WASM build containing `bridge_cffi`, while leaving the
//! browser `SysOps` implementation and real call execution for the next step.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static HAS_BYTECODE: AtomicBool = AtomicBool::new(false);
static NEXT_CALL_ID: AtomicU64 = AtomicU64::new(1);

pub const WASM_RUNTIME_UNAVAILABLE: &str =
    "bridge_cffi reached in WASM; browser runtime dispatch is not implemented yet";

pub fn stage_runtime_bytecode(bytecode: &[u8]) -> Result<(), &'static str> {
    if bytecode.is_empty() {
        return Err("bridge_cffi received empty BAML bytecode");
    }
    HAS_BYTECODE.store(true, Ordering::Release);
    Ok(())
}

pub fn call_function_in_wasm(
    _function_name: &str,
    _encoded_args: &[u8],
) -> Result<Vec<u8>, &'static str> {
    if !HAS_BYTECODE.load(Ordering::Acquire) {
        return Err("bridge_cffi WASM runtime has not received BAML bytecode");
    }
    Err(WASM_RUNTIME_UNAVAILABLE)
}

pub fn new_function_call_id() -> u64 {
    NEXT_CALL_ID.fetch_add(1, Ordering::Relaxed)
}
