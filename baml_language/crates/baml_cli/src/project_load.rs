use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use baml_project::ProjectDatabase;
use baml_workspace::discover_baml_files;

use crate::reporter::Reporter;

/// Canonicalize `from` and load all discovered `.baml` files into a fresh
/// [`ProjectDatabase`]. Returns the database, the canonical project root,
/// and the list of loaded files.
///
/// **`baml.toml` is required.** A project root must contain a `baml.toml`
/// (Cargo-style). Without it, the recursive walk would happily slurp every
/// `.baml` file under cwd, which in workspace-shaped directories means
/// hundreds of unrelated/conflicting fixtures. Symptom: `baml run`/`baml
/// pack` appears to hang for tens of seconds (it's actually salsa churning
/// through type inference on every fixture). Failing fast with a clear
/// message is the fix.
///
/// Standalone single-file callers (`baml run --file …`, `baml pack --file
/// …`) skip this path entirely; they don't need a `baml.toml`.
///
/// When `baml_src/` exists alongside `baml.toml`, only files under that
/// directory are loaded (mirroring `run_expression`'s `has_explicit_project`
/// branch). Without `baml_src/`, the walk falls back to the project root.
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

    let toml_path = canonical.join("baml.toml");
    if !toml_path.exists() {
        anyhow::bail!(
            "`{}` doesn't look like a BAML project — no `baml.toml` found.\n\
             Run `baml init` to create one, or pass `--from <project-dir>` to point\n\
             at an existing project. For one-off scripts, use `--file <PATH>` instead.",
            canonical.display()
        );
    }
    // Manifest validation, Cargo-style: `[package].name` is mandatory.
    // Failing here — before any source discovery or compilation — gives
    // the user the same fast feedback they get from a malformed
    // `Cargo.toml`, rather than crashing several seconds in when a
    // packaging verb tries to use the name.
    validate_baml_toml(&toml_path)?;

    let baml_src = canonical.join("baml_src");
    let has_baml_src = baml_src.is_dir();
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

/// Read `<root>/baml.toml` and assert it has `[package]` with a `name`
/// field, the way Cargo treats `Cargo.toml`'s `[package].name`. Returns
/// the resolved name as a byproduct so single callers (e.g. pack output
/// naming) can reuse it without re-parsing.
pub(crate) fn validate_baml_toml(toml_path: &Path) -> Result<String> {
    let content = std::fs::read_to_string(toml_path)
        .with_context(|| format!("Failed to read {}", toml_path.display()))?;
    let table: toml::Table = content
        .parse()
        .with_context(|| format!("Failed to parse {}", toml_path.display()))?;
    let package = table
        .get("package")
        .and_then(|v| v.as_table())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "{}: missing `[package]` table.\n\
             Add:\n\n    [package]\n    name = \"<your-project-name>\"\n",
                toml_path.display()
            )
        })?;
    let name = package
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "{}: `[package]` is missing `name = \"<your-project-name>\"`.",
                toml_path.display()
            )
        })?;
    if name.trim().is_empty() {
        anyhow::bail!("{}: `[package].name` cannot be empty.", toml_path.display());
    }
    Ok(name.to_string())
}

/// Convenience: read and return `[package].name` for a known-valid
/// manifest. The validation pass at load time means this is guaranteed
/// to succeed for any path we've actually loaded as a project; the
/// `Result` is purely for the file-IO surface.
pub(crate) fn read_package_name(root: &Path) -> Result<String> {
    validate_baml_toml(&root.join("baml.toml"))
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
    fn rejects_dir_with_no_baml_toml() {
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
        assert!(
            msg.contains("baml init") || msg.contains("--file"),
            "got: {msg}"
        );
    }

    /// `baml_src/` alone (no `baml.toml`) is now rejected — `baml.toml`
    /// is required, mirroring Cargo's `Cargo.toml` requirement.
    #[test]
    fn rejects_baml_src_only_without_baml_toml() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("baml_src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("main.baml"), "function main() -> int { 1 }").unwrap();

        let err = load_project_from(tmp.path()).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("baml.toml"), "got: {msg}");
    }

    /// `baml.toml` with no `[package]` table → manifest error at load
    /// time, before any sources are discovered.
    #[test]
    fn rejects_baml_toml_without_package_table() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("baml.toml"), "# empty\n").unwrap();
        let err = load_project_from(tmp.path()).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("[package]"), "got: {msg}");
    }

    /// `baml.toml` with `[package]` but no `name` field → likewise.
    #[test]
    fn rejects_baml_toml_with_package_but_no_name() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("baml.toml"), "[package]\n").unwrap();
        let err = load_project_from(tmp.path()).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("name"), "got: {msg}");
    }

    /// Empty `[package].name = \"\"` is also rejected.
    #[test]
    fn rejects_baml_toml_with_empty_name() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("baml.toml"), "[package]\nname = \"\"\n").unwrap();
        let err = load_project_from(tmp.path()).unwrap_err();
        assert!(format!("{err}").contains("cannot be empty"));
    }

    /// `read_package_name` returns the name for a valid manifest.
    #[test]
    fn read_package_name_returns_field() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("baml.toml"),
            "[package]\nname = \"my-app\"\n",
        )
        .unwrap();
        assert_eq!(read_package_name(tmp.path()).unwrap(), "my-app");
    }

    fn valid_manifest() -> &'static str {
        "[package]\nname = \"test-project\"\n"
    }

    /// `baml.toml`-only project root: walk recursively from the root.
    #[test]
    fn accepts_dir_with_baml_toml() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("baml.toml"), valid_manifest()).unwrap();
        fs::write(tmp.path().join("a.baml"), "function main() -> int { 1 }").unwrap();

        let (_db, _root, files) = load_project_from(tmp.path()).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("a.baml"));
    }

    /// `baml.toml` + `baml_src/` project layout: only the `baml_src/`
    /// subdirectory is walked, so stray `.baml` files outside (e.g. a
    /// stale top-level `test_simple.baml`) aren't pulled in.
    #[test]
    fn baml_src_layout_walks_only_baml_src_subtree() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("baml.toml"), valid_manifest()).unwrap();
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
