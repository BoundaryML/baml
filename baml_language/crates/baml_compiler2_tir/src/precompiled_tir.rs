//! Precompiled per-package `PackageInterface` cache for the frozen stdlib.
//!
//! Resolving a package's `PackageInterface` (all class fields, enum variants,
//! type aliases, function signatures, throw sets) is the dominant cost of the
//! first `baml check`: a trivial user file lazily pulls in the whole stdlib type
//! graph through `package_interface(<stdlib pkg>)`. The stdlib is frozen per
//! compiler version, so that interface is deterministic.
//!
//! `PackageInterface` is pure data (`Ty` + names; no `'db` lifetime, no salsa
//! handles), so it serializes with borsh and needs *no* reconstruction on load
//! -- unlike the HIR caches, the cached value is returned directly. This module
//! lets a consumer embed it (built once by `baml_builtins2_prebuilt`) and
//! install it, so `package_interface` short-circuits for stdlib packages.
//!
//! The cache is optional: when unset (tests, LSP), `package_interface` resolves
//! from source as before, so behavior is unchanged.

use std::{collections::HashMap, sync::OnceLock};

use crate::{interfaces::ImplementsRegistry, package_interface::PackageInterface};

static PRECOMPILED: OnceLock<HashMap<String, PackageInterface>> = OnceLock::new();
static PRECOMPILED_IMPLEMENTS: OnceLock<HashMap<String, ImplementsRegistry>> = OnceLock::new();

/// Install the precompiled stdlib `PackageInterface` cache (first writer wins).
/// Keyed by package name (e.g. `baml`, `log`, `reflect`, `testing`, `assert`).
pub fn set_precompiled_package_interfaces(map: HashMap<String, PackageInterface>) {
    let _ = PRECOMPILED.set(map);
}

/// The precompiled interface for a stdlib package by name, if installed.
pub fn precompiled_package_interface(name: &str) -> Option<&'static PackageInterface> {
    PRECOMPILED.get()?.get(name)
}

/// Install the precompiled stdlib `ImplementsRegistry` cache (first writer wins).
/// Keyed by package name. Resolving interface-implementation rules over the
/// stdlib is a second cold cost pulled by inference; this skips it.
pub fn set_precompiled_implements_registries(map: HashMap<String, ImplementsRegistry>) {
    let _ = PRECOMPILED_IMPLEMENTS.set(map);
}

/// The precompiled implements registry for a stdlib package by name, if installed.
pub fn precompiled_implements_registry(name: &str) -> Option<&'static ImplementsRegistry> {
    PRECOMPILED_IMPLEMENTS.get()?.get(name)
}
