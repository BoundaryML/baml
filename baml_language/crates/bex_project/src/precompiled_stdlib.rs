//! Compiler-built stdlib surface used only by transient runtime compilation.
//!
//! Cargo keys `OUT_DIR` and reruns this crate's build script with the exact
//! compiler dependency graph, so the embedded bytes cannot outlive the build
//! that produced their `Program`/`PackageInterface` layouts. The small schema
//! key below catches accidental producer/consumer format drift inside a build.

use std::collections::BTreeMap;

use bex_vm_types::Program;

const ARTIFACT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/stdlib_prefix.borsh"));

pub(crate) struct PrecompiledStdlib {
    pub interfaces: BTreeMap<String, Vec<u8>>,
    pub program: Program,
}

pub(crate) fn load() -> Result<PrecompiledStdlib, String> {
    let (key, interfaces, program): (String, BTreeMap<String, Vec<u8>>, Program) =
        borsh::from_slice(ARTIFACT)
            .map_err(|error| format!("decode compiler-built stdlib artifact: {error}"))?;
    let expected_key = crate::precompiled_stdlib_config::artifact_key();
    if key != expected_key {
        return Err(format!(
            "compiler-built stdlib artifact key mismatch: expected `{expected_key}`, got `{key}`"
        ));
    }
    Ok(PrecompiledStdlib {
        interfaces,
        program,
    })
}
