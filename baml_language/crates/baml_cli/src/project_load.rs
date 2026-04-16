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

#[cfg(test)]
mod tests {
    use super::*;

    // ── load_project_from — non-existent path ─────────────────────────────

    /// A path that does not exist on the filesystem must produce an error.
    #[test]
    fn test_load_project_from_nonexistent_path_errors() {
        let result = load_project_from(Path::new("/definitely/does/not/exist/baml_src"));
        assert!(
            result.is_err(),
            "non-existent path must return an error, got Ok"
        );
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("Could not resolve path"),
            "error should mention path resolution: {msg}"
        );
    }

    // ── load_project_from — empty directory ───────────────────────────────

    /// An existing directory with no `.baml` files returns an empty file list.
    /// The canonical root path returned must equal the actual directory on disk.
    #[test]
    fn test_load_project_from_empty_directory_returns_empty_list() {
        let tmp = tempfile::tempdir().unwrap();
        let (db, canonical, files) = load_project_from(tmp.path()).unwrap();

        assert!(files.is_empty(), "empty dir must yield no .baml files");
        assert_eq!(
            canonical,
            std::fs::canonicalize(tmp.path()).unwrap(),
            "canonical root must equal the real tempdir path"
        );
        // The database must have been initialised (get_project returns None only
        // when no root has been set — here it is set, but there are no files).
        let _ = db; // just confirm it compiles; project may or may not be Some
    }

    // ── load_project_from — directory with .baml files ───────────────────

    /// A directory that contains at least one `.baml` file must surface that
    /// file in the returned list.
    #[test]
    fn test_load_project_from_loads_baml_files() {
        let tmp = tempfile::tempdir().unwrap();
        let file_a = tmp.path().join("main.baml");
        let file_b = tmp.path().join("types.baml");
        std::fs::write(&file_a, "function main() -> int { 1 }\n").unwrap();
        std::fs::write(&file_b, "class Foo { x int }\n").unwrap();

        let (db, canonical, files) = load_project_from(tmp.path()).unwrap();

        assert_eq!(
            canonical,
            std::fs::canonicalize(tmp.path()).unwrap(),
            "canonical root must match"
        );
        assert_eq!(
            files.len(),
            2,
            "two .baml files should be discovered; got {files:?}"
        );

        // Each returned path must be a canonical absolute path.
        for f in &files {
            assert!(f.is_absolute(), "file path must be absolute: {f:?}");
        }

        // The source files registered in the database must match what was loaded.
        let source_files = db.get_source_files();
        assert_eq!(
            source_files.len(),
            2,
            "database must contain the same number of source files as loaded; got {source_files:?}"
        );
    }

    // ── load_project_from — non-.baml files are ignored ──────────────────

    /// Non-`.baml` files in the directory must NOT appear in the returned list.
    #[test]
    fn test_load_project_from_ignores_non_baml_files() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("readme.md"), "# docs\n").unwrap();
        std::fs::write(tmp.path().join("schema.json"), "{}").unwrap();
        std::fs::write(tmp.path().join("main.baml"), "function main() -> int { 0 }\n").unwrap();

        let (_db, _root, files) = load_project_from(tmp.path()).unwrap();
        assert_eq!(files.len(), 1, "only the .baml file should be loaded");
        assert!(
            files[0].extension().and_then(|e| e.to_str()) == Some("baml"),
            "the loaded file must have .baml extension"
        );
    }

    // ── load_project_from — returned path is canonical ───────────────────

    /// The canonical path returned must NOT be a relative path even if `from`
    /// is relative (e.g., `.`).
    #[test]
    fn test_load_project_from_returns_absolute_canonical_path() {
        let tmp = tempfile::tempdir().unwrap();

        // Use `tmp.path()` which is already absolute, then verify the return.
        let (_db, canonical, _files) = load_project_from(tmp.path()).unwrap();

        assert!(
            canonical.is_absolute(),
            "canonical root must be absolute, got: {canonical:?}"
        );
    }

    // ── load_project_from — files in subdirectory ─────────────────────────

    /// Files nested in subdirectories under the root should also be discovered.
    #[test]
    fn test_load_project_from_discovers_nested_baml_files() {
        let tmp = tempfile::tempdir().unwrap();
        let sub = tmp.path().join("models");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("user.baml"), "class User { id int }\n").unwrap();
        std::fs::write(tmp.path().join("main.baml"), "function main() -> int { 0 }\n").unwrap();

        let (_db, _root, files) = load_project_from(tmp.path()).unwrap();
        assert_eq!(
            files.len(),
            2,
            "should discover .baml files in subdirectories; got {files:?}"
        );
    }
}