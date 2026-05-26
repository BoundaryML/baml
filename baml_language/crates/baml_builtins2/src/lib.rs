//! Builtin `.baml` stub files for the compiler2 pipeline.
//!
//! All sources live under `baml_std/` and are embedded at compile time via
//! `include_str!` — no filesystem reads at runtime, works on both native and WASM.
//!
//! # Layout: folder tree = package + ns_* subfolders = namespace
//!
//! Everything lives under `baml_std/baml/` → package **baml**.
//! Sub-namespaces (env, llm, http, etc.) are expressed via `ns_*` subdirectories
//! on disk, and namespace is derived from path segments at runtime.
//!
//! # Virtual path
//!
//! Builtin virtual path is `<builtin>/<package>/<relative_path>`. The HIR derives
//! package and namespace from path segments (see `baml_compiler2_hir::file_package`).

/// A builtin `.baml` file: package, path within package, and embedded contents.
pub struct BuiltinFile {
    /// Package name (e.g. `"baml"`).
    pub package: &'static str,
    /// Relative path within the package directory (e.g. `"containers.baml"`,
    /// `"ns_env/env.baml"`). Namespace is derived from `ns_*` segments.
    pub relative_path: &'static str,
    /// File contents embedded at compile time via `include_str!`.
    pub contents: &'static str,
}

impl BuiltinFile {
    /// Build the virtual path for this builtin file.
    /// e.g. `"<builtin>/baml/ns_env/env.baml"`.
    pub fn virtual_path(&self) -> String {
        format!("<builtin>/{}/{}", self.package, self.relative_path)
    }

    /// Derive the namespace path from `ns_*` segments in `relative_path`.
    /// e.g. `"ns_env/env.baml"` → `["env"]`, `"containers.baml"` → `[]`.
    pub fn namespace_path(&self) -> Vec<&str> {
        let segments: Vec<&str> = self.relative_path.split('/').collect();
        if segments.len() <= 1 {
            return vec![];
        }
        // Exclude the filename (last segment), filter ns_* from intermediates
        segments[..segments.len() - 1]
            .iter()
            .filter_map(|s| s.strip_prefix("ns_"))
            .collect()
    }
}

/// Package name for the main std package (baml types and namespaces).
pub const PACKAGE_BAML: &str = "baml";
/// Package name for the testing package.
pub const PACKAGE_TESTING: &str = "testing";
/// Package name for the assert package.
pub const PACKAGE_ASSERT: &str = "assert";

/// Absolute path to the `baml_std/` source tree, captured at compile time via
/// `CARGO_MANIFEST_DIR`. Used by `baml_builtins2_codegen` to produce clickable
/// file paths in build-script diagnostic messages (stderr only, never in
/// generated code or committed artifacts).
pub const BAML_STD_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/baml_std");

/// YAML documentation for BAML keywords, embedded at compile time.
pub const BAML_KEYWORDS_YAML: &str = include_str!("../keyword_docs/baml_keywords.yaml");

/// YAML crosswalk documentation for TypeScript/JS keywords, embedded at compile time.
pub const TS_KEYWORDS_YAML: &str = include_str!("../keyword_docs/ts_keywords.yaml");

/// Builtin registration macro: package, relative virtual path, filesystem include path.
macro_rules! builtin {
    ($pkg:literal, $fs_path:literal) => {
        BuiltinFile {
            package: $pkg,
            relative_path: $fs_path,
            contents: include_str!(concat!("../baml_std/", $pkg, "/", $fs_path)),
        }
    };
}

/// All builtin `.baml` files, in registration order. Namespaces derived from
/// `ns_*` folder segments in `relative_path`.
pub const ALL: &[BuiltinFile] = &[
    // --- Root namespace (no ns_* prefix) ---
    builtin!("baml", "containers.baml"),
    builtin!("baml", "core.baml"),
    builtin!("baml", "int.baml"),
    builtin!("baml", "float.baml"),
    builtin!("baml", "bool.baml"),
    builtin!("baml", "null.baml"),
    builtin!("baml", "string.baml"),
    builtin!("baml", "uint8array.baml"),
    builtin!("baml", "type_class.baml"),
    // --- Namespaced (ns_* folders) ---
    builtin!("baml", "ns_errors/errors.baml"),
    builtin!("baml", "ns_errors/stack_trace.baml"),
    builtin!("baml", "ns_panics/panics.baml"),
    builtin!("baml", "ns_env/env.baml"),
    builtin!("baml", "ns_io/io.baml"),
    builtin!("baml", "ns_http/http.baml"),
    builtin!("baml", "ns_events/events.baml"),
    builtin!("baml", "ns_math/math.baml"),
    builtin!("baml", "ns_sys/sys.baml"),
    builtin!("baml", "ns_fs/fs.baml"),
    builtin!("baml", "ns_glob/glob.baml"),
    builtin!("baml", "ns_net/net.baml"),
    builtin!("baml", "ns_media/media.baml"),
    builtin!("baml", "ns_json/json.baml"),
    builtin!("baml", "ns_unstable/unstable.baml"),
    builtin!("baml", "ns_llm/llm_types.baml"),
    builtin!("baml", "ns_llm/llm.baml"),
    builtin!("baml", "ns_stream/stream.baml"),
    builtin!("baml", "ns_future/future.baml"),
    // --- reflect package (standalone, accessible as `reflect.type_of(...)`) ---
    builtin!("reflect", "reflect.baml"),
    // --- testing package ---
    builtin!("testing", "registry.baml"),
    builtin!("testing", "types.baml"),
    // --- assert package ---
    builtin!("assert", "assert.baml"),
    // --- log package ---
    builtin!("log", "log.baml"),
];

mod adt;
mod media;
pub use adt::*;
pub use media::{MediaContent, MediaValue};
