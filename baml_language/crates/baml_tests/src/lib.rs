//! Test utilities and automatic tests for the BAML compiler.
//!
//! This crate provides:
//! - [`engine`]: Unified test infrastructure using `baml_test!` macro
//! - Automatic snapshot tests generated from the `projects/{tier}/` directories by `build.rs`

pub mod engine;

/// Compile BAML source and run the entry function, returning bytecode display + result.
///
/// # Variants
///
/// Simple (source only, entry defaults to `"main"`, opt defaults to `OptLevel::One`):
/// ```ignore
/// baml_test!("source")
/// ```
///
/// Struct-style with named fields (all fields except `baml` are optional):
/// ```ignore
/// baml_test! {
///     baml: "source",
///     entry: "func",
///     args: { "x" => val },
///     opt: OptLevel::Zero,
/// }
/// ```
#[macro_export]
macro_rules! baml_test {
    // Simple: source only
    ($source:expr) => {
        $crate::engine::run_test(
            $source,
            "main",
            $crate::engine::IndexMap::new(),
            $crate::engine::OptLevel::One,
        )
        .await
    };
    // baml only
    (baml: $source:expr $(,)?) => {
        $crate::engine::run_test(
            $source,
            "main",
            $crate::engine::IndexMap::new(),
            $crate::engine::OptLevel::One,
        )
        .await
    };
    // baml + entry
    (baml: $source:expr, entry: $entry:expr $(,)?) => {
        $crate::engine::run_test(
            $source,
            $entry,
            $crate::engine::IndexMap::new(),
            $crate::engine::OptLevel::One,
        )
        .await
    };
    // baml + args
    (baml: $source:expr, args: { $($k:literal => $v:expr),* $(,)? } $(,)?) => {{
        let mut __args = $crate::engine::IndexMap::new();
        $( __args.insert($k, $v); )*
        $crate::engine::run_test(
            $source,
            "main",
            __args,
            $crate::engine::OptLevel::One,
        )
        .await
    }};
    // baml + entry + args
    (baml: $source:expr, entry: $entry:expr, args: { $($k:literal => $v:expr),* $(,)? } $(,)?) => {{
        let mut __args = $crate::engine::IndexMap::new();
        $( __args.insert($k, $v); )*
        $crate::engine::run_test(
            $source,
            $entry,
            __args,
            $crate::engine::OptLevel::One,
        )
        .await
    }};
    // baml + opt
    (baml: $source:expr, opt: $opt:expr $(,)?) => {
        $crate::engine::run_test(
            $source,
            "main",
            $crate::engine::IndexMap::new(),
            $opt,
        )
        .await
    };
    // baml + entry + opt
    (baml: $source:expr, entry: $entry:expr, opt: $opt:expr $(,)?) => {
        $crate::engine::run_test(
            $source,
            $entry,
            $crate::engine::IndexMap::new(),
            $opt,
        )
        .await
    };
    // baml + args + opt
    (baml: $source:expr, args: { $($k:literal => $v:expr),* $(,)? }, opt: $opt:expr $(,)?) => {{
        let mut __args = $crate::engine::IndexMap::new();
        $( __args.insert($k, $v); )*
        $crate::engine::run_test(
            $source,
            "main",
            __args,
            $opt,
        )
        .await
    }};
    // baml + entry + args + opt
    (baml: $source:expr, entry: $entry:expr, args: { $($k:literal => $v:expr),* $(,)? }, opt: $opt:expr $(,)?) => {{
        let mut __args = $crate::engine::IndexMap::new();
        $( __args.insert($k, $v); )*
        $crate::engine::run_test(
            $source,
            $entry,
            __args,
            $opt,
        )
        .await
    }};
    // baml + show_auto_derive (include synthesized to_json / from_json in the
    // bytecode snapshot — off by default to keep snapshots stable).
    (baml: $source:expr, show_auto_derive: $sad:expr $(,)?) => {
        $crate::engine::run_test_with_options(
            $source,
            "main",
            $crate::engine::IndexMap::new(),
            $crate::engine::OptLevel::One,
            $sad,
        )
        .await
    };
    // baml + entry + show_auto_derive
    (baml: $source:expr, entry: $entry:expr, show_auto_derive: $sad:expr $(,)?) => {
        $crate::engine::run_test_with_options(
            $source,
            $entry,
            $crate::engine::IndexMap::new(),
            $crate::engine::OptLevel::One,
            $sad,
        )
        .await
    };
}

/// Like `baml_test!` but at `OptLevel::Two` (includes MIR constant folding).
///
/// Use this for testing optimization passes like catch switch dispatch and
/// constant folding that require `OptLevel::Two`.
#[macro_export]
macro_rules! baml_test_optimized {
    // Simple: source only
    ($source:expr) => {
        $crate::engine::run_test_mir_optimized(
            $source,
            "main",
            $crate::engine::IndexMap::new(),
        )
        .await
    };
    // baml only
    (baml: $source:expr $(,)?) => {
        $crate::engine::run_test_mir_optimized(
            $source,
            "main",
            $crate::engine::IndexMap::new(),
        )
        .await
    };
    // baml + entry
    (baml: $source:expr, entry: $entry:expr $(,)?) => {
        $crate::engine::run_test_mir_optimized(
            $source,
            $entry,
            $crate::engine::IndexMap::new(),
        )
        .await
    };
    // baml + args
    (baml: $source:expr, args: { $($k:literal => $v:expr),* $(,)? } $(,)?) => {{
        let mut __args = $crate::engine::IndexMap::new();
        $( __args.insert($k, $v); )*
        $crate::engine::run_test_mir_optimized(
            $source,
            "main",
            __args,
        )
        .await
    }};
    // baml + entry + args
    (baml: $source:expr, entry: $entry:expr, args: { $($k:literal => $v:expr),* $(,)? } $(,)?) => {{
        let mut __args = $crate::engine::IndexMap::new();
        $( __args.insert($k, $v); )*
        $crate::engine::run_test_mir_optimized(
            $source,
            $entry,
            __args,
        )
        .await
    }};
}

#[cfg(test)]
macro_rules! assert_compiler2_snapshot {
    ($snapshot_path:expr, $name:expr, $output:expr) => {
        insta::with_settings!({ snapshot_path => $snapshot_path, omit_expression => true }, {
            insta::assert_snapshot!($name, $output);
        });
    };
}

#[cfg(test)]
pub mod compiler2_hir;

#[cfg(test)]
pub mod compiler2_hir_ty;

#[cfg(test)]
pub mod compiler2_ppir;

#[cfg(test)]
pub mod compiler2_tir;

#[cfg(test)]
pub mod compiler2_mir;

#[cfg(test)]
pub mod compiler2_emit;

#[cfg(test)]
pub mod incremental;

#[cfg(test)]
pub mod type_spec;

#[cfg(test)]
pub mod string_literals;

#[cfg(test)]
pub mod utils;

// Include the tests generated by build.rs. It prefers src/ (not OUT_DIR) so
// file!() returns a stable path for insta snapshots, and falls back to OUT_DIR
// when the source tree is read-only, as it is under a per-crate build system;
// either way it reports the path it used through this env.
#[cfg(test)]
include!(env!("BAML_GENERATED_TESTS"));
