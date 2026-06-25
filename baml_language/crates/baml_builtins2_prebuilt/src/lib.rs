//! Precompiled BAML stdlib emit artifact.
//!
//! `build.rs` runs the compiler's core emit passes (1-4) over the embedded
//! stdlib once and serializes the resulting [`EmitState`] prefix to
//! `$OUT_DIR/stdlib_prefix.bin`. This crate embeds those bytes and exposes a
//! deserialize helper so a normal compile can resume from the stdlib prefix
//! (via `generate_project_bytecode_with_prefix`) instead of recompiling the
//! ~676-function stdlib from source on every invocation.

// Compiler types are reached through baml_db's re-export (workspace policy).
use baml_db::baml_compiler2_emit::EmitState;

/// Borsh-serialized stdlib [`EmitState`] prefix, produced at build time.
pub static STDLIB_PREFIX_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/stdlib_prefix.bin"));

/// Deserialize the precompiled stdlib [`EmitState`] prefix.
///
/// Pass the result to `generate_project_bytecode_with_prefix` (or
/// `ProjectDatabase::get_bytecode_with_prefix`) to skip recompiling the stdlib.
pub fn stdlib_prefix() -> EmitState {
    borsh::from_slice(STDLIB_PREFIX_BYTES).expect("decode precompiled stdlib EmitState")
}
