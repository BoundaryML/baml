//! Centralized diagnostic collection for BAML projects.
//!
//! This module provides the `check()` and `check_file()` methods that collect
//! all diagnostics from a BAML project using the unified `Diagnostic` type.
//!
//! ## Example
//!
//! ```ignore
//! let result = db.check();
//! for diag in &result.diagnostics {
//!     println!("{}", diag.message);
//! }
//! ```

use std::collections::HashMap;

use baml_compiler_diagnostics::Diagnostic;
use baml_db::{FileId, SourceFile};
use baml_lsp2_actions::check_file as lsp2_check_file;
use baml_workspace::Project;

use crate::ProjectDatabase;

/// Result of checking a project, containing diagnostics and metadata for rendering.
#[derive(Debug)]
pub struct CheckResult {
    /// The collected diagnostics.
    pub diagnostics: Vec<Diagnostic>,
    /// Maps `FileId` to source text (for Ariadne rendering).
    pub sources: HashMap<FileId, String>,
    /// Maps `FileId` to file path (for URL generation).
    pub file_paths: HashMap<FileId, std::path::PathBuf>,
}

/// Collect all diagnostics from a project using the compiler2 pipeline.
///
/// This replaces the legacy `collect_diagnostics` that used `baml_compiler_hir` /
/// `baml_compiler_tir`. All diagnostics now come from the compiler2 pipeline via
/// `baml_lsp2_actions::check_file`.
///
/// The `project` and `source_files` parameters are accepted for API compatibility
/// with existing callers but are not used — the compiler2 pipeline derives the
/// file set internally from [`baml_compiler2_hir::compiler2_all_files`].
pub fn collect_diagnostics(
    db: &ProjectDatabase,
    _project: Project,
    _source_files: &[SourceFile],
) -> Vec<Diagnostic> {
    collect_compiler2_diagnostics(db)
}

/// Collect all diagnostics from the **compiler2** pipeline (parse + HIR2 + TIR2).
///
/// Files checked are [`baml_compiler2_hir::compiler2_all_files`]: user project
/// sources plus compiler2 stdlib stubs under `<builtin>/baml/...` (packages
/// `baml`, `env`, etc.).
///
/// Diagnostics are sorted by (`file_id`, primary span start, message) for
/// stable snapshot output.
pub fn collect_compiler2_diagnostics(db: &ProjectDatabase) -> Vec<Diagnostic> {
    let source_files = baml_compiler2_hir::compiler2_all_files(db);
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    for file in &source_files {
        diagnostics.extend(lsp2_check_file(db, *file));
    }

    // Collect package-level diagnostics (name conflicts and namespace shadows).
    // Discover all unique packages from file metadata.
    let mut seen_packages = std::collections::HashSet::new();
    for file in &source_files {
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, *file);
        seen_packages.insert(pkg_info.package.clone());
    }
    for pkg_name in seen_packages {
        let pkg_id = baml_compiler2_hir::package::PackageId::new(db, pkg_name);
        let items = baml_compiler2_hir::package::package_items(db, pkg_id);
        for conflict in items.conflicts() {
            diagnostics.push(conflict.to_diagnostic(db));
        }
        for shadow in items.shadows() {
            diagnostics.push(shadow.to_diagnostic(db));
        }
    }
    diagnostics.sort_by(|a, b| {
        let a_span = a.primary_span();
        let b_span = b.primary_span();
        match (a_span, b_span) {
            (Some(sa), Some(sb)) => {
                let file_cmp = sa.file_id.as_u32().cmp(&sb.file_id.as_u32());
                if file_cmp != std::cmp::Ordering::Equal {
                    return file_cmp;
                }
                let start_cmp = sa.range.start().cmp(&sb.range.start());
                if start_cmp != std::cmp::Ordering::Equal {
                    return start_cmp;
                }
                a.message.cmp(&b.message)
            }
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.message.cmp(&b.message),
        }
    });
    diagnostics
}

impl ProjectDatabase {
    /// Check the entire project and return all diagnostics.
    ///
    /// This is the centralized entry point for diagnostic collection, replacing
    /// the duplicated logic in the LSP server and test infrastructure.
    ///
    /// Returns a `CheckResult` containing diagnostics and metadata for rendering.
    pub fn check(&self) -> CheckResult {
        let Some(_project) = self.get_project() else {
            return CheckResult {
                diagnostics: Vec::new(),
                sources: HashMap::new(),
                file_paths: HashMap::new(),
            };
        };

        let source_files: Vec<SourceFile> = self.get_source_files();
        let mut sources: HashMap<FileId, String> = HashMap::new();
        let mut file_paths: HashMap<FileId, std::path::PathBuf> = HashMap::new();

        // Build all maps from user files
        for source_file in &source_files {
            let file_id = source_file.file_id(self);
            let text = source_file.text(self).clone();
            let path = source_file.path(self);

            sources.insert(file_id, text);
            file_paths.insert(file_id, path);
        }

        // Also register compiler2 builtin files for diagnostics rendering
        let all_c2_files = baml_compiler2_hir::compiler2_all_files(self);
        for file in &all_c2_files {
            let file_id = file.file_id(self);
            if let std::collections::hash_map::Entry::Vacant(e) = sources.entry(file_id) {
                e.insert(file.text(self).clone());
                file_paths.insert(file_id, file.path(self));
            }
        }

        let diagnostics = collect_compiler2_diagnostics(self);

        CheckResult {
            diagnostics,
            sources,
            file_paths,
        }
    }

    /// Legacy check method for backwards compatibility.
    /// Returns (diagnostics, sources) tuple.
    pub fn check_legacy(&self) -> (Vec<Diagnostic>, HashMap<FileId, String>) {
        let result = self.check();
        (result.diagnostics, result.sources)
    }

    /// Check a single file and return diagnostics for that file only.
    ///
    /// Note: This still requires the full project context for type checking.
    pub fn check_file(&self, file: SourceFile) -> Vec<Diagnostic> {
        lsp2_check_file(self, file)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    #[ignore = "compiler2: llm_types.baml builtin causes unreachable arm errors from catch expressions"]
    fn test_check_empty_project() {
        let mut db = ProjectDatabase::new();
        db.set_project_root(Path::new("/tmp"));

        let result = db.check();
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    #[ignore = "compiler2: llm_types.baml builtin causes unreachable arm errors from catch expressions"]
    fn test_check_valid_file() {
        let mut db = ProjectDatabase::new();
        db.set_project_root(Path::new("/tmp"));
        db.add_or_update_file(Path::new("/tmp/test.baml"), "class Foo {\n  name string\n}");

        let result = db.check();
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn test_check_parse_error() {
        let mut db = ProjectDatabase::new();
        db.set_project_root(Path::new("/tmp"));
        db.add_or_update_file(Path::new("/tmp/test.baml"), "class Foo {");

        let result = db.check();
        assert!(!result.diagnostics.is_empty());
    }
}
