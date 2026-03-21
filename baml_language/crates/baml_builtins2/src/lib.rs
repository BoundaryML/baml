//! Builtin `.baml` stub files for the compiler2 pipeline.
//!
//! All sources live under `baml_std/` and are embedded at compile time via
//! `include_str!` — no filesystem reads at runtime, works on both native and WASM.
//!
//! # Layout: folder tree = package
//!
//! Everything lives under `baml_std/baml/` → package **baml**.
//! Sub-namespaces (env, llm, http, etc.) are expressed via the macro's namespace argument.
//!
//! Namespaces within the package are specified explicitly in the `builtin!` macro.
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
/// Path must follow `../baml_std/<package>/...` so the folder tree defines the package.
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
/// All builtins are in the `baml` package. Namespaces are explicit per entry.
pub const ALL: &[BuiltinFile] = &[
    // --- baml_std/baml/ ---
    builtin!(
        "baml",
        root,
        "containers.baml",
        "../baml_std/baml/containers.baml"
    ),
    builtin!("baml", root, "core.baml", "../baml_std/baml/core.baml"),
    builtin!(
        "baml",
        ["errors"],
        "errors.baml",
        "../baml_std/baml/errors.baml"
    ),
    builtin!("baml", root, "string.baml", "../baml_std/baml/string.baml"),
    builtin!("baml", ["env"], "env.baml", "../baml_std/baml/env.baml"),
    builtin!("baml", ["http"], "http.baml", "../baml_std/baml/http.baml"),
    builtin!("baml", ["math"], "math.baml", "../baml_std/baml/math.baml"),
    builtin!("baml", ["sys"], "sys.baml", "../baml_std/baml/sys.baml"),
    builtin!("baml", ["fs"], "fs.baml", "../baml_std/baml/fs.baml"),
    builtin!("baml", ["net"], "net.baml", "../baml_std/baml/net.baml"),
    builtin!(
        "baml",
        ["media"],
        "media.baml",
        "../baml_std/baml/media.baml"
    ),
    builtin!(
        "baml",
        ["unstable"],
        "unstable.baml",
        "../baml_std/baml/unstable.baml"
    ),
    builtin!(
        "baml",
        ["llm"],
        "llm_types.baml",
        "../baml_std/baml/llm_types.baml"
    ),
    builtin!("baml", ["llm"], "llm.baml", "../baml_std/baml/llm.baml"),
    // // --- baml_std/env/ ---
    // builtin!("env", root, "env.baml", "../baml_std/env/env.baml"),
];
