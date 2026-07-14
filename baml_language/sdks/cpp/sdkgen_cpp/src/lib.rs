//! C++ SDK emitter. Stub — the API surface mirrors
//! `sdkgen_python_pydantic2` so the `sdk_test_harness_setup::cpp` target and
//! the future `baml-cli generate` wiring are written against the final
//! signature, but codegen panics until the real emitter lands. The target
//! output shape is specified in the bridge-cpp codegen spec (single
//! `baml_sdk.hpp` + bindings/_typemap/_inlinedbaml sources).

use std::{collections::HashMap, path::PathBuf};

use baml_codegen_types::SymbolPool;
pub use baml_codegen_types::{NamingConvention, OutputType};

/// Build the C++ SDK output tree for `pool` with precompiled BAML bytecode
/// as the runtime payload. Returned paths are relative to the `baml_sdk/`
/// output root.
pub fn to_source_code_with_bytecode(
    _pool: &SymbolPool,
    _baml_bytecode: &[u8],
    _naming_convention: NamingConvention,
) -> HashMap<PathBuf, String> {
    unimplemented!(
        "sdkgen_cpp::to_source_code_with_bytecode is a stub — the C++ emitter has not been implemented yet"
    );
}
