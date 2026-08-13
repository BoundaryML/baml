//! Build-script-side infrastructure for the sdk-test crates under
//! `sdk_tests/crates/`. Each generator crate (e.g.
//! `crates/python_pydantic2/`) fans out over every fixture under
//! `sdk_tests/fixtures/<fixture>/baml_src/` and produces one
//! `crates/<generator>/<fixture>/generated/` tree per fixture.
//!
//! This crate is generator-agnostic: it discovers fixtures, loads
//! `.baml` source into a [`ProjectDatabase`], gates on diagnostics,
//! symlinks/copies `customizable/` overlays into the generated tree,
//! and emits a per-fixture `#[test]` scaffold to `OUT_DIR`. The
//! scaffold is a sequence of macro / function invocations against
//! `::sdk_test_harness_runner::*` — every emitted `#[test]` body,
//! including the shared `build_diagnostics::no_build_failures`,
//! lives in the sibling `sdk_test_harness_runner` crate.
//!
//! Generator-specific entry points live in submodules like
//! [`python_pydantic2`], [`typescript`], and [`typescript_web`].
//!
//! Layout the helpers assume:
//!
//! ```text
//! sdk_tests/
//! ├── fixtures/<fixture>/baml_src/                # .baml only
//! └── crates/<generator>/<fixture>/
//!     ├── customizable/                           # *.py / *.ts, tracked
//!     └── generated/                              # build output, gitignored
//! ```

use std::{
    fmt::{self, Display},
    fs,
    path::{Path, PathBuf},
};

use baml_codegen_types::{
    GeneratedOutputFile, OutputOptions, OutputProvenance, SymbolPool, VcsPolicy,
    write_generated_output,
};
use baml_db::baml_compiler_diagnostics::Severity;
use baml_project::ProjectDatabase;

pub mod cpp;
pub mod csharp;
pub mod go;
pub mod java;
pub mod python_pydantic2;
pub mod rust;
pub mod swift;
pub mod typescript;
pub mod typescript_web;

/// Emit one Cargo build-script line. Cargo consumes directives and
/// warnings from stdout, so this intentionally writes there.
#[allow(clippy::print_stdout)]
pub(crate) fn emit_cargo_line(args: fmt::Arguments<'_>) {
    println!("{args}");
}

/// Build-script-side soft-failure recorder. The two generator
/// `run_all` entry points use this to capture env-dependent failures
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
pub(crate) fn write_codegen_output<C>(
    output_directory: &Path,
    output: impl IntoIterator<Item = (PathBuf, C)>,
    fixture: &str,
    diagnostics: &mut BuildDiagnostics,
) where
    C: AsRef<[u8]>,
{
    let files = output
        .into_iter()
        .map(|(relative_path, contents)| {
            GeneratedOutputFile::new(relative_path, contents.as_ref().to_vec())
        })
        .collect();
    if let Err(error) = write_generated_output(output_directory, files, &harness_output_options()) {
        diagnostics.record("codegen_write", fixture, error);
    }
}

/// Provenance for harness-generated fixtures.
///
/// The freshness fields exist for real projects, where a later invocation
/// decides whether to regenerate. Fixtures are rebuilt unconditionally by the
/// harness, so a fixed marker keeps their output byte-stable across runs —
/// which the C# determinism check depends on.
pub(crate) fn harness_output_options() -> OutputOptions {
    OutputOptions {
        provenance: OutputProvenance {
            input_fingerprint: "sdk-test-harness".to_string(),
            toolchain_version: baml_version::CANONICAL_VERSION.to_string(),
            generator_name: "sdk-test-harness".to_string(),
        },
        vcs: VcsPolicy::Ignore,
    }
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

/// Enumerate every `<fixtures_root>/<name>/` that contains a
/// `baml_src/` subdirectory. Sorted so codegen output ordering is
/// stable across builds.
pub fn discover_fixtures(fixtures_root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let entries = fs::read_dir(fixtures_root).unwrap_or_else(|e| {
        panic!(
            "failed to read fixtures root {}: {e}",
            fixtures_root.display()
        )
    });
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if !path.join("baml_src").is_dir() {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            // Port checkpoint: this fixture exercises the host-reflection
            // extraction contract, which is restored in the later compiler
            // slices. Keep it out of eager SDK build-script codegen until then.
            if name == "host_reflect" {
                continue;
            }
            out.push(name.to_string());
        }
    }
    out.sort();
    out
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
    db.set_project_root(&canonical);
    let baml_files = baml_workspace::discover_baml_files(&canonical);
    assert!(
        !baml_files.is_empty(),
        "fixture `{fixture}`: no .baml files under {}",
        canonical.display()
    );
    for file_path in &baml_files {
        let content = fs::read_to_string(file_path)
            .unwrap_or_else(|_| panic!("failed to read {}", file_path.display()));
        db.add_or_update_file(file_path, &content);
    }

    let source_files = db.get_source_files();
    let diagnostics = baml_project::collect_diagnostics(&db);
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

    let pool = baml_project::build_symbol_pool(&db);
    let program = db
        .get_bytecode()
        .unwrap_or_else(|e| panic!("fixture `{fixture}`: bytecode compilation failed: {e:?}"));
    let baml_bytecode = borsh::to_vec(&program)
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

/// Recursively copy `customizable_dir` into `dst_dir`. Used by the Go and
/// TypeScript targets: symlinks would force every parallel
/// test process to either set `NODE_OPTIONS=--preserve-symlinks`
/// (which breaks the pnpm CLI, itself a symlinked node script) or
/// let node follow the symlink and resolve `node_modules` from
/// `customizable/` (which has none). Copying sidesteps both;
/// build.rs's `cargo:rerun-if-changed=` watch on `customizable/`
/// re-stages on edit.
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

/// Symlink every file in `customizable_dir` into `dst_dir`. On
/// Windows `symlink_file` requires Developer Mode or admin, so a
/// failed symlink falls back to `fs::copy` — the cost is that local
/// edits to `customizable/` aren't picked up without re-running
/// build.rs (which `cargo:rerun-if-changed=` handles).
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

#[cfg(test)]
mod csharp_abi_probe_tests {
    use std::{env, fs, path::PathBuf};

    #[test]
    #[ignore = "writes bytecode to the path requested by the C# product workflow"]
    fn emit_function_calls_bytecode() {
        let output = PathBuf::from(
            env::var("BAML_CSHARP_ABI_PROBE_BYTECODE")
                .expect("BAML_CSHARP_ABI_PROBE_BYTECODE is not set"),
        );
        assert!(
            output.is_absolute(),
            "C# ABI probe bytecode path must be absolute"
        );
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("harness_setup is not inside sdk_tests")
            .join("fixtures");
        let fixture = super::load_fixture(&fixtures, "function_calls");
        fs::write(&output, fixture.baml_bytecode).unwrap_or_else(|error| {
            panic!(
                "failed to write C# ABI probe bytecode to {}: {error}",
                output.display()
            )
        });
    }
}
