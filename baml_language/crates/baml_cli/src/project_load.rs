use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use baml_project::ProjectDatabase;
use baml_workspace::discover_baml_files;

use crate::reporter::Reporter;

/// Canonicalize `from` and load all discovered `.baml` files into a fresh
/// [`ProjectDatabase`]. Returns the database, the canonical project root,
/// and the list of loaded files.
///
/// **Project marker required.** `from` must contain a `baml.toml` or a
/// `baml_src/` subdirectory (matching the layout `baml run -e` already
/// recognizes — see `run_command::run_expression`). Without that, the
/// recursive walk would happily slurp every `.baml` file under cwd, which
/// in workspace-shaped directories means hundreds of unrelated/conflicting
/// fixtures. Symptom: `baml run`/`baml pack` appears to hang for tens of
/// seconds (it's actually salsa churning through type inference on every
/// fixture). Failing fast with a clear message is the fix.
///
/// When `baml_src/` exists, only files under that directory are loaded
/// (mirroring `run_expression`'s `has_explicit_project` branch). If only
/// `baml.toml` exists, the walk falls back to the project root.
///
/// Callers decide how to surface an empty `files` result (e.g. `run` bails,
/// `test` returns `NoTestsRun`, `generate` returns `Other`).
pub(crate) fn load_project_from(from: &Path) -> Result<(ProjectDatabase, PathBuf, Vec<PathBuf>)> {
    load_project_from_inner(from, |_| {})
}

/// Variant of [`load_project_from`] that announces each discovered
/// file through the [`Reporter`] as it's loaded — cargo's
/// `   Compiling foo v0.1.0` shape but for source files. Used by
/// `run`/`pack`/`test`/`generate` so the user sees per-file progress
/// instead of a single `Loading <project>` line. `grep`/`describe`
/// stay on the silent [`load_project_from`].
pub(crate) fn load_project_from_reporting(
    from: &Path,
    reporter: &Reporter,
) -> Result<(ProjectDatabase, PathBuf, Vec<PathBuf>)> {
    load_project_from_inner(from, |path| {
        reporter.spin("Loading", path.display().to_string());
    })
}

fn load_project_from_inner(
    from: &Path,
    on_file: impl Fn(&Path),
) -> Result<(ProjectDatabase, PathBuf, Vec<PathBuf>)> {
    let canonical = std::fs::canonicalize(from)
        .with_context(|| format!("Could not resolve path: {}", from.display()))?;

    let has_baml_toml = canonical.join("baml.toml").exists();
    let baml_src = canonical.join("baml_src");
    let has_baml_src = baml_src.is_dir();
    if !has_baml_toml && !has_baml_src {
        anyhow::bail!(
            "`{}` doesn't look like a BAML project.\n\
             Expected `baml.toml` or a `baml_src/` directory at the project root.\n\
             Pass `--from <project-dir>` or run from inside one.",
            canonical.display()
        );
    }

    let walk_root = if has_baml_src {
        baml_src
    } else {
        canonical.clone()
    };

    let mut db = ProjectDatabase::new();
    db.set_project_root(&canonical);
    let baml_files = discover_baml_files(&walk_root);
    for file_path in &baml_files {
        on_file(file_path);
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read {}", file_path.display()))?;
        db.add_or_update_file(file_path, &content);
    }
    Ok((db, canonical, baml_files))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    /// Regression for the `baml run` / `baml pack` indefinite hang when
    /// invoked from a directory that has `.baml` files but no project
    /// marker (e.g. the BAML workspace root, which contains 500+ unrelated
    /// fixtures across `demo/`, `sdk_tests/`, and `baml_tests/projects/`).
    /// Without a `baml.toml` or `baml_src/`, refuse to compile rather
    /// than slurp the whole subtree.
    #[test]
    fn rejects_dir_with_no_project_marker() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("loose.baml"),
            "function main() -> int { 1 }",
        )
        .unwrap();

        let err = load_project_from(tmp.path()).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("doesn't look like a BAML project"),
            "got: {msg}"
        );
        assert!(msg.contains("baml.toml"), "got: {msg}");
        assert!(msg.contains("baml_src/"), "got: {msg}");
    }

    /// `baml.toml`-only project root: walk recursively from the root.
    #[test]
    fn accepts_dir_with_baml_toml() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("baml.toml"), "").unwrap();
        fs::write(tmp.path().join("a.baml"), "function main() -> int { 1 }").unwrap();

        let (_db, _root, files) = load_project_from(tmp.path()).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("a.baml"));
    }

    /// `baml_src/` project layout: only that subdirectory is walked, so
    /// stray `.baml` files outside `baml_src/` (e.g. a stale top-level
    /// `test_simple.baml`) aren't pulled in.
    #[test]
    fn baml_src_layout_walks_only_baml_src_subtree() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("baml_src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("main.baml"), "function main() -> int { 1 }").unwrap();
        // Loose file at the root should NOT be discovered.
        fs::write(
            tmp.path().join("stray.baml"),
            "function wrong() -> int { 2 }",
        )
        .unwrap();

        let (_db, _root, files) = load_project_from(tmp.path()).unwrap();
        assert_eq!(files.len(), 1, "got files: {files:?}");
        assert!(files[0].ends_with("main.baml"));
    }
}
