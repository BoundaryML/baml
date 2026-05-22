//! Test-side runtime for the `sdk_tests/crates/<generator>/` crates.
//! Wired in as `[dev-dependencies]` while the sibling
//! `sdk_test_harness_setup` crate is wired in as
//! `[build-dependencies]`.
//!
//! The scaffold emitted by
//! `sdk_test_harness_setup::<generator>::run_all` is a sequence of
//! macro / function invocations against this crate:
//!
//! ```text
//! // OUT_DIR/<generator>_tests.rs (emitted by sdk_test_harness_setup)
//! ::sdk_test_harness_runner::build_diagnostics!();          // or: !(ignore = "…")
//!
//! mod docstrings_etc {
//!     #[test] fn ruff()    { ::sdk_test_harness_runner::run_test_cmd(…); }
//!     #[test] fn pyright() { ::sdk_test_harness_runner::run_test_cmd(…); }
//!     // …
//! }
//! ```
//!
//! Each per-generator `<generator>::test_suite!` macro
//! (`include!`s the scaffold) lives below, alongside
//! [`build_diagnostics!`] (the shared diagnostics test) and
//! [`run_test_cmd`] (the toolchain-command runner).

use std::{
    env,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    process::{Command, Output},
};

/// Test-side helper. Runs `cmd` inside
/// `<CARGO_MANIFEST_DIR>/<fixture>/generated/`, panicking on
/// non-zero exit. Cargo sets `CARGO_MANIFEST_DIR` for the test
/// binary at runtime so the helper resolves the right generator
/// crate without the macro having to thread it through.
///
/// `cache_subdir` names the per-toolchain subdirectory under
/// `<workspace>/target/` used for the tool's cache (`uv-cache` for
/// uv, `pnpm-store` for pnpm). `cache_env_var` is the environment
/// variable the tool reads (`UV_CACHE_DIR`,
/// `npm_config_store_dir`, …).
///
/// If `uv` is managed by mise but its shim isn't on PATH, the
/// helper falls back to `mise which uv` before giving up.
pub fn run_test_cmd(fixture: &str, cmd: &str, cache_subdir: &str, cache_env_var: &str) {
    run_test_cmd_with_env(fixture, cmd, cache_subdir, cache_env_var, &[]);
}

/// Same as [`run_test_cmd`] but threads additional environment
/// variables into the child process.
pub fn run_test_cmd_with_env(
    fixture: &str,
    cmd: &str,
    cache_subdir: &str,
    cache_env_var: &str,
    extra_env: &[(&str, &str)],
) {
    let manifest = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set; run via `cargo test`"),
    );
    let dir = manifest.join(fixture).join("generated");
    assert!(
        dir.exists(),
        "{fixture}/generated/ not found at {} — did build.rs run?",
        dir.display()
    );

    // sdk-test crates live at `<workspace>/sdk_tests/crates/<generator>/`,
    // so the workspace root is the 3rd ancestor of the manifest dir.
    let workspace_root = manifest
        .ancestors()
        .nth(3)
        .expect("sdk-test crate not at <workspace>/sdk_tests/crates/<generator>/");
    let cache_dir = workspace_root.join("target").join(cache_subdir);

    assert!(
        !cmd.contains('"') && !cmd.contains('\''),
        "run_test_cmd does not handle quoted args: `{cmd}`"
    );
    let mut words = cmd.split_whitespace();
    let prog = words.next().unwrap_or_else(|| panic!("empty command"));
    let args: Vec<&str> = words.collect();

    let output = run_test_process(prog, &args, &dir, &cache_dir, cache_env_var, extra_env)
        .unwrap_or_else(|e| panic!("failed to spawn `{cmd}` for fixture `{fixture}`: {e}"));
    assert!(
        output.status.success(),
        "fixture `{fixture}` `{cmd}` failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run_test_process(
    prog: &str,
    args: &[&str],
    dir: &Path,
    cache_dir: &Path,
    cache_env_var: &str,
    extra_env: &[(&str, &str)],
) -> io::Result<Output> {
    let mut command = Command::new(prog);
    command
        .args(args)
        .current_dir(dir)
        .env(cache_env_var, cache_dir);
    for (k, v) in extra_env {
        command.env(k, v);
    }
    let output = command.output();

    match output {
        Err(err) if err.kind() == ErrorKind::NotFound && prog == "uv" => {
            let uv = resolve_mise_uv()?;
            let mut fallback = Command::new(uv);
            fallback
                .args(args)
                .current_dir(dir)
                .env(cache_env_var, cache_dir);
            for (k, v) in extra_env {
                fallback.env(k, v);
            }
            fallback.output()
        }
        other => other,
    }
}

fn resolve_mise_uv() -> io::Result<PathBuf> {
    let output = Command::new("mise").args(["which", "uv"]).output()?;
    if !output.status.success() {
        return Err(io::Error::new(
            ErrorKind::NotFound,
            format!(
                "`uv` is not on PATH and `mise which uv` failed:\n{}.",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if path.is_empty() {
        return Err(io::Error::new(
            ErrorKind::NotFound,
            "`uv` is not on PATH and `mise which uv` returned an empty path",
        ));
    }

    Ok(PathBuf::from(path))
}

/// Read `$OUT_DIR/build_diagnostics.txt` (the file
/// [`sdk_test_harness_setup::BuildDiagnostics::finalize`] writes) and panic
/// with the records if non-empty. Called from inside the
/// `mod build_diagnostics { #[test] fn no_build_failures }` block
/// the [`build_diagnostics!`] macro expands to — `out_dir` is
/// `env!("OUT_DIR")` resolved at the macro's call site, so it
/// points at the *generator crate's* OUT_DIR (where
/// `sdk_test_harness_setup` wrote the file).
#[doc(hidden)]
pub fn __check_build_diagnostics(out_dir: &str) {
    let path = format!("{out_dir}/build_diagnostics.txt");
    let contents = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{path}: {e} — did build.rs run?"));
    if !contents.trim().is_empty() {
        let count = contents.matches("\n---\n").count() + 1;
        panic!("sdk-test build.rs recorded {count} diagnostic record(s):\n\n{contents}");
    }
}

/// Emit the shared `mod build_diagnostics { #[test] fn
/// no_build_failures }` test that reads
/// `$OUT_DIR/build_diagnostics.txt` and fails with the records.
/// `sdk_test_harness_setup`'s scaffold emitter stamps one invocation per
/// generator scaffold:
///
/// ```text
/// // Default — fail loudly on any recorded diagnostic.
/// ::sdk_test_harness_runner::build_diagnostics!();
///
/// // Skip — every fixture records a codegen failure (nodejs_typescript
/// // while codegen_nodejs is a stub).
/// ::sdk_test_harness_runner::build_diagnostics!(ignore = "codegen_nodejs is a stub");
/// ```
///
/// `env!("OUT_DIR")` inside the expansion resolves at the macro's
/// call site (i.e. inside the generator crate's test compilation),
/// so the path lines up with where `sdk_test_harness_setup` wrote the file.
#[macro_export]
macro_rules! build_diagnostics {
    () => {
        mod build_diagnostics {
            #[test]
            fn no_build_failures() {
                $crate::__check_build_diagnostics(env!("OUT_DIR"));
            }
        }
    };
    (ignore = $reason:literal) => {
        mod build_diagnostics {
            #[test]
            #[ignore = $reason]
            fn no_build_failures() {
                $crate::__check_build_diagnostics(env!("OUT_DIR"));
            }
        }
    };
}

/// Python + pydantic2 generator's test-side glue. Invoked from
/// `crates/python_pydantic2/src/lib.rs` as
/// `sdk_test_harness_runner::python_pydantic2::test_suite!()`.
pub mod python_pydantic2 {
    /// `include!`s `OUT_DIR/python_pydantic2_tests.rs` — the
    /// per-fixture scaffold emitted by
    /// `sdk_test_harness_setup::python_pydantic2::run_all`.
    #[macro_export]
    macro_rules! python_pydantic2_test_suite {
        () => {
            include!(concat!(env!("OUT_DIR"), "/python_pydantic2_tests.rs"));
        };
    }

    pub use crate::python_pydantic2_test_suite as test_suite;
}

/// Node.js + TypeScript generator's test-side glue. Invoked from
/// `crates/nodejs_typescript/src/lib.rs` as
/// `sdk_test_harness_runner::nodejs_typescript::test_suite!()`.
pub mod nodejs_typescript {
    /// `include!`s `OUT_DIR/nodejs_typescript_tests.rs` — the
    /// per-fixture scaffold emitted by
    /// `sdk_test_harness_setup::nodejs_typescript::run_all`.
    #[macro_export]
    macro_rules! nodejs_typescript_test_suite {
        () => {
            include!(concat!(env!("OUT_DIR"), "/nodejs_typescript_tests.rs"));
        };
    }

    pub use crate::nodejs_typescript_test_suite as test_suite;
}
