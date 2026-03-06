//! Builtin `.baml` stub files for the compiler2 pipeline.
//!
//! All sources live under `baml_std/` and are embedded at compile time via
//! `include_str!` — no filesystem reads at runtime, works on both native and WASM.
//!
//! # Layout: folder tree = package
//!
//! The first directory under `baml_std/` is the **package** name:
//!
//! - `baml_std/baml/...` → package **baml** (containers, string, env, http, math, sys, media)
//! - `baml_std/env/...` → package **env**
//!
//! So adding a new std package = add `baml_std/<pkg>/` and register files with that package.
//! Namespaces within a package are still specified explicitly in the macro (hardcoded for now).
//!
//! # Virtual path
//!
//! Builtin virtual path is `<builtin>/<package>/<namespace...>/<filename>`. The HIR derives
//! package and namespace from path segments (see `baml_compiler2_hir::file_package`).

/// A builtin `.baml` file: package, namespace, filename, and embedded contents.
pub struct BuiltinFile {
    /// Package name (e.g. `"baml"`, `"env"`).
    pub package: &'static str,
    /// Sub-namespace within the package (e.g. `&[]` for root, `&["env"]` for `baml.env`).
    pub namespace: &'static [&'static str],
    /// Filename only (e.g. `"containers.baml"`, `"env.baml"`).
    pub filename: &'static str,
    /// File contents embedded at compile time via `include_str!`.
    pub contents: &'static str,
}

impl BuiltinFile {
    /// Build the virtual path for this builtin file.
    pub fn virtual_path(&self) -> String {
        if self.namespace.is_empty() {
            format!("<builtin>/{}/{}", self.package, self.filename)
        } else {
            format!(
                "<builtin>/{}/{}/{}",
                self.package,
                self.namespace.join("/"),
                self.filename
            )
        }
    }
}

/// Package name for the main std package (baml types and namespaces).
pub const PACKAGE_BAML: &str = "baml";

/// Single macro form: package (from `baml_std/$pkg/...`), namespace (root or `[ns, ...]`), filename, path.
/// Path must follow `../../../baml_std/<package>/...` so the folder tree defines the package.
macro_rules! builtin {
    ($pkg:literal, root, $filename:literal, $path:literal) => {
        BuiltinFile {
            package: $pkg,
            namespace: &[],
            filename: $filename,
            contents: include_str!($path),
        }
    };
    ($pkg:literal, [$($ns:literal),+], $filename:literal, $path:literal) => {
        BuiltinFile {
            package: $pkg,
            namespace: &[$($ns),+],
            filename: $filename,
            contents: include_str!($path),
        }
    };
}

/// All builtin `.baml` files, in registration order.
///
/// Package = first directory under `baml_std/` (baml, env, …). Namespaces are explicit.
/// Register baml before env so `env` can call `baml.env.get` / `baml.sys.panic`.
pub const ALL: &[BuiltinFile] = &[
    // --- baml_std/baml/ ---
    builtin!(
        "baml",
        root,
        "containers.baml",
        "../../../baml_std/baml/containers.baml"
    ),
    builtin!(
        "baml",
        root,
        "string.baml",
        "../../../baml_std/baml/string.baml"
    ),
    builtin!(
        "baml",
        ["env"],
        "env.baml",
        "../../../baml_std/baml/env.baml"
    ),
    builtin!(
        "baml",
        ["http"],
        "http.baml",
        "../../../baml_std/baml/http.baml"
    ),
    builtin!(
        "baml",
        ["math"],
        "math.baml",
        "../../../baml_std/baml/math.baml"
    ),
    builtin!(
        "baml",
        ["sys"],
        "sys.baml",
        "../../../baml_std/baml/sys.baml"
    ),
    builtin!(
        "baml",
        ["media"],
        "media.baml",
        "../../../baml_std/baml/media.baml"
    ),
    // --- baml_std/env/ ---
    builtin!("env", root, "env.baml", "../../../baml_std/env/env.baml"),
];
