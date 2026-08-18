//! Test utilities and automatic tests for the BAML compiler.
//!
//! This crate provides:
//! - [`engine`]: Unified test infrastructure using `baml_test!` macro
//! - Automatic snapshot tests generated from the `projects/{tier}/` directories by `build.rs`

pub mod engine;

/// This crate's source dir: the root every corpus path (`baml_src/`,
/// `projects/`, fixtures) and file-snapshot dir resolves against.
///
/// The compile-time `CARGO_MANIFEST_DIR` is baked into the test binaries,
/// and for the CI nix unit graph that is a build sandbox
/// (`/build/baml_tests-<ver>/`) that does not exist when a prebuilt binary
/// runs - every corpus read and file snapshot would miss. `BAML_TESTS_DIR`
/// binds the dir when set; unset (every local and cargo-arm run), behavior
/// is byte-identical. Same pattern as `BAML_SURFACE_SNAPSHOT_DIR` and
/// `BAML_PARAM_SCHEMA_GOLDEN`, which exist for the same relocated-build
/// reason. The L2 snapshot lane sets it; nothing else does.
pub fn manifest_dir() -> std::path::PathBuf {
    std::env::var_os("BAML_TESTS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

/// A file-snapshot assertion whose snapshot dir resolves through
/// [`manifest_dir`]. `$subdir` is the module's snapshot dir relative to the
/// crate root - the same directory insta would derive by default, so unset
/// (every local and cargo-arm run) this is byte-identical to a bare
/// `insta::assert_snapshot!`; in relocated runs the env override points it
/// at the real checkout instead of the baked build sandbox. Inline
/// snapshots need none of this and stay bare.
#[cfg(test)]
macro_rules! file_snapshot {
    ($subdir:literal, $($arg:tt)*) => {{
        let mut settings = insta::Settings::clone_current();
        settings.set_snapshot_path($crate::manifest_dir().join($subdir));
        settings.bind(|| {
            insta::assert_snapshot!($($arg)*);
        })
    }};
}
#[cfg(test)]
pub(crate) use file_snapshot;

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

// Include the tests generated by build.rs.
// Written to src/ (not OUT_DIR) so file!() returns a stable path for insta snapshots.
#[cfg(test)]
include!("generated_tests.rs");
