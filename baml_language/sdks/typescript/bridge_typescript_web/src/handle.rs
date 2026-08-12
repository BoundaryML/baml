//! Raw ordinary handle-table bindings.

use bridge_cffi::handle::{self as handle_core, HandleError, HandleParts};
use bridge_ctypes::baml_bridge::cffi::BamlHandleType;
use wasm_bindgen::prelude::*;

use crate::errors::{handle_error, unexpected_handle_type};

fn key_with_expected_type(
    operation: &'static str,
    parts: HandleParts,
    expected: BamlHandleType,
) -> Result<u64, JsError> {
    let expected = expected as i32;
    if parts.handle_type != expected {
        return Err(unexpected_handle_type(
            operation,
            expected,
            parts.handle_type,
        ));
    }
    Ok(parts.key)
}

#[wasm_bindgen(js_name = cloneHandle)]
pub fn clone_handle(key: u64) -> Result<u64, JsError> {
    handle_core::clone_handle(key).map_err(|error| handle_error("cloneHandle", &error))
}

#[wasm_bindgen(js_name = releaseHandle)]
pub fn release_handle(key: u64) -> bool {
    match handle_core::release_handle(key) {
        Ok(()) => true,
        Err(HandleError::InvalidHandle) => false,
        Err(_) => false,
    }
}

#[wasm_bindgen(js_name = _testHandleTableEntryCount)]
pub fn test_handle_table_entry_count() -> Result<u32, JsError> {
    u32::try_from(handle_core::live_handle_count())
        .map_err(|_| JsError::new("handle table entry count exceeds uint32"))
}

#[wasm_bindgen(js_name = seedFunctionRefHandle)]
pub fn seed_function_ref_handle(global_index: u32) -> Result<u64, JsError> {
    key_with_expected_type(
        "seedFunctionRefHandle",
        handle_core::seed_function_ref_handle(u64::from(global_index)),
        BamlHandleType::FunctionRef,
    )
}

#[wasm_bindgen(js_name = seedGenericMediaHandle)]
pub fn seed_generic_media_handle() -> Result<u64, JsError> {
    key_with_expected_type(
        "seedGenericMediaHandle",
        handle_core::seed_generic_media_handle(),
        BamlHandleType::AdtMediaGeneric,
    )
}
