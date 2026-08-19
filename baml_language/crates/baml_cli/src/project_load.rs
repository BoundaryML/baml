use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use baml_project::ProjectDatabase;
use baml_workspace::{
    BAML_TOML, discover_baml_files, find_baml_project_root, project_search_dir,
    project_source_root, resolve_project_search_start,
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

/// `--file` and `--project` both name a source location.
pub(crate) fn validate_file_project_flags(
    file: Option<&Path>,
    project: Option<&Path>,
) -> Result<()> {
    if file.is_some() && project.is_some() {
        anyhow::bail!(
            "`--file` and `--project` are mutually exclusive; `--file` already names \
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
    validate_file_project_flags(file, from)?;

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
        std::fs::canonicalize(file_path).with_context(|| format!("file not found: {display}"))?;
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

/// Resolve `from` (or cwd when omitted) and load all discovered `.baml` files
/// into a fresh
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
/// An explicit `--project` is also a source-root escape hatch. Normal containing-
/// project discovery still wins when the supplied path overlaps the selected
/// `baml_src/`, but a disjoint sibling tree is loaded directly instead of being
/// silently redirected to that `baml_src/`. The nearest ancestor `baml.toml`
/// still supplies package/settings in that case. With no ancestor manifest,
/// the explicit directory is a standalone manifest-less project.
///
/// Without an explicit `--project`, a marker is still required. This keeps default
/// current-directory discovery from recursively loading an arbitrary workspace.
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
/// `   Compiling foo v0.1.0` shape but for source files. The per-file
/// `Loading <path>` flood is verbose-only detail, reachable through
/// [`resolve_source_location`] with a reporter; command code goes through
/// `ProjectSession` instead.
pub(crate) fn load_project_from_reporting(
    from: Option<&Path>,
    reporter: &Reporter,
) -> Result<(ProjectDatabase, PathBuf, Vec<PathBuf>)> {
    load_project_from_inner(from, |path| {
        reporter.spin("Loading", path.display().to_string());
    })
}

/// Lenient counterpart to [`resolve_project_sources`] for introspection
/// sessions: `Ok(None)` when no project root is found (instead of an error),
/// and a present `baml.toml` is read **raw, unvalidated** — its bytes still
/// key the bytecode cache identically to the strict path, but a broken
/// manifest must not lock an agent out of `describe`.
pub(crate) fn resolve_project_sources_lenient(
    from: Option<&Path>,
) -> Result<Option<ResolvedProject>> {
    let Some(layout) = resolve_project_layout(from)? else {
        return Ok(None);
    };
    let toml_path = layout.root.join(BAML_TOML);
    let manifest = if toml_path.exists() {
        std::fs::read_to_string(&toml_path).ok()
    } else {
        None
    };
    use rayon::prelude::*;
    let files = discover_baml_files(&layout.source_root)
        .into_par_iter()
        .map(|path| {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            Ok((path, content))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Some(ResolvedProject {
        root: layout.root,
        source_root: layout.source_root,
        manifest,
        files,
    }))
}

/// The directory a projectless introspection session roots at (the same
/// fallback [`load_project_or_default`] uses when no project is found).
pub(crate) fn projectless_search_dir(from: Option<&Path>) -> Result<PathBuf> {
    let canonical = resolve_search_start(from)?;
    Ok(project_search_dir(&canonical))
}

/// Read-only/introspection loader: like [`load_project_from`] but **never
/// fails on a missing `baml.toml`**. This is for the commands an agent
/// reaches for first (`describe`) - the most expensive thing it can
/// do is fail fast and burn a turn, so it always has something to
/// work with.
///
/// 1. Resolve explicit `from` with the same project/source split as
///    [`load_project_from`]. A containing project's settings remain available,
///    while a disjoint explicit source tree is loaded directly. Manifest
///    validation is intentionally skipped: introspection doesn't need a valid
///    `[package].name`, and a malformed manifest shouldn't block it.
/// 2. With no explicit `from`, walk ancestors for `baml.toml` or `baml_src/`.
///    If no marker exists, return a **default state** holding only the BAML
///    stdlib (`baml.*`) and zero user files. This makes `baml describe
///    baml.String` work in any directory without recursively loading it.
///
/// Note the two branches differ in what they load:
/// - The **default-state** branch (2) loads zero user files, so omitted
///   `--project` can never trigger the workspace-slurp hang.
/// - The **walk-up** branch (1) loads the ancestor project exactly as
///   [`load_project_from`] would — including the "no `baml_src/` → walk the
///   whole root" path. So if an ancestor
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
    match resolve_project_sources_lenient(from)? {
        // Walk-up or explicit source root. Manifest validation is intentionally
        // skipped — introspection doesn't need a valid `[package].name`.
        Some(resolved) => {
            let files = resolved
                .files
                .iter()
                .map(|(path, _)| path.clone())
                .collect();
            let root = resolved.root.clone();
            let db = build_db_from_sources(&resolved, |_| {});
            Ok((db, root, files))
        }
        None => {
            let canonical = resolve_search_start(from)?;
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
        Some(from) => format!("could not resolve path: {}", from.display()),
        None => "could not resolve current directory".to_string(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectLayout {
    /// Root that owns the effective `baml.toml`, or the explicit standalone
    /// source directory when no ancestor manifest applies.
    pub(crate) root: PathBuf,
    /// Directory recursively searched for `.baml` source files.
    pub(crate) source_root: PathBuf,
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

/// Resolve the settings root and source root for a CLI invocation.
///
/// With no explicit `from`, this is ordinary marker-based project discovery.
/// With an explicit `from`, a containing project is used only when its selected
/// source tree overlaps the supplied path. A disjoint path is treated as an
/// explicit source root; an ancestor manifest still supplies settings.
pub(crate) fn resolve_project_layout(from: Option<&Path>) -> Result<Option<ProjectLayout>> {
    let canonical = resolve_search_start(from)?;
    let explicit_root = project_search_dir(&canonical);

    match find_baml_project_root(&canonical) {
        Some(discovered_root) => {
            let discovered_source_root = project_source_root(&discovered_root);
            if from.is_none() || paths_overlap(&explicit_root, &discovered_source_root) {
                return Ok(Some(ProjectLayout {
                    root: discovered_root,
                    source_root: discovered_source_root,
                }));
            }

            // The explicit path and the discovered baml_src are siblings (or
            // otherwise disjoint). Honor the explicit source path, but retain
            // an ancestor manifest's package/settings when one exists.
            let root = if discovered_root.join(BAML_TOML).is_file() {
                discovered_root
            } else {
                explicit_root.clone()
            };
            Ok(Some(ProjectLayout {
                root,
                source_root: project_source_root(&explicit_root),
            }))
        }
        None if from.is_some() => Ok(Some(ProjectLayout {
            root: explicit_root.clone(),
            source_root: project_source_root(&explicit_root),
        })),
        None => Ok(None),
    }
}

pub(crate) fn find_project_root_from(from: Option<&Path>) -> Result<Option<PathBuf>> {
    Ok(resolve_project_layout(from)?.map(|layout| layout.root))
}

fn load_project_from_inner(
    from: Option<&Path>,
    on_file: impl Fn(&Path),
) -> Result<(ProjectDatabase, PathBuf, Vec<PathBuf>)> {
    let resolved = resolve_project_sources(from)?;
    let files = resolved.files.iter().map(|(p, _)| p.clone()).collect();
    let db = build_db_from_sources(&resolved, on_file);
    Ok((db, resolved.root, files))
}

/// A resolved project with every source read into memory but no
/// [`ProjectDatabase`] built yet.
///
/// This is the seam the bytecode cache needs: computing a cache key requires
/// the root, manifest, and file contents — but *not* a database. On a cache
/// hit the database (and everything downstream of it: typecheck, emit) is
/// skipped entirely; on a miss [`build_db_from_sources`] reuses these
/// already-read contents instead of re-reading from disk.
pub(crate) struct ResolvedProject {
    /// Canonical settings root: the directory holding the effective
    /// `baml.toml`, or the explicit standalone source root without a manifest.
    pub root: PathBuf,
    /// Directory the `.baml` files were discovered under. Usually
    /// `root/baml_src`, but an explicit `--project` pointing at a disjoint
    /// source tree makes it something else entirely — so it is recorded
    /// rather than re-derived, since re-deriving it from `root` would name a
    /// different file set than the one actually loaded.
    pub source_root: PathBuf,
    /// `baml.toml` content when present (already validated).
    pub manifest: Option<String>,
    /// Discovered `.baml` files with contents, in discovery (sorted) order.
    pub files: Vec<(PathBuf, String)>,
}

/// Resolve the project root, validate the manifest, and read every source
/// file into memory. The strict-path front half of [`load_project_from`].
pub(crate) fn resolve_project_sources(from: Option<&Path>) -> Result<ResolvedProject> {
    let search_start = resolve_search_start(from)?;
    let Some(layout) = resolve_project_layout(from)? else {
        anyhow::bail!(
            "`{}` doesn't look like it belongs to a BAML project — no `baml.toml` \
             and no `baml_src/` directory found in it or its ancestors.\n\
             add a `baml_src/` directory with your `.baml` files, run `baml init`, \
             or pass `--project <DIR>` to load an explicit source directory.",
            search_start.display()
        );
    };

    let toml_path = layout.root.join(BAML_TOML);

    // Manifest validation, Cargo-style: when a `baml.toml` is present it must
    // be valid (`[package].name` mandatory). Failing here — before any source
    // discovery or compilation — gives the same fast feedback as a malformed
    // `Cargo.toml`, rather than crashing several seconds in when a packaging
    // verb tries to use the name. A manifest-less `baml_src/` project skips
    // this: `baml.toml` is opt-in.
    let manifest = if toml_path.exists() {
        let content = std::fs::read_to_string(&toml_path)
            .with_context(|| format!("failed to read {}", toml_path.display()))?;
        let manifest = crate::manifest::parse(&content)
            .with_context(|| format!("failed to parse {}", toml_path.display()))?;
        crate::manifest::package_name(&manifest, &toml_path)?;
        // Unknown keys are advisory, not fatal: a typo (`[scriptz]`,
        // `nmae = ...`) warns rather than silently no-ops, but a
        // forward-compatible manifest still loads.
        for warning in crate::manifest::unknown_field_warnings(&manifest) {
            crate::reporter::print_warning(format_args!("{warning}"));
        }
        Some(content)
    } else {
        None
    };

    // Read sources across worker threads; `collect` on an indexed parallel
    // iterator preserves discovery order, so `FileId` assignment (and with it
    // diagnostic ordering) is identical to a serial read.
    use rayon::prelude::*;
    let files = discover_baml_files(&layout.source_root)
        .into_par_iter()
        .map(|path| {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            Ok((path, content))
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(ResolvedProject {
        root: layout.root,
        source_root: layout.source_root,
        manifest,
        files,
    })
}

/// Build a [`ProjectDatabase`] from already-read sources.
pub(crate) fn build_db_from_sources(
    resolved: &ResolvedProject,
    on_file: impl Fn(&Path),
) -> ProjectDatabase {
    let mut db = ProjectDatabase::new();
    db.set_project_root(&resolved.root);
    for (path, _) in &resolved.files {
        on_file(path);
    }
    // Bulk registration: one project-file-list write instead of one per file
    // (the per-file path is O(files²) Vec copies + one salsa revision each).
    db.add_or_update_files(
        resolved
            .files
            .iter()
            .map(|(path, content)| (path.as_path(), content.as_str())),
    );
    db
}

/// Read `<root>/baml.toml` and assert it has `[package]` with a `name`
/// field, the way Cargo treats `Cargo.toml`'s `[package].name`. Returns
/// the resolved name as a byproduct so single callers (e.g. pack output
/// naming) can reuse it without re-parsing.
pub(crate) fn validate_baml_toml(toml_path: &Path) -> Result<String> {
    let content = std::fs::read_to_string(toml_path)
        .with_context(|| format!("failed to read {}", toml_path.display()))?;
    let manifest = crate::manifest::parse(&content)
        .with_context(|| format!("failed to parse {}", toml_path.display()))?;
    crate::manifest::package_name(&manifest, toml_path)
}

/// Resolve the project's name for output-artifact naming (used by `baml
/// pack`). Prefers `[package].name` from `<root>/baml.toml` when a manifest
/// is present (guaranteed valid for any path we've loaded as a project).
/// For a manifest-less `baml_src/` project, falls back to the project
/// directory's name — the way `cargo` names a target after its directory
/// when no explicit name is given.
pub(crate) fn resolve_project_name(from: Option<&Path>) -> Result<String> {
    let search_start = resolve_search_start(from)?;
    let Some(layout) = resolve_project_layout(from)? else {
        anyhow::bail!(
            "`{}` doesn't look like it belongs to a BAML project — no `baml.toml` \
             and no `baml_src/` directory found in it or its ancestors.",
            search_start.display()
        );
    };
    let toml_path = layout.root.join(BAML_TOML);
    if toml_path.exists() {
        return validate_baml_toml(&toml_path);
    }
    layout
        .root
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "could not derive a project name from `{}`; pass `-o <PATH>` to name the output.",
                layout.root.display()
            )
        })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    /// An explicit source directory is intentionally sufficient even without
    /// `baml.toml` or a `baml_src/` wrapper.
    #[test]
    fn accepts_explicit_unmarked_source_directory() {
        let tmp = TempDir::new().unwrap();
        let source = tmp.path().join("loose.baml");
        fs::write(&source, "function main() -> int { 1 }").unwrap();

        let resolved = resolve_project_sources(Some(tmp.path())).unwrap();
        assert_eq!(resolved.root, fs::canonicalize(tmp.path()).unwrap());
        assert_eq!(resolved.manifest, None);
        assert_eq!(resolved.files.len(), 1);
        assert_eq!(resolved.files[0].0, fs::canonicalize(source).unwrap());
    }

    /// A sibling source tree must not be redirected to the conventional
    /// `baml_src/` selected by marker discovery.
    #[test]
    fn explicit_sibling_source_overrides_manifestless_baml_src() {
        let tmp = TempDir::new().unwrap();
        let primary = tmp.path().join("baml_src");
        let alternate = tmp.path().join("baml_src_temp2");
        fs::create_dir(&primary).unwrap();
        fs::create_dir(&alternate).unwrap();
        fs::write(
            primary.join("primary.baml"),
            "function primary() -> int { 1 }",
        )
        .unwrap();
        fs::write(
            alternate.join("alternate.baml"),
            "function alternate() -> int { 2 }",
        )
        .unwrap();

        let resolved = resolve_project_sources(Some(&alternate)).unwrap();
        assert_eq!(resolved.root, fs::canonicalize(&alternate).unwrap());
        assert_eq!(resolved.manifest, None);
        assert_eq!(resolved.files.len(), 1);
        assert!(resolved.files[0].0.ends_with("alternate.baml"));
    }

    /// An ancestor manifest remains the settings/package root even when
    /// `--project` selects a disjoint source tree.
    #[test]
    fn explicit_sibling_source_retains_ancestor_manifest() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join(BAML_TOML), valid_manifest()).unwrap();
        let primary = tmp.path().join("baml_src");
        let alternate = tmp.path().join("baml_src_temp2");
        fs::create_dir(&primary).unwrap();
        fs::create_dir(&alternate).unwrap();
        fs::write(
            primary.join("primary.baml"),
            "function primary() -> int { 1 }",
        )
        .unwrap();
        fs::write(
            alternate.join("alternate.baml"),
            "function alternate() -> int { 2 }",
        )
        .unwrap();

        let resolved = resolve_project_sources(Some(&alternate)).unwrap();
        assert_eq!(resolved.root, fs::canonicalize(tmp.path()).unwrap());
        assert_eq!(resolved.manifest.as_deref(), Some(valid_manifest()));
        assert_eq!(resolved.files.len(), 1);
        assert!(resolved.files[0].0.ends_with("alternate.baml"));
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

    /// Explicit unmarked source roots behave consistently for introspection:
    /// user files and the stdlib are both loaded.
    #[test]
    fn introspection_loads_explicit_unmarked_source_and_stdlib() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("loose.baml"),
            "function main() -> int { 1 }",
        )
        .unwrap();

        let (db, root, files) = load_project_or_default(Some(tmp.path())).unwrap();
        assert_eq!(files.len(), 1, "got files: {files:?}");
        assert!(files[0].ends_with("loose.baml"));
        assert_eq!(root, std::fs::canonicalize(tmp.path()).unwrap());
        // The builtin `baml` package is present even with no user files.
        let baml_pkg = baml_surface::Package::named(&db, "baml");
        assert!(
            !baml_pkg.namespaces(&db).is_empty(),
            "stdlib `baml` package missing from default state"
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
