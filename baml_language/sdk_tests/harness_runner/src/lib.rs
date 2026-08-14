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
//! ::sdk_test_harness_runner::setup_guard!("SDK_TEST_…_SETUP"); // asserts setup.sh ran
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
//! [`build_diagnostics!`] (the shared diagnostics test),
//! [`setup_guard!`] (asserts the crate's setup.sh ran this run), and
//! [`run_test_cmd`] (the toolchain-command runner).

use std::{
    env, fs,
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
/// helper falls back to `mise which uv` before giving up. On Windows,
/// `pnpm` is commonly exposed as `pnpm.cmd` and `gradle` as
/// `gradle.bat` (there is no bare `gradle.exe`); Rust's process
/// launcher does not consistently apply shell-style `PATHEXT`
/// expansion when asked to spawn `pnpm` / `gradle`, so the helper
/// retries the explicit shim (`pnpm.cmd` / `gradle.bat`).
pub fn run_test_cmd(fixture: &str, cmd: &str, cache_subdir: &str, cache_env_var: &str) {
    run_test_cmd_with_env(fixture, cmd, cache_subdir, cache_env_var, &[]);
}

/// Same as [`run_test_cmd`], but treats the listed process exit codes as
/// successful outcomes. This lets a harness model tool-specific non-error
/// statuses explicitly—for example, pytest uses exit code 5 when collection
/// succeeds but finds no tests.
pub fn run_test_cmd_allowing_exit_codes(
    fixture: &str,
    cmd: &str,
    cache_subdir: &str,
    cache_env_var: &str,
    allowed_exit_codes: &[i32],
) {
    run_test_cmd_with_env_allowing_exit_codes(
        fixture,
        cmd,
        cache_subdir,
        cache_env_var,
        &[],
        allowed_exit_codes,
    );
}

/// Run the Go toolchain against one generated fixture. Prefer the repository's
/// mise-managed Go binary so a globally installed `go` cannot accidentally use
/// a different GOROOT than the pinned compiler.
pub fn run_go_test(fixture: &str) {
    let manifest = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set; run via `cargo test`"),
    );
    let dir = manifest.join(fixture).join("generated");
    let workspace_root = workspace_root_from_manifest(&manifest);
    let go = resolve_mise_tool("go").unwrap_or_else(|_| PathBuf::from("go"));

    let output = Command::new(&go)
        .args(["test", "./..."])
        .current_dir(&dir)
        .env_remove("GOROOT")
        .env("CGO_ENABLED", "1")
        .env("GOCACHE", workspace_root.join("target/go-build-cache"))
        .env("GOMODCACHE", workspace_root.join("target/go-mod-cache"))
        // Go makes module-cache directories read-only by default, and
        // deleting a file needs write permission on its PARENT directory —
        // so `cargo clean` aborts on the first file under `target/
        // go-mod-cache` with "Permission denied", leaving the whole Rust
        // target tree behind. `-modcacherw` keeps those directories
        // writable, which is exactly what this flag exists for.
        .env("GOFLAGS", "-modcacherw")
        .env("BAML_RUNTIME_PATH", go_runtime_library(workspace_root))
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "failed to spawn `{}` for fixture `{fixture}`: {e}",
                go.display()
            )
        });
    assert!(
        output.status.success(),
        "fixture `{fixture}` `go test ./...` failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn go_runtime_library(workspace_root: &Path) -> PathBuf {
    let filename = if cfg!(target_os = "macos") {
        "libbridge_cffi.dylib"
    } else if cfg!(target_os = "windows") {
        "bridge_cffi.dll"
    } else {
        "libbridge_cffi.so"
    };
    workspace_root.join("target").join("debug").join(filename)
}

/// Run a toolchain command from a workspace-relative directory. Used for
/// package-level checks that do not belong to a generated fixture app.
pub fn run_workspace_cmd(relative_dir: &str, cmd: &str, cache_subdir: &str, cache_env_var: &str) {
    let manifest = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set; run via `cargo test`"),
    );
    let workspace_root = workspace_root_from_manifest(&manifest);
    let dir = workspace_root.join(relative_dir);
    assert!(
        dir.exists(),
        "workspace command dir not found at {}",
        dir.display()
    );

    let cache_dir = workspace_root.join("target").join(cache_subdir);
    assert!(
        !cmd.contains('"') && !cmd.contains('\''),
        "run_workspace_cmd does not handle quoted args: `{cmd}`"
    );
    let mut words = cmd.split_whitespace();
    let prog = words.next().unwrap_or_else(|| panic!("empty command"));
    let args: Vec<&str> = words.collect();

    let output = run_test_process(prog, &args, &dir, &cache_dir, cache_env_var, &[])
        .unwrap_or_else(|e| panic!("failed to spawn `{cmd}` in `{relative_dir}`: {e}"));
    assert!(
        output.status.success(),
        "workspace command `{cmd}` in `{relative_dir}` failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Java-fixture variant of [`run_test_cmd`]: injects
/// `BAML_JAVA_BRIDGE_LIB` pointing at the workspace-built
/// `bridge_java` cdylib (produced by `crates/java/setup.sh`), so the
/// generated `Baml` anchor can `System.load` the engine during tests.
pub fn run_java_test_cmd(fixture: &str, cmd: &str, cache_subdir: &str, cache_env_var: &str) {
    let manifest = std::path::PathBuf::from(
        env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set; run via `cargo test`"),
    );
    let lib_name = if cfg!(target_os = "windows") {
        "bridge_java.dll"
    } else if cfg!(target_os = "macos") {
        "libbridge_java.dylib"
    } else {
        "libbridge_java.so"
    };
    let lib = workspace_root_from_manifest(&manifest)
        .join("target")
        .join("debug")
        .join(lib_name);
    let lib_str = lib.to_string_lossy().into_owned();
    run_test_cmd_with_env(
        fixture,
        cmd,
        cache_subdir,
        cache_env_var,
        &[("BAML_JAVA_BRIDGE_LIB", lib_str.as_str())],
    );
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
    run_test_cmd_with_env_allowing_exit_codes(
        fixture,
        cmd,
        cache_subdir,
        cache_env_var,
        extra_env,
        &[],
    );
}

fn run_test_cmd_with_env_allowing_exit_codes(
    fixture: &str,
    cmd: &str,
    cache_subdir: &str,
    cache_env_var: &str,
    extra_env: &[(&str, &str)],
    allowed_exit_codes: &[i32],
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
    let workspace_root = workspace_root_from_manifest(&manifest);
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
    let accepted = output.status.success()
        || output
            .status
            .code()
            .is_some_and(|code| allowed_exit_codes.contains(&code));
    assert!(
        accepted,
        "fixture `{fixture}` `{cmd}` failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn workspace_root_from_manifest(manifest: &Path) -> &Path {
    manifest
        .ancestors()
        .nth(3)
        .expect("sdk-test crate not at <workspace>/sdk_tests/crates/<generator>/")
}

/// Assert the generated TypeScript Node SDK fixture is native ESM output, not
/// CommonJS masquerading under a `"type": "module"` package.
pub fn assert_typescript_node_generated_esm(fixture: &str, runtime_dir: &str) {
    let manifest = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set; run via `cargo test`"),
    );
    let generated_root = manifest.join(fixture).join("generated");
    let generated = generated_root.join(runtime_dir);
    assert!(
        generated.exists(),
        "{fixture}/generated/ not found at {} - did build.rs run?",
        generated.display()
    );

    let package_json = fs::read_to_string(generated_root.join("package.json"))
        .unwrap_or_else(|e| panic!("{fixture}: read generated/package.json: {e}"));
    assert!(
        package_json.contains(r#""type": "module""#),
        "{fixture}: generated package.json must mark the fixture as ESM"
    );

    let tsconfig = fs::read_to_string(generated_root.join("tsconfig.node.json"))
        .unwrap_or_else(|e| panic!("{fixture}: read generated/tsconfig.node.json: {e}"));
    assert!(
        tsconfig.contains(r#""module": "nodenext""#)
            && tsconfig.contains(r#""moduleResolution": "nodenext""#),
        "{fixture}: generated tsconfig.json must compile in NodeNext ESM mode"
    );

    let sdk_root = generated.join("baml_sdk");
    let mut saw_esm_syntax = false;
    for path in collect_ts_files(&sdk_root) {
        let rel = path
            .strip_prefix(&generated)
            .unwrap_or(&path)
            .display()
            .to_string();
        let contents =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("{fixture}: read {rel}: {e}"));

        assert!(
            !contents.contains("module.exports")
                && !contents.contains("exports.")
                && !contents.contains("require("),
            "{fixture}: generated {rel} contains CommonJS syntax"
        );

        if contents.contains("import ") || contents.contains("export ") {
            saw_esm_syntax = true;
        }

        for (line_no, line) in contents.lines().enumerate() {
            if let Some(specifier) = import_from_specifier(line) {
                assert!(
                    !specifier.starts_with('.') || specifier.ends_with(".js"),
                    "{fixture}: generated {rel}:{} has extensionless relative import `{specifier}`",
                    line_no + 1
                );
            }
        }
    }

    assert!(
        saw_esm_syntax,
        "{fixture}: generated baml_sdk did not contain ESM import/export syntax"
    );
}

/// Assert that the browser generator emits ESM with browser-oriented module
/// resolution and dispatches exclusively through the web bridge package.
pub fn assert_typescript_web_generated_esm(fixture: &str, runtime_dir: &str) {
    let manifest = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set; run via `cargo test`"),
    );
    let generated_root = manifest.join(fixture).join("generated");
    let generated = generated_root.join(runtime_dir);
    assert!(
        generated.exists(),
        "{fixture}/generated/ not found at {} - did build.rs run?",
        generated.display()
    );
    let package_json = fs::read_to_string(generated_root.join("package.json"))
        .unwrap_or_else(|e| panic!("{fixture}: read generated/package.json: {e}"));
    assert!(
        package_json.contains(r#""type": "module""#),
        "{fixture}: generated package.json must mark the fixture as ESM"
    );
    let tsconfig = fs::read_to_string(generated_root.join(format!("tsconfig.{runtime_dir}.json")))
        .unwrap_or_else(|e| panic!("{fixture}: read generated/tsconfig.{runtime_dir}.json: {e}"));
    assert!(
        tsconfig.contains(r#""module": "ESNext""#)
            && tsconfig.contains(r#""moduleResolution": "Bundler""#),
        "{fixture}: generated tsconfig.json must use browser ESM resolution"
    );
    let mut saw_web_bridge = false;
    for path in collect_ts_files(&generated.join("baml_sdk")) {
        let contents = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{fixture}: read {}: {e}", path.display()));
        assert!(
            !contents.contains("@boundaryml/baml-bridge\""),
            "{fixture}: generated {} still dispatches through the Node bridge",
            path.display()
        );
        if contents.contains("@boundaryml/baml-bridge-web") {
            saw_web_bridge = true;
        }
        for (line_no, line) in contents.lines().enumerate() {
            if let Some(specifier) = import_from_specifier(line) {
                assert!(
                    !specifier.starts_with('.') || specifier.ends_with(".js"),
                    "{fixture}: generated {}:{} has extensionless relative import `{specifier}`",
                    path.display(),
                    line_no + 1
                );
            }
        }
    }
    assert!(
        saw_web_bridge,
        "{fixture}: generated SDK never imports @boundaryml/baml-bridge-web"
    );
    let inlined_bytecode = fs::read_to_string(generated.join("baml_sdk/_inlinedbaml.ts"))
        .unwrap_or_else(|e| panic!("{fixture}: read generated bytecode module: {e}"));
    // The bytecode is emitted as base64 lines joined into one string; an empty
    // program would still emit the export, so non-emptiness means at least one
    // populated payload line.
    let has_bytecode_payload = inlined_bytecode
        .lines()
        .any(|line| line.starts_with("  \"") && line.ends_with("\",") && line.len() > 5);
    assert!(
        inlined_bytecode.contains("export const BYTECODE = decodeBytecode(BYTECODE_BASE64);")
            && has_bytecode_payload,
        "{fixture}: generated SDK must contain non-empty BAML bytecode"
    );
    let root = fs::read_to_string(generated.join("baml_sdk/index.ts"))
        .unwrap_or_else(|e| panic!("{fixture}: read generated SDK root: {e}"));
    assert!(
        root.contains(
            "initializeRuntimeFromBytecode(_inlinedbaml.BYTECODE, _inlinedbaml.BAML_TOML)"
        ),
        "{fixture}: generated SDK root must initialize the web runtime from emitted bytecode and metadata"
    );
}

fn collect_ts_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_ts_files_inner(root, &mut files);
    files
}

fn collect_ts_files_inner(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display())) {
        let path = entry
            .unwrap_or_else(|e| panic!("read {} entry: {e}", dir.display()))
            .path();
        if path.is_dir() {
            collect_ts_files_inner(&path, files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("ts") {
            files.push(path);
        }
    }
}

fn import_from_specifier(line: &str) -> Option<&str> {
    let from = line.find(" from ")?;
    let rest = line[from + " from ".len()..].trim_start();
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &rest[quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some(&rest[..end])
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
        #[cfg(windows)]
        Err(err) if err.kind() == ErrorKind::NotFound && prog == "pnpm" => {
            let mut fallback = Command::new("pnpm.cmd");
            fallback
                .args(args)
                .current_dir(dir)
                .env(cache_env_var, cache_dir);
            for (k, v) in extra_env {
                fallback.env(k, v);
            }
            fallback.output()
        }
        // Gradle ships as `gradle.bat` on Windows (no bare `gradle.exe`),
        // and Rust's launcher doesn't reliably apply PATHEXT (see the
        // `pnpm.cmd` note above), so retry the explicit batch launcher.
        #[cfg(windows)]
        Err(err) if err.kind() == ErrorKind::NotFound && prog == "gradle" => {
            let mut fallback = Command::new("gradle.bat");
            fallback
                .args(args)
                .current_dir(dir)
                .env(cache_env_var, cache_dir);
            for (k, v) in extra_env {
                fallback.env(k, v);
            }
            fallback.output()
        }
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
    resolve_mise_tool("uv")
}

fn resolve_mise_tool(tool: &str) -> io::Result<PathBuf> {
    let output = Command::new("mise").args(["which", tool]).output()?;
    if !output.status.success() {
        return Err(io::Error::new(
            ErrorKind::NotFound,
            format!(
                "`{tool}` is not on PATH and `mise which {tool}` failed:\n{}.",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if path.is_empty() {
        return Err(io::Error::new(
            ErrorKind::NotFound,
            format!("`{tool}` is not on PATH and `mise which {tool}` returned an empty path"),
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

/// Panic unless the per-generator setup script ran *this* test run.
///
/// Each `crates/<generator>/setup.sh` appends `<env_var>=1` to the
/// file at `$NEXTEST_ENV` (a nextest setup-script feature): nextest
/// then injects that var into the matched tests' processes for that
/// run only. So presence of the var is a per-run breadcrumb proving
/// the setup script executed — not a stale on-disk marker, and not
/// the weaker "are we under nextest at all" check (`NEXTEST=1` is set
/// regardless of which scripts ran).
///
/// Under plain `cargo test` the setup-script breadcrumb is unavailable, so the
/// guard is a no-op and the generated fixture tests report any real setup
/// problems themselves. Called from the `mod setup_guard { #[test] fn ran }`
/// block the [`setup_guard!`] macro expands to.
#[doc(hidden)]
pub fn __check_setup_ran(env_var: &str) {
    if env::var_os(env_var).is_some() || env::var_os("NEXTEST").is_none() {
        return;
    }

    panic!(
        "sdk-test setup script did not run for this test run \
         (env var `{env_var}` is unset).\n\n\
         These tests require their `crates/<generator>/setup.sh` (uv sync / \
         pnpm install + native build) to have run first, which sets `{env_var}` \
         via $NEXTEST_ENV.\n\n\
         Fix: run the tests with `cargo nextest run` — it fires setup.sh \
         automatically."
    );
}

/// Emit the `mod setup_guard { #[test] fn ran }` test that asserts
/// the per-generator setup script ran this test run (via
/// [`__check_setup_ran`]). `sdk_test_harness_setup`'s scaffold
/// emitter stamps one invocation per generator scaffold, passing the
/// env var that generator's `setup.sh` writes to `$NEXTEST_ENV`:
///
/// ```text
/// // Default — fail loudly if setup.sh didn't run.
/// ::sdk_test_harness_runner::setup_guard!("SDK_TEST_PYTHON_PYDANTIC2_SETUP");
///
/// // An optional setup guard may be ignored with the same reason as its suite.
/// ::sdk_test_harness_runner::setup_guard!(
///     ignore = "target temporarily disabled", "SDK_TEST_TYPESCRIPT_SETUP");
/// ```
#[macro_export]
macro_rules! setup_guard {
    ($env:literal) => {
        mod setup_guard {
            #[test]
            fn ran() {
                $crate::__check_setup_ran($env);
            }
        }
    };
    (ignore = $reason:literal, $env:literal) => {
        mod setup_guard {
            #[test]
            #[ignore = $reason]
            fn ran() {
                $crate::__check_setup_ran($env);
            }
        }
    };
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
/// // Skip while a target is temporarily disabled.
/// ::sdk_test_harness_runner::build_diagnostics!(ignore = "target temporarily disabled");
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

/// Java generator's test-side glue. Invoked from
/// `crates/java/src/lib.rs` as
/// `sdk_test_harness_runner::java::test_suite!()`.
pub mod java {
    /// `include!`s `OUT_DIR/java_tests.rs` — the per-fixture scaffold
    /// emitted by `sdk_test_harness_setup::java::run_all`.
    #[macro_export]
    macro_rules! java_test_suite {
        () => {
            include!(concat!(env!("OUT_DIR"), "/java_tests.rs"));
        };
    }

    pub use crate::java_test_suite as test_suite;
}

/// Swift generator's test-side glue. Invoked from
/// `crates/swift/src/lib.rs` as
/// `sdk_test_harness_runner::swift::test_suite!()`.
pub mod swift {
    /// `include!`s `OUT_DIR/swift_tests.rs` — the per-fixture
    /// scaffold emitted by `sdk_test_harness_setup::swift::run_all`.
    #[macro_export]
    macro_rules! swift_test_suite {
        () => {
            include!(concat!(env!("OUT_DIR"), "/swift_tests.rs"));
        };
    }

    pub use crate::swift_test_suite as test_suite;
}

/// C++ generator's test-side glue. Invoked from `crates/cpp/src/lib.rs` as
/// `sdk_test_harness_runner::cpp::test_suite!()`.
pub mod cpp {
    /// `include!`s `OUT_DIR/cpp_tests.rs` — the per-fixture scaffold emitted
    /// by `sdk_test_harness_setup::cpp::run_all`.
    #[macro_export]
    macro_rules! cpp_test_suite {
        () => {
            include!(concat!(env!("OUT_DIR"), "/cpp_tests.rs"));
        };
    }

    pub use crate::cpp_test_suite as test_suite;
}

/// Rust generator's test-side glue. Invoked from
/// `crates/rust/src/lib.rs` as
/// `sdk_test_harness_runner::rust::test_suite!()`.
pub mod rust {
    /// `include!`s `OUT_DIR/rust_tests.rs` — the per-fixture scaffold
    /// emitted by `sdk_test_harness_setup::rust::run_all`.
    #[macro_export]
    macro_rules! rust_test_suite {
        () => {
            include!(concat!(env!("OUT_DIR"), "/rust_tests.rs"));
        };
    }

    pub use crate::rust_test_suite as test_suite;
}

/// Node TypeScript test-side glue. Invoked from
/// `crates/typescript/src/lib.rs` as
/// `sdk_test_harness_runner::typescript::test_suite!()`.
pub mod typescript {
    /// `include!`s `OUT_DIR/typescript_tests.rs` — the
    /// per-fixture scaffold emitted by
    /// `sdk_test_harness_setup::typescript::run_all`.
    #[macro_export]
    macro_rules! typescript_test_suite {
        () => {
            include!(concat!(env!("OUT_DIR"), "/typescript_tests.rs"));
        };
    }

    pub use crate::typescript_test_suite as test_suite;
}

/// Go generator's test-side glue.
pub mod go {
    #[macro_export]
    macro_rules! go_test_suite {
        () => {
            include!(concat!(env!("OUT_DIR"), "/go_tests.rs"));
        };
    }

    pub use crate::go_test_suite as test_suite;
}

/// Browser and Cloudflare Workers TypeScript test-side glue. Invoked from
/// `crates/typescript_web/src/lib.rs`.
pub mod typescript_web {
    /// `include!`s `OUT_DIR/typescript_web_tests.rs`, emitted by
    /// `sdk_test_harness_setup::typescript_web::run_all_from_typescript_sources`.
    #[macro_export]
    macro_rules! typescript_web_test_suite {
        () => {
            include!(concat!(env!("OUT_DIR"), "/typescript_web_tests.rs"));
        };
    }

    pub use crate::typescript_web_test_suite as test_suite;
}
