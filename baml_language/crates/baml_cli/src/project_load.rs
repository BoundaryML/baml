use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
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
