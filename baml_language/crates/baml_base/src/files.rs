//! File management with Salsa 2022 API.
//!
//! Defines the core structures for accessing file contents and paths.
//!
//! ## Virtual file classification
//!
//! The compiler uses two categories of non-user files, identified by path prefix:
//!
//! | Category  | Prefix          | Example                              |
//! |-----------|-----------------|--------------------------------------|
//! | Built-in  | `<builtin>/`    | `<builtin>/baml/llm.baml`            |
//! | Generated | `<generated:`   | `<generated:stream/resume.baml>`     |
//!
//! The canonical prefix constants live in this file.
//! Other crates should classify files through the [`SourceFile::is_builtin`],
//! [`SourceFile::is_generated`], and [`SourceFile::is_virtual`] methods, and
//! only use [`SourceFile::builtin_path_prefix`] /
//! [`SourceFile::generated_path_prefix`] when they must construct or strip
//! virtual paths.

use std::path::PathBuf;

use crate::FileId;

// ── Canonical virtual-file path prefixes ──────────────────────────────────────
const BUILTIN_PATH_PREFIX: &str = "<builtin>/";
const GENERATED_PATH_PREFIX: &str = "<generated:";

/// A source file tracked by the Salsa database.
///
/// This is a salsa input, which means it is the primary way to provide source
/// text to the compiler. The struct itself is a lightweight handle; the actual
/// data lives in the Salsa database and is accessed through the generated
/// field accessors (`text`, `path`, `file_id`).
#[salsa::input]
pub struct SourceFile {
    /// The full source text of the file.
    #[returns(ref)]
    pub text: String,

    /// File path used for diagnostics and error reporting.
    ///
    /// For real files this is an absolute filesystem path.
    /// For virtual files it is one of the synthetic prefixed paths described
    /// in the module-level documentation.
    pub path: PathBuf,

    /// The [`FileId`] associated with this source file.
    ///
    /// Stored here so that lightweight [`crate::Span`] values can identify
    /// their origin file without carrying the full `SourceFile` handle.
    pub file_id: FileId,
}

impl SourceFile {
    // ── Classification predicates ────────────────────────────────────────────

    /// Returns `true` if this file is a compiler-provided built-in.
    ///
    /// Built-in files (path prefix `<builtin>/`) define internal standard-library
    /// types and functions. They are excluded from user-facing validation and
    /// diagnostic reporting.
    pub fn is_builtin(self, db: &dyn salsa::Database) -> bool {
        self.has_prefix(db, BUILTIN_PATH_PREFIX)
    }

    /// Returns `true` if this file was synthesized by a compiler pass.
    ///
    /// Generated files (path prefix `<generated:`) are internal implementation
    /// details — currently only PPIR stream expansions — and must not be
    /// surfaced to users as source locations.
    pub fn is_generated(self, db: &dyn salsa::Database) -> bool {
        self.has_prefix(db, GENERATED_PATH_PREFIX)
    }

    /// Returns `true` if this file is virtual, i.e. either built-in or generated.
    ///
    /// Virtual files are not user-authored and should be skipped by passes that
    /// operate only on user source (e.g. name validation, stream diagnostics).
    pub fn is_virtual(self, db: &dyn salsa::Database) -> bool {
        self.has_prefix(db, BUILTIN_PATH_PREFIX) || self.has_prefix(db, GENERATED_PATH_PREFIX)
    }

    // ── Prefix accessors ────────────────────────────────────────────────────

    /// Returns the path prefix string used for built-in files (`"<builtin>/"`).
    ///
    /// Use this only when you need to **construct** or **strip** a built-in
    /// path (e.g. in `baml_builtins` or `baml_compiler_hir::file_namespace`).
    /// For classification, prefer [`is_builtin`](Self::is_builtin).
    pub fn builtin_path_prefix() -> &'static str {
        BUILTIN_PATH_PREFIX
    }

    /// Returns the path prefix string used for generated files (`"<generated:"`).
    ///
    /// Use this only when you need to **construct** a generated path (e.g. in
    /// `baml_compiler_ppir::ppir_expansion_cst`).
    /// For classification, prefer [`is_generated`](Self::is_generated).
    pub fn generated_path_prefix() -> &'static str {
        GENERATED_PATH_PREFIX
    }

    // ── private ──────────────────────────────────────────────────────────────

    fn has_prefix(self, db: &dyn salsa::Database, prefix: &str) -> bool {
        self.path(db).to_string_lossy().starts_with(prefix)
    }
}
