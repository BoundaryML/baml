//! Workspace and project management.
//!
//! Handles discovering BAML files in a project directory and managing the project structure.
//!
//! This crate provides:
//! - File discovery (`discover_baml_files`)
//! - Project root tracking (`Project` Salsa input)
//! - Source file utilities
//! - The base `Db` trait for project context
//!
//! ## Architecture Note
//!
//! `Project` is defined here (rather than in `baml_project`) because:
//! - Lower-level crates (`baml_compiler_hir`, `baml_compiler_tir`, `baml_compiler_mir`) need the `Project` type
//!   in their query signatures (e.g., `validate_hir(db, project)`)
//! - If `Project` were in `baml_project`, those crates would need to depend on
//!   `baml_project`, creating a circular dependency
//! - This follows the pattern: low-level types here, high-level operations in `baml_project`
//!
//! This is similar to how ty/ruff structures their codebase:
//! - `ruff_db` provides low-level types and the base `Db` trait
//! - `ty_project` provides high-level `ProjectDatabase` and operations
//! - The `Program` singleton (compiler settings) lives in the semantic crate

use std::path::PathBuf;

use baml_base::SourceFile;

mod discovery;
pub use discovery::discover_baml_files;

mod project_resolution;
pub use project_resolution::{
    BAML_SRC_DIR, BAML_TOML, find_baml_project_root, find_baml_project_root_from_ancestors,
    project_search_dir, project_source_root, resolve_project_search_start,
};

/// Database trait for workspace/project context.
///
/// Provides access to the project being compiled. Extended by downstream
/// crates (`baml_compiler_hir::Db`, `baml_compiler_tir::Db`, etc.).
#[salsa::db]
pub trait Db: salsa::Database {
    /// Returns the project being analyzed.
    fn project(&self) -> Project;

    /// Per-file throw-analysis facts seeded from a previous compile.
    ///
    /// When present, `throw_inference::file_throw_facts` returns the seeded
    /// facts for a file instead of re-walking its body — the bytecode
    /// cache's per-file reuse sets this for files whose content is
    /// unchanged (facts are a pure function of file content + name
    /// resolution, and the cache's dirty-set analysis re-walks any file
    /// whose resolution-relevant surroundings changed). Defaults to `None`:
    /// every other database compiles honestly.
    fn seeded_throw_facts(&self) -> Option<SeededThrowFacts> {
        None
    }

    /// The stdlib packages' resolved typed interfaces from a previous compile,
    /// keyed by package name.
    ///
    /// When present, `package_interface::package_interface` returns the seeded
    /// interface for a stdlib package instead of re-deriving it from source —
    /// removing the cold-typecheck floor a fresh process otherwise pays to
    /// re-normalize every stdlib signature/class/alias before it can typecheck
    /// user code. The stdlib is a compiler-build constant (no user file can
    /// contribute to a stdlib package), so the CLI caches this once per compiler
    /// build under `bex_cache::stdlib_interface_key` and seeds it back on every
    /// compile. Defaults to `None`: every other database compiles honestly.
    fn seeded_stdlib_interface(&self) -> Option<SeededStdlibInterface> {
        None
    }

    /// Per-function `callable_throws` values from a previous compile, keyed by
    /// (source path, item-tree `LocalItemId`).
    ///
    /// When present, `callable::callable_throws` returns the seeded `Ty` for a
    /// clean function instead of inferring its body — the bytecode cache sets
    /// this for functions the per-file reuse plan proved unchanged (both their
    /// own body and their transitive throw contributors are stable, per the
    /// throws-taint closure). Cutting `callable_throws` removes the last cold
    /// `infer_scope_types` pull a dirty file otherwise forces on its clean
    /// callees. Defaults to `None`: every other database infers honestly.
    fn seeded_callable_throws(&self) -> Option<SeededCallableThrows> {
        None
    }

    /// Source-less dependency packages mounted into this database as serialized
    /// `PackageInterface` blobs, keyed by the package name (the mount alias).
    ///
    /// When present (BEP-066 mounted-package linking), each entry makes its name a *dependency*
    /// of every user package (`package_dependencies`) whose `package_interface`
    /// is served straight from the blob — the mounted package has **no source
    /// files** (`package_items` is empty; that is the point). Cross-package
    /// resolution for a mounted name goes through the interface rows instead of
    /// raw items. Names colliding with the reserved package set (the stdlib
    /// packages, `user`, `root`, `env`) are ignored entirely — see
    /// `baml_compiler2_hir::package::mounted_package_names`. Defaults to `None`:
    /// every other database resolves dependencies from source only.
    fn mounted_packages(&self) -> Option<MountedPackages> {
        None
    }
}

/// Input: per-file `FunctionThrowFacts` from a previous compile, keyed by
/// the full source-file path string (`SourceFile::path` display form).
#[salsa::input]
pub struct SeededThrowFacts {
    #[returns(ref)]
    pub by_path:
        std::collections::BTreeMap<String, Vec<baml_type::throw_facts::FunctionThrowFacts>>,
}

/// Input: exact per-function `callable_throws` results from a previous compile,
/// keyed by source-file path string (`SourceFile::path` display form) then by
/// item-tree `LocalItemId::as_u32`.
///
/// Holds a typed `baml_type::Ty` — not opaque bytes like [`SeededStdlibInterface`]
/// — because `Ty` is a low-crate type this workspace crate can already name
/// (same as [`SeededThrowFacts`]). The `LocalItemId` key is a content-derived,
/// process-independent item-tree index, so a byte-identical file's functions map
/// to the same keys across compiles. `callable_throws` reads it through a
/// *tracked* dependency (present-from-construction, empty until seeded), so a
/// later seed on a reused database invalidates the memo.
#[salsa::input]
pub struct SeededCallableThrows {
    #[returns(ref)]
    pub by_path: std::collections::BTreeMap<String, std::collections::BTreeMap<u32, baml_type::Ty>>,
}

/// Input: the stdlib packages' resolved `PackageInterface`s from a previous
/// compile, keyed by package name; each value is `borsh(PackageInterface)`.
///
/// The value is opaque bytes rather than the typed interface because
/// `PackageInterface` lives in `baml_compiler2_hir_ty`, which depends on this
/// crate — naming it here would be a dependency cycle. `baml_compiler2_hir_ty`
/// deserializes the relevant package's bytes on a seed hit. Per-package (not
/// whole-map) bytes keep the short-circuit's deserialize cost to one package
/// per query call. This mirrors [`SeededThrowFacts`], which holds a type its
/// low crate can name; `PackageInterface` has no such low-crate home.
#[salsa::input]
pub struct SeededStdlibInterface {
    #[returns(ref)]
    pub by_package: std::collections::BTreeMap<String, Vec<u8>>,
}

/// Input: source-less dependency packages mounted as serialized
/// `PackageInterface` blobs, keyed by package name (the mount alias); each
/// value is `borsh(PackageInterface)`.
///
/// Opaque bytes for the same reason as [`SeededStdlibInterface`]:
/// `PackageInterface` lives in `baml_compiler2_tir`, which depends on this
/// crate. `baml_compiler2_tir` deserializes a package's bytes when its
/// `package_interface` is queried. A Salsa input, so mounting/unmounting a
/// package invalidates dependents for free (the B-694 delivery mechanism
/// generalized to any alias, per BEP-066 mounted-package linking).
#[salsa::input]
pub struct MountedPackages {
    #[returns(ref)]
    pub by_package: std::collections::BTreeMap<String, Vec<u8>>,

    /// Names whose blobs are compiler-built, image-immutable dependencies.
    ///
    /// This is deliberately metadata on the mounted-package transport rather
    /// than a second interface store: both ordinary runtime mounts and the
    /// precompiled stdlib use the same `PackageInterface` bytes, while the
    /// compiler can still keep immutable rows on its fact-free fast path.
    #[returns(ref)]
    pub immutable_precompiled: std::collections::BTreeSet<String>,
}

/// Input: the project root configuration
///
/// This tracks both the root path and the list of source files in the project.
/// By storing files as an input field, Salsa can properly track changes to the
/// file list (files added/removed) as well as changes to individual files.
#[salsa::input]
pub struct Project {
    pub root: PathBuf,

    /// The list of source files in this project.
    /// This should be updated whenever files are added or removed.
    #[returns(ref)]
    pub files: Vec<SourceFile>,
}

/// Input: compiler2-only extra source files.
///
/// Holds builtin stub files (`.baml`) for the compiler2 pipeline that must NOT
/// be visible to the v1 compiler. These are files like `containers.baml`,
/// `env.baml`, etc. that use compiler2-specific syntax (generic type parameters,
/// `$rust_type`, etc.) which the v1 parser cannot handle.
///
/// `baml_compiler2_hir` queries (`namespace_items`, `package_items`) use a
/// combined view: `project.files()` (user + v1 builtins) ∪ `compiler2_extra_files.files()`.
#[salsa::input]
pub struct Compiler2ExtraFiles {
    /// Compiler2-only source files (e.g., `baml_builtins2` stubs).
    #[returns(ref)]
    pub files: Vec<SourceFile>,
}
