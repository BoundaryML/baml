use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use baml_project::ProjectDatabase;
use baml_workspace::{
    discover_baml_files, find_baml_project_root, project_search_dir, project_source_root,
    resolve_project_search_start,
};

use crate::reporter::Reporter;

/// The source location shape shared by `run`, `pack`, and `playground`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SourceLocation {
    /// A resolved BAML project root discovered by walking up from `from` or cwd.
    Project { root: PathBuf, files: Vec<PathBuf> },
    /// A hermetic single-file source selected through `--file`.
    StandaloneFile { file: PathBuf, root: PathBuf },
}

/// `--file` and `--from` both name a source location.
pub(crate) fn validate_file_from_flags(file: Option<&Path>, from: Option<&Path>) -> Result<()> {
    if file.is_some() && from.is_some() {
        anyhow::bail!(
            "`--file` and `--from` are mutually exclusive — `--file` already names \
             the single source to load."
        );
    }
    Ok(())
}

/// Resolve the CLI source location exactly like `baml run` project/file mode.
pub(crate) fn resolve_source_location(
    from: Option<&Path>,
    file: Option<&Path>,
    reporter: Option<&Reporter>,
) -> Result<SourceLocation> {
    validate_file_from_flags(file, from)?;

    if let Some(file) = file {
        let canonical = resolve_standalone_file(file)?;
        let root = canonical
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        return Ok(SourceLocation::StandaloneFile {
            file: canonical,
            root,
        });
    }

    let (_db, root, files) = match reporter {
        Some(reporter) => load_project_from_reporting(from, reporter)?,
        None => load_project_from(from)?,
    };
    Ok(SourceLocation::Project { root, files })
}

pub(crate) fn resolve_standalone_file(file_path: &Path) -> Result<PathBuf> {
    let display = file_path.display().to_string();
    let canonical =
        std::fs::canonicalize(file_path).with_context(|| format!("File not found: {display}"))?;
    if !canonical.is_file() {
        anyhow::bail!("`{}` is not a file.", canonical.display());
    }
    if canonical.extension().and_then(|ext| ext.to_str()) != Some("baml") {
        anyhow::bail!(
            "`{}` is not a BAML source file. Use a `.baml` file with `--file`.",
            canonical.display()
        );
    }
    Ok(canonical)
}

/// Resolve `from` (or cwd when omitted), walk up to the BAML project root, and
/// load all discovered `.baml` files into a fresh
/// [`ProjectDatabase`]. Returns the database, the canonical project root,
/// and the list of loaded files. Used by the build/execute commands
/// (`run`/`test`/`generate`/`pack`).
///
/// **`baml.toml` is opt-in.** A directory is a BAML project if it has
/// *either* a `baml.toml` *or* a `baml_src/` directory — `baml.toml` is
/// only needed when you actually use one of its features (dependencies,
/// version locks, `[scripts]`, multiple packages). The two cases:
///
/// - **Manifest present** (`baml.toml`): validated up front (Cargo-style,
///   `[package].name` is mandatory when you write a manifest). Sources come
///   from `baml_src/` if present, else the whole project root.
/// - **Manifest-less** (`baml_src/` only): no validation; sources are
///   loaded *only* from `baml_src/`. Because discovery is scoped to that
///   subdirectory, a manifest-less project can never slurp an unmarked
///   tree — which is exactly the workspace-shaped hang (`baml run`/`pack`
///   stalling while salsa type-infers hundreds of stray fixtures) the old
///   "`baml.toml` required" rule guarded against.
///
/// If the start path and its ancestors have *neither* marker, this is rejected
/// with a clear message that points at `baml_src/`, `baml init`, and `--file`.
///
/// Standalone single-file callers (`baml run --file …`, `baml pack --file
/// …`) skip this path entirely; they don't need a project at all.
///
/// Callers decide how to surface an empty `files` result (e.g. `run` bails,
/// `test` returns `NoTestsRun`, `generate` returns `Other`).
pub(crate) fn load_project_from(
    from: Option<&Path>,
) -> Result<(ProjectDatabase, PathBuf, Vec<PathBuf>)> {
    load_project_from_inner(from, |_| {})
}

/// Variant of [`load_project_from`] that announces each discovered
/// file through the [`Reporter`] as it's loaded — cargo's
/// `   Compiling foo v0.1.0` shape but for source files. Used by
/// `run`/`pack`/`test`/`generate` so the user sees per-file progress
/// instead of a single `Loading <project>` line. `grep`/`describe`
/// stay on the lenient [`load_project_or_default`].
pub(crate) fn load_project_from_reporting(
    from: Option<&Path>,
    reporter: &Reporter,
) -> Result<(ProjectDatabase, PathBuf, Vec<PathBuf>)> {
    load_project_from_inner(from, |path| {
        reporter.spin("Loading", path.display().to_string());
    })
}

/// Read-only/introspection loader: like [`load_project_from`] but **never
/// fails on a missing `baml.toml`**. This is for the commands an agent
/// reaches for first (`describe`, `grep`) — the most expensive thing they
/// can do is fail fast and burn a turn, so they always have something to
/// work with.
///
/// 1. Walk the ancestors of `from` (Cargo-style, stopping at the
///    filesystem root) for the nearest directory containing a `baml.toml`
///    or `baml_src/`. If one is found, load that project exactly as
///    [`load_project_from`] would — so running `baml describe` from a
///    subdirectory resolves the enclosing project. Manifest validation is
///    intentionally skipped: introspection doesn't need a valid
///    `[package].name`, and a malformed manifest shouldn't block it.
/// 2. If no project marker exists anywhere up the tree, return a **default
///    state**: a [`ProjectDatabase`] holding only the BAML stdlib
///    (`baml.*`, loaded by `set_project_root` regardless of user files)
///    and **zero user files**. This is what makes `baml describe
///    baml.String` work in any directory, with no project at all. The
///    empty `files` result is expected, not an error; callers resolve
///    against the stdlib regardless.
///
/// Note the two branches differ in what they load:
/// - The **default-state** branch (2) loads zero user files, so it can
///   never trigger the workspace-slurp hang that [`load_project_from`]
///   guards against.
/// - The **walk-up** branch (1) loads the ancestor project exactly as
///   [`load_project_from`] would — including the "no `baml_src/` → walk the
///   whole root" path in [`build_project_db`]. So if an ancestor
///   `baml.toml` sits at a workspace root with hundreds of loose `.baml`
///   files and no `baml_src/`, this *can* reintroduce that hang. That's a
///   deliberate trade-off: a directory with a `baml.toml` is a declared
///   project, and loading it is the same thing `run`/`test` already do from
///   that root. The slurp guard exists to reject *unmarked* directories, not
///   marked ones. Manifest-less projects use `baml_src/`, which scopes the
///   walk. (See the `walk_up_*` tests.)
pub(crate) fn load_project_or_default(
    from: Option<&Path>,
) -> Result<(ProjectDatabase, PathBuf, Vec<PathBuf>)> {
    let canonical = resolve_search_start(from)?;

    match find_baml_project_root(&canonical) {
        // Walk-up: load the ancestor project. Manifest validation is
        // intentionally skipped (see the docstring on point 1) — unlike the
        // strict `load_project_from` path, introspection doesn't need a
        // valid `[package].name`.
        Some(root) => build_project_db(root, |_| {}),
        None => {
            let mut db = ProjectDatabase::new();
            let root = project_search_dir(&canonical);
            db.set_project_root(&root);
            Ok((db, root, Vec::new()))
        }
    }
}

/// Resolve a path suitable for project discovery. When `from` is omitted,
/// discovery starts at the current directory.
fn resolve_search_start(from: Option<&Path>) -> Result<PathBuf> {
    resolve_project_search_start(from).with_context(|| match from {
        Some(from) => format!("Could not resolve path: {}", from.display()),
        None => "Could not resolve current directory".to_string(),
    })
}

pub(crate) fn find_project_root_from(from: Option<&Path>) -> Result<Option<PathBuf>> {
    let canonical = resolve_search_start(from)?;
    Ok(find_baml_project_root(&canonical))
}

fn load_project_from_inner(
    from: Option<&Path>,
    on_file: impl Fn(&Path),
) -> Result<(ProjectDatabase, PathBuf, Vec<PathBuf>)> {
    let canonical = resolve_search_start(from)?;
    let Some(canonical) = find_baml_project_root(&canonical) else {
        anyhow::bail!(
            "`{}` doesn't look like it belongs to a BAML project — no `baml.toml` \
             and no `baml_src/` directory found in it or its ancestors.\n\
             Add a `baml_src/` directory with your `.baml` files, run `baml init` \
             to create a `baml.toml`, or for a one-off script use `--file <PATH>`.",
            canonical.display()
        );
    };

    let toml_path = canonical.join("baml.toml");
    let has_baml_toml = toml_path.exists();

    // Manifest validation, Cargo-style: when a `baml.toml` is present it must
    // be valid (`[package].name` mandatory). Failing here — before any source
    // discovery or compilation — gives the same fast feedback as a malformed
    // `Cargo.toml`, rather than crashing several seconds in when a packaging
    // verb tries to use the name. A manifest-less `baml_src/` project skips
    // this: `baml.toml` is opt-in.
    if has_baml_toml {
        validate_baml_toml(&toml_path)?;
    }

    build_project_db(canonical, on_file)
}

/// Build a [`ProjectDatabase`] rooted at `canonical` (a directory already
/// known to be a project root). When `baml_src/` exists, only files under
/// it are loaded; otherwise the walk falls back to the root.
///
/// Shared by two callers with **different validation preconditions**: the
/// strict [`load_project_from`] path validates the manifest *before*
/// calling this, while the lenient [`load_project_or_default`] walk-up path
/// intentionally does not. Do **not** add manifest validation in here — it
/// would silently start rejecting manifests on the lenient path and break
/// project-less introspection.
fn build_project_db(
    canonical: PathBuf,
    on_file: impl Fn(&Path),
) -> Result<(ProjectDatabase, PathBuf, Vec<PathBuf>)> {
    let walk_root = project_source_root(&canonical);

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

/// Resolve the project's name for output-artifact naming (used by `baml
/// pack`). Prefers `[package].name` from `<root>/baml.toml` when a manifest
/// is present (guaranteed valid for any path we've loaded as a project).
/// For a manifest-less `baml_src/` project, falls back to the project
/// directory's name — the way `cargo` names a target after its directory
/// when no explicit name is given.
pub(crate) fn resolve_project_name(from: Option<&Path>) -> Result<String> {
    let canonical = resolve_search_start(from)?;
    let Some(canonical) = find_baml_project_root(&canonical) else {
        anyhow::bail!(
            "`{}` doesn't look like it belongs to a BAML project — no `baml.toml` \
             and no `baml_src/` directory found in it or its ancestors.",
            canonical.display()
        );
    };
    let toml_path = canonical.join("baml.toml");
    if toml_path.exists() {
        return validate_baml_toml(&toml_path);
    }
    canonical
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Could not derive a project name from `{}`; pass `-o <PATH>` to name the output.",
                canonical.display()
            )
        })
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

        let err = load_project_from(Some(tmp.path())).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("doesn't look like it belongs to a BAML project"),
            "got: {msg}"
        );
        assert!(msg.contains("baml.toml"), "got: {msg}");
        assert!(
            msg.contains("baml init") || msg.contains("--file"),
            "got: {msg}"
        );
    }

    /// `baml_src/` alone (no `baml.toml`) is a valid manifest-less project:
    /// `baml.toml` is opt-in. Sources load from `baml_src/` only.
    #[test]
    fn accepts_baml_src_only_without_baml_toml() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("baml_src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("main.baml"), "function main() -> int { 1 }").unwrap();

        let (_db, _root, files) = load_project_from(Some(tmp.path())).unwrap();
        assert_eq!(files.len(), 1, "got files: {files:?}");
        assert!(files[0].ends_with("main.baml"));
    }

    /// A manifest-less `baml_src/` project loads *only* `baml_src/` — loose
    /// `.baml` files at the root are NOT slurped, so the discovery stays
    /// bounded and can't trigger the workspace hang.
    #[test]
    fn baml_src_only_ignores_loose_root_files() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("baml_src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("main.baml"), "function main() -> int { 1 }").unwrap();
        // Loose file at the root (no baml.toml) must NOT be discovered.
        fs::write(
            tmp.path().join("stray.baml"),
            "function wrong() -> int { 2 }",
        )
        .unwrap();

        let (_db, _root, files) = load_project_from(Some(tmp.path())).unwrap();
        assert_eq!(files.len(), 1, "got files: {files:?}");
        assert!(files[0].ends_with("main.baml"));
    }

    /// `baml.toml` with no `[package]` table → manifest error at load
    /// time, before any sources are discovered.
    #[test]
    fn rejects_baml_toml_without_package_table() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("baml.toml"), "# empty\n").unwrap();
        let err = load_project_from(Some(tmp.path())).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("[package]"), "got: {msg}");
    }

    /// `baml.toml` with `[package]` but no `name` field → likewise.
    #[test]
    fn rejects_baml_toml_with_package_but_no_name() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("baml.toml"), "[package]\n").unwrap();
        let err = load_project_from(Some(tmp.path())).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("name"), "got: {msg}");
    }

    /// Empty `[package].name = \"\"` is also rejected.
    #[test]
    fn rejects_baml_toml_with_empty_name() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("baml.toml"), "[package]\nname = \"\"\n").unwrap();
        let err = load_project_from(Some(tmp.path())).unwrap_err();
        assert!(format!("{err}").contains("cannot be empty"));
    }

    /// `resolve_project_name` returns `[package].name` when a manifest is
    /// present.
    #[test]
    fn resolve_project_name_uses_package_name() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("baml.toml"),
            "[package]\nname = \"my-app\"\n",
        )
        .unwrap();
        assert_eq!(resolve_project_name(Some(tmp.path())).unwrap(), "my-app");
    }

    /// `resolve_project_name` falls back to the directory name for a
    /// manifest-less project (cargo-style), so `baml pack` can name the
    /// artifact without a `baml.toml`.
    #[test]
    fn resolve_project_name_falls_back_to_dir_name() {
        let tmp = TempDir::new().unwrap();
        let proj = tmp.path().join("my-cool-project");
        fs::create_dir(&proj).unwrap();
        fs::create_dir(proj.join("baml_src")).unwrap();

        assert_eq!(
            resolve_project_name(Some(&proj)).unwrap(),
            "my-cool-project"
        );
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

        let (_db, _root, files) = load_project_from(Some(tmp.path())).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("a.baml"));
    }

    #[test]
    fn project_loader_walks_up_from_baml_src_dir() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("baml.toml"), valid_manifest()).unwrap();
        let src = tmp.path().join("baml_src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("main.baml"), "function main() -> int { 1 }").unwrap();

        let (_db, root, files) = load_project_from(Some(&src)).unwrap();
        assert_eq!(root, std::fs::canonicalize(tmp.path()).unwrap());
        assert_eq!(files.len(), 1, "got files: {files:?}");
        assert!(files[0].ends_with("main.baml"));
    }

    #[test]
    fn project_loader_walks_up_from_nested_baml_src_dir() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("baml.toml"), valid_manifest()).unwrap();
        let nested = tmp.path().join("baml_src/nested/deeper");
        fs::create_dir_all(&nested).unwrap();
        fs::write(
            tmp.path().join("baml_src/main.baml"),
            "function main() -> int { 1 }",
        )
        .unwrap();

        let (_db, root, files) = load_project_from(Some(&nested)).unwrap();
        assert_eq!(root, std::fs::canonicalize(tmp.path()).unwrap());
        assert_eq!(files.len(), 1, "got files: {files:?}");
        assert!(files[0].ends_with("main.baml"));
    }

    #[test]
    fn project_loader_prefers_baml_toml_over_nearer_baml_src_marker() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("baml.toml"), valid_manifest()).unwrap();
        let nested = tmp.path().join("child/baml_src/nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(
            tmp.path().join("child/baml_src/main.baml"),
            "function main() -> int { 1 }",
        )
        .unwrap();

        let (_db, root, files) = load_project_from(Some(&nested)).unwrap();
        assert_eq!(root, std::fs::canonicalize(tmp.path()).unwrap());
        assert_eq!(files.len(), 1, "got files: {files:?}");
        assert!(files[0].ends_with("main.baml"));
    }

    // ── load_project_or_default (read-only / introspection path) ────────────

    /// A directory with `.baml` files but no `baml.toml` (and no ancestor
    /// `baml.toml`) yields the "default state": zero user files, but the
    /// stdlib is loaded so `baml describe baml.*` still resolves. The
    /// loose `.baml` is deliberately NOT slurped.
    #[test]
    fn default_state_loads_no_user_files_but_keeps_stdlib() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("loose.baml"),
            "function main() -> int { 1 }",
        )
        .unwrap();

        let (db, root, files) = load_project_or_default(Some(tmp.path())).unwrap();
        assert!(
            files.is_empty(),
            "default state must not slurp loose files: {files:?}"
        );
        assert_eq!(root, std::fs::canonicalize(tmp.path()).unwrap());
        // The builtin `baml` package is present even with no user files.
        let pkgs = baml_lsp2_actions::non_user_package_names(&db);
        assert!(
            pkgs.contains("baml"),
            "stdlib `baml` package missing from default state: {pkgs:?}"
        );
    }

    /// A manifest-less `baml_src/` project is a real project for introspection,
    /// matching `run`/`pack` discovery.
    #[test]
    fn introspection_loads_baml_src_only_project() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("baml_src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("main.baml"), "function main() -> int { 1 }").unwrap();

        let (_db, root, files) = load_project_or_default(Some(tmp.path())).unwrap();
        assert_eq!(root, std::fs::canonicalize(tmp.path()).unwrap());
        assert_eq!(files.len(), 1, "got files: {files:?}");
        assert!(files[0].ends_with("main.baml"));
    }

    /// `load_project_or_default` walks up to find an ancestor marker, so
    /// introspection from a subdirectory resolves the enclosing project.
    #[test]
    fn walk_up_finds_ancestor_project() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("baml.toml"), valid_manifest()).unwrap();
        let src = tmp.path().join("baml_src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("main.baml"), "function main() -> int { 1 }").unwrap();
        // A nested directory we invoke from.
        let nested = src.join("nested");
        fs::create_dir(&nested).unwrap();

        let (_db, root, files) = load_project_or_default(Some(&nested)).unwrap();
        assert_eq!(root, std::fs::canonicalize(tmp.path()).unwrap());
        assert_eq!(files.len(), 1, "got files: {files:?}");
        assert!(files[0].ends_with("main.baml"));
    }

    /// The same walk-up works for manifest-less `baml_src/` projects.
    #[test]
    fn walk_up_finds_ancestor_baml_src_only_project() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("baml_src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("main.baml"), "function main() -> int { 1 }").unwrap();
        let nested = src.join("nested");
        fs::create_dir(&nested).unwrap();

        let (_db, root, files) = load_project_or_default(Some(&nested)).unwrap();
        assert_eq!(root, std::fs::canonicalize(tmp.path()).unwrap());
        assert_eq!(files.len(), 1, "got files: {files:?}");
        assert!(files[0].ends_with("main.baml"));
    }

    /// Walk-up to an ancestor `baml.toml` that has **no `baml_src/`** loads
    /// the ancestor project by walking its whole root — the documented
    /// trade-off (a marked directory is a declared project, so loading it is
    /// intentional; the slurp guard only rejects *unmarked* dirs). This
    /// pins that behavior so it can't regress silently.
    #[test]
    fn walk_up_loads_ancestor_without_baml_src() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("baml.toml"), valid_manifest()).unwrap();
        // No baml_src/ — loose .baml files live at the root.
        fs::write(tmp.path().join("a.baml"), "function a() -> int { 1 }").unwrap();
        fs::write(tmp.path().join("b.baml"), "function b() -> int { 2 }").unwrap();
        // Invoke from a nested subdir so the walk-up path is exercised.
        let nested = tmp.path().join("sub").join("deeper");
        fs::create_dir_all(&nested).unwrap();

        let (_db, root, files) = load_project_or_default(Some(&nested)).unwrap();
        assert_eq!(root, std::fs::canonicalize(tmp.path()).unwrap());
        assert_eq!(
            files.len(),
            2,
            "ancestor project without baml_src should load its root .baml files: {files:?}"
        );
    }

    /// Walk-up tolerates a malformed manifest — introspection doesn't need
    /// a valid `[package].name`, unlike the strict `load_project_from` path.
    #[test]
    fn walk_up_tolerates_invalid_manifest() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("baml.toml"), "# no package table\n").unwrap();
        fs::write(tmp.path().join("a.baml"), "function main() -> int { 1 }").unwrap();

        // Strict loader rejects it...
        assert!(load_project_from(Some(tmp.path())).is_err());
        // ...but the introspection loader still loads the files.
        let (_db, _root, files) = load_project_or_default(Some(tmp.path())).unwrap();
        assert_eq!(files.len(), 1, "got files: {files:?}");
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

        let (_db, _root, files) = load_project_from(Some(tmp.path())).unwrap();
        assert_eq!(files.len(), 1, "got files: {files:?}");
        assert!(files[0].ends_with("main.baml"));
    }
}
