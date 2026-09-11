//! Test-side runtime for the `sdk_tests/crates/<generator>/` crates, wired in
//! as their `[dev-dependencies]`.
//!
//! Each generator crate declares its suite in source, by invoking that
//! generator's `test_suite!` macro with its fixture rows:
//!
//! ```text
//! // crates/python_pydantic2/src/lib.rs
//! sdk_test_harness_runner::python_pydantic2::test_suite! {
//!     fixture docstrings_etc;
//!     fixture function_calls;
//! }
//! ```
//!
//! That expands to one `mod <fixture>` per row, each holding the generator's
//! toolchain checks. The per-generator macros live below, alongside
//! [`setup_guard!`] (asserts the crate's setup.sh ran this run),
//! [`fixture_manifest!`] (pins the rows against the corpus), and
//! [`run_test_cmd`] (the toolchain-command runner).
//!
//! The rows are source rather than generated because `sdk_test_codegen` runs
//! from `setup.sh`, which nextest fires *after* these test binaries are built.

use std::{
    env, fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    process::{Command, Output},
};

pub mod fixtures;

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

/// `<generator crate>/<fixture>/<subdir>`, for tests that need to look at a
/// fixture's inputs rather than run a toolchain against its output.
///
/// Resolves against `CARGO_MANIFEST_DIR`, which cargo sets for the test
/// binary at run time, so callers need not thread the crate root through.
pub fn fixture_path(fixture: &str, subdir: &str) -> PathBuf {
    PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"))
        .join(fixture)
        .join(subdir)
}

/// Whether `dir` holds any file whose name ends with `suffix`, at any depth.
///
/// Suffix rather than extension: the TypeScript suites key off `.test.ts`,
/// which is not an extension.
pub fn has_file_with_suffix(dir: &Path, suffix: &str) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        if path.is_dir() {
            return has_file_with_suffix(&path, suffix);
        }
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(suffix))
    })
}

/// The engine cdylib the generated SDKs load at run time.
///
/// A test binary lives in `<target>/<profile>/deps/`, so the sibling
/// `<target>/<profile>/` is where `cargo build -p bridge_cffi` put the
/// library — regardless of `CARGO_TARGET_DIR` or profile.
pub fn engine_library() -> PathBuf {
    let exe = env::current_exe().expect("current test binary path");
    let profile_dir = exe
        .parent()
        .and_then(Path::parent)
        .expect("test binary not under <target>/<profile>/deps");
    let name = if cfg!(target_os = "windows") {
        "bridge_cffi.dll"
    } else if cfg!(target_os = "macos") {
        "libbridge_cffi.dylib"
    } else {
        "libbridge_cffi.so"
    };
    let path = profile_dir.join(name);
    assert!(
        path.is_file(),
        "engine library not found at {} — run `cargo build -p bridge_cffi` first \
         (the nextest setup script does this automatically)",
        path.display()
    );
    path
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
/// The breadcrumb is absent under plain `cargo test`, and that is a failure,
/// not a pass: setup.sh is what generates each fixture's SDK, so without it
/// these tests would run against a stale tree or none at all. Called from the
/// `mod setup_guard { #[test] fn ran }` block the [`setup_guard!`] macro
/// expands to.
#[doc(hidden)]
pub fn __check_setup_ran(env_var: &str) {
    if env::var_os(env_var).is_some() {
        return;
    }

    panic!(
        "sdk-test setup script did not run for this test run \
         (env var `{env_var}` is unset).\n\n\
         These tests require their `crates/<generator>/setup.sh` to have run \
         first: it generates each fixture's SDK and installs the language \
         toolchain, then sets `{env_var}` via $NEXTEST_ENV.\n\n\
         Fix: run the tests with `cargo nextest run` — it fires setup.sh \
         automatically. Plain `cargo test` cannot."
    );
}

/// Emit the `mod setup_guard { #[test] fn ran }` test that asserts
/// the per-generator setup script ran this test run (via
/// [`__check_setup_ran`]). `sdk_test_codegen`'s scaffold
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

/// Emit the `mod fixture_manifest { #[test] fn matches_corpus }` oracle that
/// pins a generator crate's declared fixtures against
/// [`fixtures::SHARED`] and the corpus on disk. Invoked once by each
/// generator's `test_suite!` expansion, with that suite's fixture names.
#[macro_export]
macro_rules! fixture_manifest {
    ( $( $name:ident ),+ $(,)? ) => {
        mod fixture_manifest {
            /// The fixtures this crate declares tests for.
            const DECLARED: &[&str] = &[ $( stringify!($name) ),+ ];

            #[test]
            fn matches_corpus() {
                $crate::fixtures::assert_shared_manifest(env!("CARGO_MANIFEST_DIR"), DECLARED);
            }
        }
    };
}

/// Python + pydantic2 generator's test-side glue. Invoked from
/// `crates/python_pydantic2/src/lib.rs`.
pub mod python_pydantic2 {
    /// Declare the Python suite: three toolchain checks per fixture, plus the
    /// shared setup guard and fixture-manifest oracle.
    ///
    /// The fixture list is source rather than build-script output because
    /// `sdk_test_codegen` runs from `setup.sh`, which nextest fires *after*
    /// this crate's test binary is already compiled. `fixture_manifest!`
    /// keeps the list honest.
    #[macro_export]
    macro_rules! python_pydantic2_test_suite {
        ( $( fixture $name:ident; )+ ) => {
            $crate::setup_guard!("SDK_TEST_PYTHON_PYDANTIC2_SETUP");
            $crate::fixture_manifest!( $( $name ),+ );

            $(
                mod $name {
                    const CACHE_SUBDIR: &str = "uv-cache";
                    const CACHE_ENV_VAR: &str = "UV_CACHE_DIR";

                    fn cmd(command: &str) {
                        $crate::run_test_cmd(
                            stringify!($name),
                            command,
                            CACHE_SUBDIR,
                            CACHE_ENV_VAR,
                        );
                    }

                    #[test]
                    fn ruff() {
                        cmd("uv run ruff check --config pyproject.toml baml_sdk");
                    }

                    #[test]
                    fn pyright() {
                        cmd("uv run pyright");
                    }

                    #[test]
                    fn pytest() {
                        // Exit 5 is pytest's "collected no tests", which is a
                        // pass for a fixture whose overlay is all static
                        // type-check probes.
                        $crate::run_test_cmd_allowing_exit_codes(
                            stringify!($name),
                            "uv run pytest -v",
                            CACHE_SUBDIR,
                            CACHE_ENV_VAR,
                            &[5],
                        );
                    }
                }
            )+
        };
    }

    pub use crate::python_pydantic2_test_suite as test_suite;
}

/// Expand one gated Java check. `macro_rules!` cannot expand to an attribute
/// position, so the gate has to emit the whole item.
#[macro_export]
#[doc(hidden)]
macro_rules! __java_gate {
    (on, $name:ident, $body:block) => {
        #[test]
        fn $name() $body
    };
    (later, $name:ident, $body:block) => {
        #[test]
        #[ignore = "generated Java API not complete enough for this fixture yet \
                    — un-ignore as capabilities land"]
        fn $name() $body
    };
}

/// Java generator's test-side glue. Invoked from `crates/java/src/lib.rs`.
pub mod java {
    /// Declare the Java suite: a `javac` and a `junit` gate per fixture, plus
    /// the shared setup guard and fixture-manifest oracle.
    ///
    /// Each gate is marked `on` or `later`. `later` emits the test `#[ignore]`d
    /// — the generated API is not complete enough for that fixture yet, and
    /// un-ignoring is the signal that its parity tests are expected to pass.
    /// A green `junit` requires a green `javac`: the `test` task compiles the
    /// test sources first.
    ///
    /// ```text
    /// fixture type_shapes      { javac: on,    junit: on    }
    /// fixture unsupported_only { javac: later, junit: later }
    /// ```
    #[macro_export]
    macro_rules! java_test_suite {
        ( $( fixture $name:ident { javac: $javac:ident, junit: $junit:ident } )+ ) => {
            $crate::setup_guard!("SDK_TEST_JAVA_SETUP");
            $crate::fixture_manifest!( $( $name ),+ );

            $(
                mod $name {
                    const CACHE_SUBDIR: &str = "gradle-home";
                    const CACHE_ENV_VAR: &str = "GRADLE_USER_HOME";

                    fn cmd(command: &str) {
                        $crate::run_test_cmd(
                            stringify!($name),
                            command,
                            CACHE_SUBDIR,
                            CACHE_ENV_VAR,
                        );
                    }

                    $crate::__java_gate!($javac, javac, {
                        cmd("gradle --no-daemon --console=plain compileTestJava");
                    });

                    $crate::__java_gate!($junit, junit, {
                        // A fixture whose overlay has no `.java` sources has
                        // nothing for `gradle test` to compile or run.
                        if !$crate::has_file_with_suffix(
                            &$crate::fixture_path(stringify!($name), "customizable"),
                            ".java",
                        ) {
                            return;
                        }
                        $crate::run_java_test_cmd(
                            stringify!($name),
                            "gradle --no-daemon --console=plain test",
                            CACHE_SUBDIR,
                            CACHE_ENV_VAR,
                        );
                    });
                }
            )+
        };
    }

    pub use crate::java_test_suite as test_suite;
}

/// Expand one Swift fixture's check, gated or not. `macro_rules!` cannot
/// expand to an attribute position, so the gate has to emit the whole item.
///
/// An ungated fixture still skips off-macOS — there is no Swift toolchain on
/// the other CI hosts. A gated one is `#[ignore]`d everywhere: it is known
/// broken on the only platform that can run it, so there is nothing for the
/// host check to add.
#[macro_export]
#[doc(hidden)]
macro_rules! __swift_gate {
    ( ; $name:ident, $body:block ) => {
        #[test]
        #[cfg_attr(not(target_os = "macos"), ignore = "swift toolchain is macOS-only in CI")]
        fn $name() $body
    };
    ( ($reason:literal) ; $name:ident, $body:block ) => {
        #[test]
        #[ignore = $reason]
        fn $name() $body
    };
}

/// Swift generator's test-side glue. Invoked from `crates/swift/src/lib.rs`.
pub mod swift {
    /// Declare the Swift suite: one `swift test` per fixture, plus the shared
    /// setup guard and fixture-manifest oracle.
    ///
    /// A row may carry `later "<reason>"` to mark that fixture's suite as
    /// known-broken; it is then emitted `#[ignore]`d with that reason. Each
    /// row states its own reason because the causes differ — an emitter gap is
    /// not a hanging test.
    ///
    /// ```text
    /// fixture type_shapes;
    /// fixture llm_functions later "sdkgen_swift skips the streaming projections";
    /// ```
    ///
    /// Everything that touches the Swift toolchain is macOS-only: this crate's
    /// nextest setup binding is host-gated, so off-macOS no setup script runs
    /// and nothing is generated. The setup guard is `cfg`-ed out there too, or
    /// a Linux run would fail it before reaching the ignores. The manifest
    /// oracle stays live on every host — it only reads the corpus.
    #[macro_export]
    macro_rules! swift_test_suite {
        ( $( fixture $name:ident $( later $reason:literal )? ; )+ ) => {
            #[cfg(target_os = "macos")]
            $crate::setup_guard!("SDK_TEST_SWIFT_SETUP");
            $crate::fixture_manifest!( $( $name ),+ );

            $(
                mod $name {
                    // One test per fixture on purpose: `swift test` builds
                    // first, and a sibling `swift build` test would contend for
                    // the same SwiftPM `.build` lock (nextest runs a fixture's
                    // tests concurrently; SwiftPM serializes them, doubling
                    // wall clock — and a killed run leaves the lock held by an
                    // orphaned swift-build).
                    $crate::__swift_gate!( $( ($reason) )? ; swift_test, {
                        $crate::run_test_cmd(
                            stringify!($name),
                            "swift test",
                            // SwiftPM reads no env var for its cache location;
                            // the real sharing is its default per-user cache.
                            // Threaded through for API uniformity with the
                            // uv/pnpm targets.
                            "swiftpm-cache",
                            "BAML_SWIFTPM_CACHE_DIR",
                        );
                    });
                }
            )+
        };
    }

    pub use crate::swift_test_suite as test_suite;
}

/// C++ generator's test-side glue. Invoked from `crates/cpp/src/lib.rs`.
pub mod cpp {
    /// Declare the C++ suite: two toolchain checks per fixture, plus the
    /// shared setup guard and fixture-manifest oracle.
    ///
    /// The fixture list is source rather than build-script output because
    /// `sdk_test_codegen` runs from `setup.sh`, which nextest fires *after*
    /// this crate's test binary is already compiled. `fixture_manifest!`
    /// keeps the list honest.
    ///
    /// ```text
    /// sdk_test_harness_runner::cpp::test_suite! {
    ///     fixture docstrings_etc;
    ///     fixture function_calls;
    /// }
    /// ```
    #[macro_export]
    macro_rules! cpp_test_suite {
        ( $( fixture $name:ident; )+ ) => {
            $crate::setup_guard!("SDK_TEST_CPP_SETUP");
            $crate::fixture_manifest!( $( $name ),+ );

            $(
                mod $name {
                    fn cmd(command: &str) {
                        $crate::run_test_cmd(
                            stringify!($name),
                            command,
                            "cpp-cache",
                            "SDK_TEST_CPP_CACHE_DIR",
                        );
                    }

                    #[test]
                    fn compile() {
                        cmd("bash test.sh compile");
                    }

                    /// Recompiles rather than reusing `compile`'s output:
                    /// nextest runs each test in its own process, so the two
                    /// cannot share state or assume an order.
                    #[test]
                    fn run() {
                        cmd("bash test.sh run");
                    }
                }
            )+
        };
    }

    pub use crate::cpp_test_suite as test_suite;
}

/// Rust generator's test-side glue. Invoked from `crates/rust/src/lib.rs`.
pub mod rust {
    /// Edition stamped into every generated fixture crate, and the `--edition`
    /// its `rustfmt` gate passes. `sdk_test_codegen::rust` reads the same
    /// const when it writes each `Cargo.toml`, so the two cannot diverge.
    pub const GENERATED_EDITION: &str = "2024";

    /// Declare the Rust suite: three toolchain checks per fixture, plus the
    /// shared setup guard and fixture-manifest oracle.
    ///
    /// `fmt` checks the hand-ported test files reachable from `tests/main.rs`
    /// (rustfmt follows the enabled `mod` declarations; generated `src/` is
    /// intentionally not checked — the emitter's pretty-printer is its
    /// canonical format), `clippy` lints the generated library, and
    /// `cargo_test` compiles and runs the enabled ports. Only `cargo_test`
    /// gets `BAML_LIBRARY_PATH`: `baml_bridge` is dylib-only, so the fixture's
    /// tests load the engine cdylib at run time, while fmt and clippy never
    /// execute it.
    #[macro_export]
    macro_rules! rust_test_suite {
        ( $( fixture $name:ident; )+ ) => {
            $crate::setup_guard!("SDK_TEST_RUST_SETUP");
            $crate::fixture_manifest!( $( $name ),+ );

            $(
                mod $name {
                    const CACHE_SUBDIR: &str = "sdk-rust-target";
                    const CACHE_ENV_VAR: &str = "CARGO_TARGET_DIR";

                    fn cmd_env(command: &str, extra_env: &[(&str, &str)]) {
                        // Never spawn cargo without the generated manifest in
                        // place: cargo discovers manifests *upward*, so in its
                        // absence a fixture-level `cargo test` would silently
                        // become a workspace-wide one — re-entering this very
                        // test suite and forking cargo processes without bound.
                        // (The `--manifest-path Cargo.toml` pin on the commands
                        // below is the second layer of the same defense.)
                        let manifest = ::std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                            .join(stringify!($name))
                            .join("generated")
                            .join("Cargo.toml");
                        assert!(
                            manifest.exists(),
                            "{} is missing — `sdk_test_codegen rust` did not generate \
                             this fixture; refusing to run cargo without it",
                            manifest.display(),
                        );
                        $crate::run_test_cmd_with_env(
                            stringify!($name),
                            command,
                            CACHE_SUBDIR,
                            CACHE_ENV_VAR,
                            extra_env,
                        );
                    }

                    fn cmd(command: &str) {
                        cmd_env(command, &[]);
                    }

                    #[test]
                    fn fmt() {
                        cmd(&format!(
                            "rustfmt --edition {} --check tests/main.rs",
                            $crate::rust::GENERATED_EDITION,
                        ));
                    }

                    #[test]
                    fn clippy() {
                        cmd("cargo clippy --manifest-path Cargo.toml -- -D warnings");
                    }

                    #[test]
                    fn cargo_test() {
                        let engine = $crate::engine_library();
                        cmd_env(
                            "cargo test --manifest-path Cargo.toml",
                            &[
                                (
                                    "BAML_LIBRARY_PATH",
                                    engine.to_str().expect("engine path is valid UTF-8"),
                                ),
                                ("BAML_LIBRARY_DISABLE_DOWNLOAD", "true"),
                            ],
                        );
                    }
                }
            )+
        };
    }

    pub use crate::rust_test_suite as test_suite;
}

/// Node TypeScript test-side glue. Invoked from
/// `crates/typescript/src/lib.rs`.
pub mod typescript {
    /// Declare the Node suite: three checks per fixture, one bridge-wide
    /// `attw` check, plus the shared setup guard and fixture-manifest oracle.
    #[macro_export]
    macro_rules! typescript_test_suite {
        ( $( fixture $name:ident; )+ ) => {
            $crate::setup_guard!("SDK_TEST_TYPESCRIPT_SETUP");
            $crate::fixture_manifest!( $( $name ),+ );

            /// Not per-fixture: one check of the bridge package itself.
            mod bridge_typescript {
                #[test]
                fn attw() {
                    // Runs the bridge's own `attw` package script rather than
                    // `pnpm exec attw`, so a local `pnpm attw` and this test
                    // stay the same check — the script stages the pack without
                    // the native addon (`typescript_src/attw-check.js` explains
                    // why).
                    $crate::run_workspace_cmd(
                        "sdks/typescript/bridge_typescript",
                        "pnpm run attw",
                        "pnpm-store",
                        "npm_config_store_dir",
                    );
                }
            }

            $(
                mod $name {
                    fn cmd(command: &str) {
                        $crate::run_test_cmd(
                            stringify!($name),
                            command,
                            "pnpm-store",
                            "npm_config_store_dir",
                        );
                    }

                    #[test]
                    fn esm_node() {
                        $crate::assert_typescript_node_generated_esm(stringify!($name), "node");
                    }

                    #[test]
                    fn tsc_node() {
                        cmd("node node_modules/typescript/bin/tsc --noEmit --project tsconfig.node.json");
                    }

                    #[test]
                    fn vitest_node() {
                        // A fixture whose overlay carries no `.test.ts` has
                        // nothing for vitest to collect.
                        if !$crate::has_file_with_suffix(
                            &$crate::fixture_path(stringify!($name), "generated/node"),
                            ".test.ts",
                        ) {
                            return;
                        }
                        cmd("pnpm exec vitest run --config vitest.node.config.ts");
                    }
                }
            )+
        };
    }

    pub use crate::typescript_test_suite as test_suite;
}

/// Go generator's test-side glue. Invoked from `crates/go/src/lib.rs`.
pub mod go {
    /// Declare the Go suite: one `go test ./...` per fixture, plus the shared
    /// setup guard and fixture-manifest oracle.
    ///
    /// Takes two row kinds. `fixture` rows come from the shared corpus and are
    /// the ones checked against [`fixtures::SHARED`]; a `synthetic` row has no
    /// `baml_src` and is staged from a hand-built `SymbolPool` by
    /// `sdk_test_codegen::go`, so it sits outside that check by construction.
    ///
    /// Also emits `FIXTURES`, the full list, so crate-local tests that sweep
    /// every generated tree stay in step with the suite.
    #[macro_export]
    macro_rules! go_test_suite {
        ( $( fixture $name:ident; )+ $( synthetic $synthetic:ident; )* ) => {
            $crate::setup_guard!("SDK_TEST_GO_SETUP");
            $crate::fixture_manifest!( $( $name ),+ );

            /// Every fixture this suite covers, corpus rows and synthetic alike.
            pub(crate) const FIXTURES: &[&str] = &[
                $( stringify!($name), )+
                $( stringify!($synthetic), )*
            ];

            $(
                mod $name {
                    #[test]
                    fn go_test() {
                        $crate::run_go_test(stringify!($name));
                    }
                }
            )+

            $(
                mod $synthetic {
                    #[test]
                    fn go_test() {
                        $crate::run_go_test(stringify!($synthetic));
                    }
                }
            )*
        };
    }

    pub use crate::go_test_suite as test_suite;
}

/// Browser and Cloudflare Workers TypeScript test-side glue. Invoked from
/// `crates/typescript_web/src/lib.rs`.
pub mod typescript_web {
    /// Declare the Web and Workers suite: six checks per fixture, plus the
    /// shared setup guard and fixture-manifest oracle.
    ///
    /// This crate owns no checked-in TypeScript tests — `sdk_test_codegen`
    /// copies the canonical corpus over from the sibling `crates/typescript`
    /// package into local Web and Workers trees.
    #[macro_export]
    macro_rules! typescript_web_test_suite {
        ( $( fixture $name:ident; )+ ) => {
            $crate::setup_guard!("SDK_TEST_TYPESCRIPT_WEB_SETUP");
            $crate::fixture_manifest!( $( $name ),+ );

            $(
                mod $name {
                    fn cmd(command: &str) {
                        $crate::run_test_cmd(
                            stringify!($name),
                            command,
                            "pnpm-store",
                            "npm_config_store_dir",
                        );
                    }

                    /// Whether the named runtime tree carries any vitest file.
                    fn has_tests(runtime: &str) -> bool {
                        $crate::has_file_with_suffix(
                            &$crate::fixture_path(
                                stringify!($name),
                                &format!("generated/{runtime}"),
                            ),
                            ".test.ts",
                        )
                    }

                    #[test]
                    fn esm_web() {
                        $crate::assert_typescript_web_generated_esm(stringify!($name), "web");
                    }

                    #[test]
                    fn esm_workers() {
                        $crate::assert_typescript_web_generated_esm(stringify!($name), "workers");
                    }

                    #[test]
                    fn tsc_web() {
                        cmd("node node_modules/typescript/bin/tsc --noEmit --project tsconfig.web.json");
                    }

                    #[test]
                    fn tsc_workers() {
                        cmd("node node_modules/typescript/bin/tsc --noEmit --project tsconfig.workers.json");
                    }

                    #[test]
                    fn vitest_web() {
                        // A fixture whose overlay carries no `.test.ts` has
                        // nothing for vitest to collect.
                        if !has_tests("web") {
                            return;
                        }
                        cmd("pnpm exec vitest run --config vitest.web.config.ts");
                    }

                    #[test]
                    fn vitest_workers() {
                        if has_tests("workers") {
                            cmd("pnpm exec vitest run --config vitest.workers.config.ts");
                        }
                        // The integration config drives the worker startup
                        // smoke, which every fixture has whether or not it
                        // carries a ported suite.
                        cmd("pnpm exec vitest run --config vitest.integration.config.ts");
                    }
                }
            )+
        };
    }

    pub use crate::typescript_web_test_suite as test_suite;
}
