#![allow(clippy::print_stderr)]

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};
use baml_db::baml_compiler_diagnostics::{Diagnostic, Severity, render};
use baml_project::ProjectDatabase;
use baml_workspace::discover_baml_files;

/// Canonicalize `from` and load all discovered `.baml` files into a fresh
/// [`ProjectDatabase`]. Returns the database, the canonical project root, and
/// the list of loaded files.
///
/// Callers decide how to surface an empty `files` result (e.g. `run` bails,
/// `test` returns `NoTestsRun`, `generate` returns `Other`).
pub(crate) fn load_project_from(from: &Path) -> Result<(ProjectDatabase, PathBuf, Vec<PathBuf>)> {
    let canonical = std::fs::canonicalize(from)
        .with_context(|| format!("Could not resolve path: {}", from.display()))?;
    let mut db = ProjectDatabase::new();
    db.set_project_root(&canonical);
    let baml_files = discover_baml_files(&canonical);
    for file_path in &baml_files {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read {}", file_path.display()))?;
        db.add_or_update_file(file_path, &content);
    }
    Ok((db, canonical, baml_files))
}

/// Collect diagnostics on `db`. If any are errors, render them to stderr
/// and bail with `bail_context`. On success, return any warnings so the
/// caller can decide how (or whether) to surface them.
///
/// Shared between `baml run` and `baml pack` — both need the same
/// error-rendering contract; only how they handle *warnings* differs
/// (run threads through `--verbose`, pack drops them silently).
pub(crate) fn check_project_diagnostics(
    db: &ProjectDatabase,
    bail_context: &str,
) -> Result<Vec<Diagnostic>> {
    let project = db
        .get_project()
        .ok_or_else(|| anyhow!("No project context"))?;
    let source_files = db.get_source_files();
    let diagnostics = baml_project::collect_diagnostics(db, project, &source_files);

    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    let warnings: Vec<Diagnostic> = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .cloned()
        .collect();

    if errors.is_empty() {
        return Ok(warnings);
    }

    let mut sources = HashMap::new();
    let mut file_paths = HashMap::new();
    for sf in &source_files {
        let file_id = sf.file_id(db);
        sources.insert(file_id, sf.text(db).to_string());
        file_paths.insert(file_id, sf.path(db));
    }
    let rendered = render::render_diagnostics(
        &errors.iter().copied().cloned().collect::<Vec<_>>(),
        &sources,
        &file_paths,
        &render::RenderConfig::cli(),
    );
    eprintln!("{rendered}");
    anyhow::bail!("{bail_context}");
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// A clean project with no diagnostics returns an empty warning list.
    #[test]
    fn test_check_project_diagnostics_clean_project_ok() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("ok.baml");
        std::fs::write(&src, "function main() -> int { 1 }\n").unwrap();

        let mut db = ProjectDatabase::new();
        db.set_project_root(tmp.path());
        db.add_or_update_file(&src, "function main() -> int { 1 }\n");

        let warnings = check_project_diagnostics(&db, "should not bail").unwrap();
        assert!(
            warnings.is_empty(),
            "clean project should produce no warnings"
        );
    }

    /// A project with a syntax error bails with the caller-supplied context.
    #[test]
    fn test_check_project_diagnostics_errors_bail() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("broken.baml");
        let broken = "function main( -> int { 1 }\n"; // missing param list close
        std::fs::write(&src, broken).unwrap();

        let mut db = ProjectDatabase::new();
        db.set_project_root(tmp.path());
        db.add_or_update_file(&src, broken);

        let err = check_project_diagnostics(&db, "bail-context-xyz").unwrap_err();
        assert!(
            format!("{err}").contains("bail-context-xyz"),
            "bail message should carry the caller's context; got: {err}",
        );
    }
}
