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
//! Two things drive those entry points, and which one a generator uses is
//! visible in whether `crates/<generator>/` still has a `build.rs`:
//!
//! - the `sdk_test_codegen` binary (`src/main.rs`), run from a
//!   generator crate's `setup.sh` before its tests; and
//! - that `build.rs`, which additionally emits a per-fixture `#[test]`
//!   scaffold to `OUT_DIR` and records soft failures through
//!   [`BuildDiagnostics`].
//!
//! The build-script path is being retired: it makes every `cargo check` build
//! this crate's whole compiler and sdkgen closure. Migrated generators declare
//! their tests in source instead, via
//! `sdk_test_harness_runner::<generator>::test_suite!`.
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
    fmt::{self, Display},
    fs,
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

/// Emit one Cargo build-script line. Cargo consumes directives and
/// warnings from stdout, so this intentionally writes there.
#[allow(clippy::print_stdout)]
pub(crate) fn emit_cargo_line(args: fmt::Arguments<'_>) {
    println!("{args}");
}

/// Build-script-side soft-failure recorder, used by the generators still
/// driven by a `build.rs` to capture env-dependent failures
/// (missing `uv`/`pnpm`, codegen panics, `uv sync` / `pnpm install`
/// non-zero exit, codegen file write failures) without aborting the
/// build — so `cargo doc` / `cargo check` succeed on machines that
/// don't have the SDK toolchains installed.
///
/// Records flow into `$OUT_DIR/build_diagnostics.txt`, which the
/// emitted `mod build_diagnostics` test
/// (`sdk_test_harness_runner::build_diagnostics!`) reads at `cargo test`
/// time. Always call [`Self::finalize`] at the end of `run_all` —
/// it writes the file unconditionally (zero-length on success) so a
/// missing file means "build.rs did not run", which the test flags
/// distinctly from "build.rs ran cleanly".
pub struct BuildDiagnostics {
    out_dir: PathBuf,
    records: Vec<String>,
}

impl BuildDiagnostics {
    pub fn new(out_dir: &Path) -> Self {
        Self {
            out_dir: out_dir.to_path_buf(),
            records: Vec::new(),
        }
    }

    /// Record a soft failure. `stage` is one of the documented values
    /// (`codegen`, `uv_sync`, `pnpm_install`, `pyproject_write`,
    /// `package_json_write`, `symlink_customizable`,
    /// `copy_customizable`, `codegen_write`); `fixture` is the
    /// fixture directory name. Also emits a `cargo:warning=` line so
    /// `cargo build` users see an inline pointer to the diagnostics
    /// test without having to run it.
    pub fn record(&mut self, stage: &str, fixture: &str, msg: impl Display) {
        self.records
            .push(format!("stage: {stage}\nfixture: {fixture}\n{msg}"));
        emit_cargo_line(format_args!(
            "cargo:warning=sdk-test build recorded a `{stage}` failure for fixture `{fixture}` — see `cargo test build_diagnostics`"
        ));
    }

    /// Write `$OUT_DIR/build_diagnostics.txt`. Always called from
    /// `run_all`; writes zero bytes when there are no records so the
    /// downstream test can tell "ran cleanly" apart from "build.rs
    /// didn't run".
    pub fn finalize(self) {
        let path = self.out_dir.join("build_diagnostics.txt");
        let body = self.records.join("\n---\n");
        fs::write(&path, body).unwrap_or_else(|e| {
            panic!(
                "failed to write build_diagnostics.txt at {}: {e}",
                path.display()
            )
        });
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

/// [`write_codegen_output`] for generators still driven by a build script,
/// where an install failure is recorded rather than raised so `cargo check`
/// stays green. Delete alongside the last `crates/*/build.rs`.
pub(crate) fn write_codegen_output_recording<C>(
    output_directory: &Path,
    output: impl IntoIterator<Item = (PathBuf, C)>,
    fixture: &str,
    diagnostics: &mut BuildDiagnostics,
) where
    C: AsRef<[u8]>,
{
    if let Err(error) = install(output_directory, output) {
        diagnostics.record("codegen_write", fixture, error);
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

/// Resolve the workspace-root-relative path to `sdk_tests/fixtures/`
/// from a generator crate's `CARGO_MANIFEST_DIR`. Generator crates
/// live at `<workspace>/sdk_tests/crates/<generator>/`, so the
/// fixtures root is `manifest.parent().parent().join("fixtures")`.
pub fn fixtures_root_from_manifest(manifest_dir: &Path) -> PathBuf {
    manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("crate not at <workspace>/sdk_tests/crates/<generator>/")
        .join("fixtures")
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

/// Emit `cargo:rerun-if-changed=` for every file under `dir`,
/// recursively. Safe to call on a path that doesn't exist (no-op).
pub fn watch_dir(dir: &Path) {
    for path in walk_files(dir) {
        emit_cargo_line(format_args!("cargo:rerun-if-changed={}", path.display()));
    }
}

/// Recursively collect every file under `dir`. Returns an empty
/// `Vec` if `dir` is missing — callers can use this without
/// pre-checking existence.
pub fn walk_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                out.push(path);
            } else if path.is_dir() {
                out.extend(walk_files(&path));
            }
        }
    }
    out
}
