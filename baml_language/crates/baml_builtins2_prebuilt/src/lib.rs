//! Precompiled BAML stdlib artifacts.
//!
//! `build.rs` compiles the embedded, frozen stdlib once and serializes two
//! artifacts that a normal `baml` invocation loads instead of recompiling:
//!
//! - the emit [`EmitState`] prefix (`stdlib_prefix.bin`) — reused via
//!   `generate_project_bytecode_with_prefix` to skip re-lowering/re-emitting the
//!   stdlib's bytecode.
//! - the per-file pre-lowered HIR (`stdlib_hir.bin`) — installed via
//!   [`install_precompiled_hir`] so `file_semantic_index` skips lex/parse/lower
//!   for builtin files.
//!
//! Compiler types are reached through `baml_db`'s re-exports (workspace policy).

use std::collections::HashMap;

use baml_db::{
    baml_compiler2_emit::EmitState,
    baml_compiler2_hir::precompiled,
    baml_compiler2_tir::{
        interfaces::ImplementsRegistry, package_interface::PackageInterface, precompiled_tir,
    },
};

/// Borsh-serialized stdlib [`EmitState`] prefix, produced at build time.
pub static STDLIB_PREFIX_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/stdlib_prefix.bin"));

/// Borsh-serialized per-file stdlib HIR prefix, produced at build time.
pub static STDLIB_HIR_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/stdlib_hir.bin"));

/// Borsh-serialized per-package stdlib TIR `PackageInterface` prefix.
pub static STDLIB_TIR_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/stdlib_tir.bin"));

/// Borsh-serialized per-package stdlib `ImplementsRegistry` prefix.
pub static STDLIB_IMPLEMENTS_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/stdlib_implements.bin"));

/// Deserialize the precompiled stdlib [`EmitState`] prefix.
///
/// Pass the result to `generate_project_bytecode_with_prefix` (or
/// `ProjectDatabase::get_bytecode_with_prefix`) to skip recompiling the stdlib.
pub fn stdlib_prefix() -> EmitState {
    borsh::from_slice(STDLIB_PREFIX_BYTES).expect("decode precompiled stdlib EmitState")
}

/// Deserialize and install all precompiled stdlib caches, so a normal compile
/// skips re-deriving the frozen stdlib:
///
/// - HIR: `file_semantic_index` skips lex/parse/lower + builder for builtin files.
/// - TIR: `package_interface` and `package_implements_registry` return each
///   stdlib package's resolved signature graph + interface-impl rules directly
///   (the dominant cost of the first user-file type-check).
///
/// Idempotent (first call wins); call once at startup. No-op safe: if not called,
/// the compiler derives the stdlib from source as before.
pub fn install_precompiled_stdlib() {
    let hir: HashMap<String, precompiled::PrecompiledFile> =
        borsh::from_slice(STDLIB_HIR_BYTES).expect("decode precompiled stdlib HIR");
    precompiled::set_precompiled_builtins(hir);

    let tir: HashMap<String, PackageInterface> =
        borsh::from_slice(STDLIB_TIR_BYTES).expect("decode precompiled stdlib TIR");
    precompiled_tir::set_precompiled_package_interfaces(tir);

    let implements: HashMap<String, ImplementsRegistry> =
        borsh::from_slice(STDLIB_IMPLEMENTS_BYTES).expect("decode precompiled stdlib implements");
    precompiled_tir::set_precompiled_implements_registries(implements);
}

#[cfg(test)]
mod tests {
    #[test]
    fn artifacts_load_and_lookup() {
        // EmitState prefix round-trips.
        let _ = super::stdlib_prefix();
        // HIR + TIR caches install; a known builtin path and the baml package resolve.
        super::install_precompiled_stdlib();
        assert!(
            super::precompiled::precompiled_builtin("<builtin>/baml/string.baml").is_some(),
            "string.baml should be in the precompiled HIR cache"
        );
        assert!(
            super::precompiled_tir::precompiled_package_interface("baml").is_some(),
            "baml package should be in the precompiled TIR cache"
        );
    }
}
