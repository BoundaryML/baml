//! Codegen for the sdk-test crates under `sdk_tests/crates/`. Each generator
//! fans out over the shared fixture corpus
//! (`sdk_tests/fixtures/<fixture>/baml_src/`) and produces one
//! `crates/<generator>/<fixture>/generated/` tree per fixture. C# is the
//! exception: its fixtures are whole BAML projects living in-crate.
//!
//! This crate is generator-agnostic: it loads `.baml` source into a
//! [`ProjectDatabase`], gates on diagnostics, installs each generator's output
//! through the same filesystem transaction `baml generate` uses, and stages the
//! `customizable/` overlay of ported tests into the generated tree.
//! Generator-specific entry points live in submodules like [`cpp`],
//! [`python_pydantic2`] and [`typescript`].
//!
//! Every entry point is driven by the `sdk_test_codegen` binary
//! (`src/main.rs`), which a generator crate's `setup.sh` runs before its tests.
//! No generator has a `build.rs`: that path made every `cargo check` build this
//! crate's whole compiler and sdkgen closure. Generator crates declare their
//! tests in source instead, via
//! `sdk_test_harness_runner::<generator>::test_suite!`.
//!
//! Two owners cover everything written into a fixture's `generated/` tree, so
//! nothing has to be wiped to stay correct: `write_codegen_output` owns the
//! emitted SDK subtree, and [`Overlay`] owns the ported-test overlay plus the
//! per-fixture scaffolding. Both skip files whose bytes already match.
//!
//! Layout the helpers assume:
//!
//! ```text
//! sdk_tests/
//! ├── fixtures/<fixture>/baml_src/                # .baml only
//! └── crates/<generator>/<fixture>/
//!     ├── customizable/                           # ported tests, tracked
//!     └── generated/                              # codegen output, gitignored
//! ```
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use baml_codegen_types::{GeneratedOutputFile, SymbolPool, write_generated_output};
use baml_db::{ProjectDatabase, SourceRootSpec, baml_compiler_diagnostics::Severity};

pub mod cpp;
pub mod csharp;
pub mod go;
pub mod java;
pub mod python_pydantic2;
pub mod rust;
pub mod swift;
pub mod typescript;
pub mod typescript_web;

/// Where one generator's codegen reads from and writes to.
///
/// The driver builds this once and hands it to a generator, so generators
/// never read `CARGO_MANIFEST_DIR` themselves: inside a build script that
/// meant the *generator crate's* directory, and inside the driver it would
/// mean the driver's own.
pub struct CodegenCtx {
    /// `<workspace>/sdk_tests/fixtures` — the generator-agnostic corpus.
    /// C# is the exception: its fixtures live in-crate under [`Self::crate_dir`].
    pub fixtures_root: PathBuf,
    /// `<workspace>/sdk_tests/crates/<generator>`, the root of everything this
    /// generator installs.
    pub crate_dir: PathBuf,
}

impl CodegenCtx {
    /// Derive both roots from `sdk_tests_root`, checking that they exist.
    ///
    /// Every path a generator writes hangs off `crate_dir`, so a root that
    /// silently resolved wrong would install a whole generated tree in the
    /// wrong place rather than fail.
    pub fn new(sdk_tests_root: &Path, generator: &str) -> Self {
        let fixtures_root = sdk_tests_root.join("fixtures");
        let crate_dir = sdk_tests_root.join("crates").join(generator);
        assert!(
            fixtures_root.is_dir(),
            "no fixtures corpus at {} — is {} an sdk_tests root?",
            fixtures_root.display(),
            sdk_tests_root.display()
        );
        assert!(
            crate_dir.is_dir(),
            "no `{generator}` generator crate at {}",
            crate_dir.display()
        );
        Self {
            fixtures_root,
            crate_dir,
        }
    }
}

/// Install an SDK generator's complete output through the same filesystem
/// transaction used by `baml generate`.
///
/// Panics on failure: this runs from a generator crate's `setup.sh`, where a
/// failed install must stop the run before the language toolchain builds
/// against a half-installed tree.
pub(crate) fn write_codegen_output<C>(
    output_directory: &Path,
    output: impl IntoIterator<Item = (PathBuf, C)>,
    fixture: &str,
) where
    C: AsRef<[u8]>,
{
    if let Err(error) = install(output_directory, output) {
        panic!(
            "fixture `{fixture}`: failed to install generated output in {}: {error}",
            output_directory.display()
        );
    }
}

fn install<C>(
    output_directory: &Path,
    output: impl IntoIterator<Item = (PathBuf, C)>,
) -> Result<(), baml_codegen_types::OutputWriterError>
where
    C: AsRef<[u8]>,
{
    let files = output
        .into_iter()
        .map(|(relative_path, contents)| {
            GeneratedOutputFile::new(relative_path, contents.as_ref().to_vec())
        })
        .collect();
    write_generated_output(output_directory, files).map(|_| ())
}

/// A user BAML source file as it should appear in the emitter's
/// inlined-baml output. `rel_path` is relative to the fixture's
/// `baml_src/` root.
pub type UserBamlFile = (PathBuf, String);

/// Output of [`load_fixture`]: everything a generator needs to call
/// its language-specific `to_source_code` entry point.
pub struct LoadedFixture {
    pub baml_src: PathBuf,
    pub pool: SymbolPool,
    pub user_baml_files: Vec<UserBamlFile>,
    pub baml_bytecode: Vec<u8>,
}

/// Discover .baml files for one fixture, gate on diagnostics, and
/// build the codegen `SymbolPool` + inlined-files list. Panics with
/// the fixture name in the message on any compile error so a broken
/// `.baml` source doesn't masquerade as a codegen bug.
pub fn load_fixture(fixtures_root: &Path, fixture: &str) -> LoadedFixture {
    let baml_src = fixtures_root.join(fixture).join("baml_src");
    let canonical = fs::canonicalize(&baml_src)
        .unwrap_or_else(|_| panic!("baml_src not found at {}", baml_src.display()));

    let mut db = ProjectDatabase::new();
    db.ensure_stdlib_sources();
    let root = db
        .add_source_root(SourceRootSpec::new(
            canonical.clone(),
            baml_db::SourceRootKind::Workspace,
        ))
        .unwrap_or_else(|e| panic!("fixture `{fixture}`: cannot add workspace root: {e}"));
    let baml_files = baml_db::discover_baml_files(&canonical);
    assert!(
        !baml_files.is_empty(),
        "fixture `{fixture}`: no .baml files under {}",
        canonical.display()
    );
    for file_path in &baml_files {
        let content = fs::read_to_string(file_path)
            .unwrap_or_else(|_| panic!("failed to read {}", file_path.display()));
        db.add_or_update_file_in(root, file_path, &content);
    }

    let source_files = db.workspace_files();
    let diagnostics = baml_db::collect_diagnostics(&db);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    if !errors.is_empty() {
        let messages: Vec<String> = errors.iter().map(|d| format!("{d:?}")).collect();
        panic!(
            "fixture `{fixture}`: baml_src has compile errors:\n{}",
            messages.join("\n")
        );
    }

    let pool = baml_ide::build_symbol_pool(&db);
    let program = db
        .get_bytecode(root)
        .unwrap_or_else(|e| panic!("fixture `{fixture}`: bytecode compilation failed: {e:?}"));
    let baml_bytecode = baml_artifact::encode(baml_artifact::ArtifactKind::Program, &program)
        .unwrap_or_else(|e| panic!("fixture `{fixture}`: bytecode serialization failed: {e}"));
    let user_baml_files: Vec<UserBamlFile> = source_files
        .iter()
        .map(|sf| {
            let path = sf.path(&db);
            let rel = path.strip_prefix(&canonical).unwrap_or(&path).to_path_buf();
            (rel, sf.text(&db).to_string())
        })
        .collect();

    LoadedFixture {
        baml_src: canonical,
        pool,
        user_baml_files,
        baml_bytecode,
    }
}

/// Recursively copy `customizable_dir` into `dst_dir`. Used by the Go, Java,
/// Swift and TypeScript targets: symlinks would force every parallel test
/// process to either set `NODE_OPTIONS=--preserve-symlinks` (which breaks the
/// pnpm CLI, itself a symlinked node script) or let node follow the symlink
/// and resolve `node_modules` from `customizable/` (which has none). Copying
/// sidesteps both.
///
/// Stages only what `customizable_dir` holds *now* and keeps no record of it,
/// so a file deleted upstream leaves its copy behind. Callers are responsible
/// for clearing the destination before re-staging — see [`symlink_customizable`]
/// for the same contract.
pub fn copy_customizable(customizable_dir: &Path, dst_dir: &Path) {
    for entry in fs::read_dir(customizable_dir).unwrap() {
        let entry = entry.unwrap();
        let src = entry.path();
        let file_name = entry.file_name();
        let dst = dst_dir.join(&file_name);

        if src.is_dir() {
            fs::create_dir_all(&dst).unwrap_or_else(|e| {
                panic!(
                    "Failed to create {} for customizable overlay: {e}",
                    dst.display()
                )
            });
            copy_customizable(&src, &dst);
            continue;
        }
        if !src.is_file() {
            continue;
        }
        if dst.exists() || dst.symlink_metadata().is_ok() {
            let _ = fs::remove_file(&dst);
        }
        fs::copy(&src, &dst).unwrap_or_else(|e| {
            panic!(
                "Failed to copy {} from {}: {e}",
                file_name.to_string_lossy(),
                src.display()
            )
        });
    }
}

/// Symlink every file in `customizable_dir` into `dst_dir`. Used by the C++,
/// Python and Rust targets, so an edit to a ported test is picked up without
/// re-staging. On Windows `symlink_file` requires Developer Mode or admin, so
/// a failed symlink falls back to `fs::copy`, which gives up that property
/// until the next re-stage.
///
/// Stages only what `customizable_dir` holds *now* and keeps no record of it,
/// so a file deleted upstream leaves a live link behind — pointing at a path
/// that no longer exists, or worse, still compiling. Callers are responsible
/// for clearing the destination before re-staging.
pub fn symlink_customizable(customizable_dir: &Path, dst_dir: &Path) {
    for entry in fs::read_dir(customizable_dir).unwrap() {
        let entry = entry.unwrap();
        let src = entry.path();
        let file_name = entry.file_name();
        let dst = dst_dir.join(&file_name);

        // Recurse into subdirectories (e.g. `roundtrip_tests/`), mirroring
        // the tree into the generated dir so pytest discovers nested
        // `test_*.py` modules. Files inside are symlinked individually.
        if src.is_dir() {
            fs::create_dir_all(&dst).unwrap_or_else(|e| {
                panic!(
                    "Failed to create {} for customizable overlay: {e}",
                    dst.display()
                )
            });
            symlink_customizable(&src, &dst);
            continue;
        }
        if !src.is_file() {
            continue;
        }
        if dst.exists() || dst.symlink_metadata().is_ok() {
            let _ = fs::remove_file(&dst);
        }
        #[cfg(unix)]
        let symlink_result = std::os::unix::fs::symlink(&src, &dst);
        #[cfg(windows)]
        let symlink_result = std::os::windows::fs::symlink_file(&src, &dst);
        if symlink_result.is_err() {
            fs::copy(&src, &dst).unwrap_or_else(|e| {
                panic!(
                    "Failed to symlink or copy {} from {}: {e}",
                    file_name.to_string_lossy(),
                    src.display()
                )
            });
        }
    }
}

/// Everything a generator installs into a fixture's `generated/` tree that the
/// codegen writer does not own: the `customizable/` overlay of ported tests,
/// and the per-fixture scaffolding (`package.json`, `go.mod`, `Package.swift`,
/// and friends).
///
/// Records what it staged in `<fixture>/.baml-overlay`, so the next run removes
/// exactly what the previous one left and nothing else. Without such a record a
/// generator has to wipe the tree to stay correct, which also destroys the
/// output writer's byte-identical skip and hands every downstream toolchain a
/// fresh set of mtimes.
///
/// The manifest lives beside `generated/` rather than inside it so it cannot
/// collide with the output writer, which owns its own subtree and treats
/// anything unfamiliar there as a file to preserve.
pub struct Overlay {
    /// The tree entries are staged into, i.e. `<fixture>/generated`.
    root: PathBuf,
    /// Where the record of the previous run lives.
    manifest: PathBuf,
    entries: BTreeMap<String, Entry>,
}

/// How one overlay entry is materialized.
enum Entry {
    /// A symlink back to `source`, so editing the original is picked up without
    /// re-staging. Falls back to a copy where symlinks are unavailable
    /// (Windows without Developer Mode).
    Link(PathBuf),
    /// Literal bytes — scaffolding, or a copied file whose content the
    /// generator rewrote.
    Content(Vec<u8>),
}

const OVERLAY_MANIFEST: &str = ".baml-overlay";

impl Overlay {
    /// Stage into `<fixture_root>/generated`, recording in
    /// `<fixture_root>/.baml-overlay`.
    pub fn new(fixture_root: &Path) -> Self {
        Self {
            root: fixture_root.join("generated"),
            manifest: fixture_root.join(OVERLAY_MANIFEST),
            entries: BTreeMap::new(),
        }
    }

    /// Stage `contents` at `relative`, a path below the generated tree.
    pub fn file(&mut self, relative: impl AsRef<Path>, contents: impl Into<Vec<u8>>) {
        self.insert(relative.as_ref(), Entry::Content(contents.into()));
    }

    /// Stage every file under `source` at the same shape below `prefix`, as
    /// symlinks. Missing `source` stages nothing.
    pub fn link_tree(&mut self, source: &Path, prefix: impl AsRef<Path>) {
        for (relative, path) in tree_files(source) {
            self.insert(&prefix.as_ref().join(relative), Entry::Link(path));
        }
    }

    /// Stage every file under `source` at the same shape below `prefix`, by
    /// value. Used where a symlink would break the language's module
    /// resolution or source-root rules.
    pub fn copy_tree(&mut self, source: &Path, prefix: impl AsRef<Path>) {
        for (relative, path) in tree_files(source) {
            let contents = fs::read(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            self.insert(&prefix.as_ref().join(relative), Entry::Content(contents));
        }
    }

    /// Rewrite staged content in place. Entries `rewrite` returns `None` for
    /// are left alone; links are never visited, since their content belongs to
    /// the file they point at.
    pub fn rewrite_content(&mut self, rewrite: impl Fn(&str, &[u8]) -> Option<Vec<u8>>) {
        for (relative, entry) in &mut self.entries {
            if let Entry::Content(contents) = entry {
                if let Some(rewritten) = rewrite(relative, contents) {
                    *contents = rewritten;
                }
            }
        }
    }

    /// Whether any staged path ends with `suffix`.
    pub fn any_path_ends_with(&self, suffix: &str) -> bool {
        self.entries.keys().any(|path| path.ends_with(suffix))
    }

    /// Remove what the previous run staged and this one does not, then
    /// materialize every entry that differs from what is already on disk.
    pub fn install(self) {
        let previous = self.previous();

        // Write-ahead. Record the union of what the last run staged and what
        // this one will *before* touching the tree, so the manifest is always
        // a superset of the overlay-owned files on disk. Recording only at the
        // end would strand anything created before an interrupted run: absent
        // from the manifest, a later run could never recognize it as stale.
        // Over-recording is harmless — a path listed but not present is simply
        // skipped when it is removed.
        self.write_manifest(
            previous
                .iter()
                .map(String::as_str)
                .chain(self.entries.keys().map(String::as_str)),
        );

        for stale in previous
            .iter()
            .filter(|path| !self.entries.contains_key(path.as_str()))
        {
            let path = self.root.join(stale);
            if fs::symlink_metadata(&path).is_ok() {
                fs::remove_file(&path).unwrap_or_else(|error| {
                    panic!("failed to remove stale overlay {}: {error}", path.display())
                });
            }
            prune_empty_parents(&self.root, &path);
        }

        for (relative, entry) in &self.entries {
            let path = self.root.join(relative);
            if entry.matches(&path) {
                continue;
            }
            let parent = path
                .parent()
                .unwrap_or_else(|| unreachable!("an overlay path always has a parent"));
            fs::create_dir_all(parent)
                .unwrap_or_else(|error| panic!("failed to create {}: {error}", parent.display()));
            entry.materialize(&path);
        }

        self.write_manifest(self.entries.keys().map(String::as_str));
    }

    /// Paths the previous run recorded as staged.
    fn previous(&self) -> Vec<String> {
        match fs::read_to_string(&self.manifest) {
            Ok(listing) => listing
                .lines()
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect(),
            // No manifest is a first run. An unreadable one is not: treating it
            // as empty would forfeit stale removal and strand every file the
            // last run staged.
            Err(error) if error.kind() == ErrorKind::NotFound => Vec::new(),
            Err(error) => panic!(
                "failed to read overlay manifest {}: {error}",
                self.manifest.display()
            ),
        }
    }

    /// Record `paths` as this overlay's ownership, de-duplicated and sorted.
    fn write_manifest<'a>(&self, paths: impl Iterator<Item = &'a str>) {
        let listing = paths
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        write_if_changed(
            &self.manifest,
            format!("{}\n", listing.join("\n")).as_bytes(),
        );
    }

    fn insert(&mut self, relative: &Path, entry: Entry) {
        let key = relative
            .to_str()
            .unwrap_or_else(|| panic!("overlay path is not UTF-8: {}", relative.display()))
            .replace('\\', "/");
        let clash = self.entries.insert(key.clone(), entry);
        assert!(clash.is_none(), "two overlay entries claim `{key}`");
    }
}

impl Entry {
    /// Whether `path` already holds exactly this entry.
    fn matches(&self, path: &Path) -> bool {
        match self {
            // A copy fallback is as correct as a link, so accept either shape
            // as long as the bytes agree.
            Self::Link(source) => {
                fs::read_link(path).is_ok_and(|target| target == *source)
                    || matches_content_of(path, source)
            }
            Self::Content(contents) => {
                fs::symlink_metadata(path).is_ok_and(|meta| meta.is_file())
                    && fs::read(path).is_ok_and(|found| found == *contents)
            }
        }
    }

    fn materialize(&self, path: &Path) {
        // Whatever is there is the wrong shape or the wrong bytes; a symlink in
        // particular cannot be overwritten in place.
        if fs::symlink_metadata(path).is_ok() {
            let _ = fs::remove_file(path);
        }
        match self {
            Self::Link(source) => {
                #[cfg(unix)]
                let linked = std::os::unix::fs::symlink(source, path);
                #[cfg(windows)]
                let linked = std::os::windows::fs::symlink_file(source, path);
                if linked.is_err() {
                    fs::copy(source, path).unwrap_or_else(|error| {
                        panic!(
                            "failed to link or copy {} to {}: {error}",
                            source.display(),
                            path.display()
                        )
                    });
                }
            }
            Self::Content(contents) => fs::write(path, contents)
                .unwrap_or_else(|error| panic!("failed to write {}: {error}", path.display())),
        }
    }
}

/// Whether `path` is a regular file holding the same bytes as `source`.
fn matches_content_of(path: &Path, source: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| meta.is_file())
        && match (fs::read(path), fs::read(source)) {
            (Ok(found), Ok(expected)) => found == expected,
            _ => false,
        }
}

/// Every file below `dir`, paired with its path relative to `dir`. An absent
/// `dir` yields nothing.
fn tree_files(dir: &Path) -> Vec<(PathBuf, PathBuf)> {
    let mut found = Vec::new();
    collect_tree_files(dir, Path::new(""), &mut found);
    found.sort();
    found
}

fn collect_tree_files(dir: &Path, prefix: &Path, out: &mut Vec<(PathBuf, PathBuf)>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        // A generator may stage an overlay whose source does not exist. Any
        // other failure must not be read as an empty tree: `install` treats
        // whatever it does not see as stale and deletes it, so a swallowed
        // error would silently remove the fixture's ported tests.
        Err(error) if error.kind() == ErrorKind::NotFound => return,
        Err(error) => panic!("failed to read overlay source {}: {error}", dir.display()),
    };
    for entry in entries {
        let entry = entry.unwrap_or_else(|error| {
            panic!("failed to read an entry of {}: {error}", dir.display())
        });
        let path = entry.path();
        let relative = prefix.join(entry.file_name());
        if path.is_dir() {
            collect_tree_files(&path, &relative, out);
        } else if path.is_file() {
            out.push((relative, path));
        }
    }
}

/// Remove directories left empty by a stale removal, stopping at `root`.
fn prune_empty_parents(root: &Path, removed: &Path) {
    let mut parent = removed.parent();
    while let Some(dir) = parent {
        if dir == root || !dir.starts_with(root) {
            return;
        }
        if fs::remove_dir(dir).is_err() {
            return;
        }
        parent = dir.parent();
    }
}

/// Write `contents` only when they differ from what is already there, so an
/// unchanged regeneration leaves the file's mtime alone.
pub fn write_if_changed(path: &Path, contents: &[u8]) {
    if fs::read(path).is_ok_and(|found| found == contents) {
        return;
    }
    fs::write(path, contents)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", path.display()));
}
