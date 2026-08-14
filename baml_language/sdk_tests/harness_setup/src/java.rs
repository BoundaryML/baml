//! Java sdk-test target — build-script side.
//!
//! Mirrors [`crate::typescript`] but for the JVM toolchain
//! (Gradle + javac + JUnit 5).
//!
//! `sdkgen_java::to_source_code_with_bytecode` runs directly (no
//! `catch_unwind`): the emitter has landed, so a panic is a real bug
//! and aborts the build loudly. `build_diagnostics` and `setup_guard`
//! run for real; the per-fixture `javac` / `junit` tests remain
//! `#[ignore]`d with `IGNORE_REASON` and are un-ignored capability
//! by capability as the generated API fills in enough for
//! `compileTestJava` to pass.
//!
//! **Gradle steps live in `sdk_tests/crates/java/setup.sh`** (Unix) or
//! **`setup.ps1`** (Windows), NOT in build.rs. `cargo nextest run`
//! invokes the right one automatically via two platform-filtered
//! setup-script bindings (`cfg(unix)` / `cfg(windows)`) in
//! `baml_language/.config/nextest.toml`:
//!
//! ```sh
//! cd baml_language
//! cargo nextest run -p sdk_test_java
//! ```
//!
//! build.rs's job is just codegen + scaffold emit — it writes the
//! per-fixture `build.gradle.kts` / `settings.gradle.kts` that the
//! Gradle invocations consume.
//!
//! Auto-discovery story: build.rs emits `OUT_DIR/java_tests.rs` (a
//! sequence of `::sdk_test_harness_runner::*` invocations), the
//! `sdk_test_harness_runner::java::test_suite!()` macro `include!`s
//! it, and adding a new fixture is `mkdir
//! sdk_tests/fixtures/<name>/baml_src/` + dropping a `customizable/`
//! directory under `sdk_tests/crates/java/<name>/`.

use std::{
    env, fs, panic,
    path::{Path, PathBuf},
};

use sdkgen_java::NamingConvention;

use crate::{
    BuildDiagnostics, copy_customizable, discover_fixtures, emit_cargo_line,
    fixtures_root_from_manifest, load_fixture, watch_dir, write_codegen_output,
};

/// Per-fixture build.gradle.kts — no placeholder; written verbatim.
/// Configures `generated/` itself as the main source root (restricted
/// to `baml_sdk/**` so generated files can declare `package
/// baml_sdk...;`) and `tests/` as the JUnit source root.
///
/// Lives at `src/templates/build.gradle.kts` so editors give it real
/// Kotlin-DSL syntax highlighting.
const BUILD_GRADLE_KTS: &str = include_str!("templates/build.gradle.kts");

/// Per-fixture settings.gradle.kts. `__PROJECT_NAME__` is substituted
/// per fixture. No toolchain auto-provisioning: the JDK is the ambient
/// one from the repo-root `mise.toml` (`java = temurin-23`), and
/// `build.gradle.kts` pins `--release 17` against it — so every runner
/// (Linux/macOS/Windows) must have `java` on its mise install set.
const SETTINGS_GRADLE_KTS_TEMPLATE: &str = include_str!("templates/settings.gradle.kts");

/// Shared Gradle home (`<workspace>/target/<CACHE_SUBDIR>`) —
/// `setup.sh` exports `CACHE_ENV_VAR=<that path>` and
/// `sdk_test_harness_runner::run_test_cmd` sets the same env var when
/// invoking Gradle, so dependency + wrapper + provisioned-JDK caches
/// are shared across fixtures instead of landing in `~/.gradle`.
const CACHE_SUBDIR: &str = "gradle-home";
const CACHE_ENV_VAR: &str = "GRADLE_USER_HOME";

/// Env var the setup scripts write to `$NEXTEST_ENV` and the emitted
/// `setup_guard::ran` test checks for. Must stay in sync with both
/// `crates/java/setup.sh` and `setup.ps1`.
const SETUP_ENV_VAR: &str = "SDK_TEST_JAVA_SETUP";

/// Why every emitted test is `#[ignore]`d for now. Un-ignoring is the
/// signal that a capability's parity tests are expected to pass — see
/// `sdks/agent-docs/bridge-ref/ref-java-state-of-completeness.md`.
/// Fixtures whose generated + test sources compile green — their
/// `javac` gate runs for real (CI enforcement); everything else stays
/// `#[ignore]`d until its API surface lands.
const GREEN_JAVAC_FIXTURES: &[&str] = &[
    "type_shapes",
    "function_calls",
    "llm_functions",
    "docstrings_etc",
];

/// Fixtures whose JUnit runtime suite (`gradle test`) runs green against the
/// built `bridge_java` cdylib — their `junit` gate runs for real (CI
/// enforcement, mirroring [`GREEN_JAVAC_FIXTURES`]); everything else stays
/// `#[ignore]`d until its runtime surface lands. A green-junit fixture must
/// also be a green-javac fixture (the `test` task compiles the test sources
/// first). `llm_functions` qualifies: its suite is fully deterministic offline
/// with no live keys — the streaming tests run against the in-process replay
/// server (keyless SSE recordings) and the `$build_request` tests set their
/// api-key env vars through the native `BridgeEnv` setenv shim, so nothing hits
/// the network. `docstrings_etc` also qualifies trivially: its suite only reads
/// the generated `.java` source and asserts the rolled-up `Attributes:` /
/// `Members:` Javadoc (the JVM analog of Python's `inspect.getdoc`), making no
/// engine calls at all. (Serialize each fixture's javac+junit gates via a
/// nextest test-group; see `.config/nextest.toml`.)
const GREEN_JUNIT_FIXTURES: &[&str] = &[
    "type_shapes",
    "function_calls",
    "llm_functions",
    "docstrings_etc",
];

const IGNORE_REASON: &str =
    "generated Java API not complete enough for this fixture yet — un-ignore as capabilities land";

/// Entry point for `crates/java/build.rs`. Discovers fixtures, runs
/// codegen for each (best-effort — see module docs), writes the
/// Gradle templates, and emits the per-fixture test scaffold. Does
/// NOT touch Gradle — that's setup.sh's job (see module docs).
pub fn run_all() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let fixtures_root = fixtures_root_from_manifest(&manifest_dir);
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let mut diagnostics = BuildDiagnostics::new(&out_dir);

    let fixtures = discover_fixtures(&fixtures_root);
    assert!(
        !fixtures.is_empty(),
        "no fixtures discovered under {}",
        fixtures_root.display()
    );

    let mut fixture_tests = Vec::new();
    for fixture in &fixtures {
        codegen_fixture(&fixtures_root, fixture, &manifest_dir, &mut diagnostics);
        fixture_tests.push(FixtureTests {
            name: fixture.clone(),
            has_junit_tests: has_junit_tests(&manifest_dir.join(fixture).join("customizable")),
        });
    }

    write_fixtures_tests_rs(&out_dir, &fixture_tests);
    diagnostics.finalize();

    emit_cargo_line(format_args!("cargo:rerun-if-changed=build.rs"));
    watch_dir(&fixtures_root);
    for fixture in &fixtures {
        watch_dir(&manifest_dir.join(fixture).join("customizable"));
    }
}

struct FixtureTests {
    name: String,
    has_junit_tests: bool,
}

fn codegen_fixture(
    fixtures_root: &Path,
    fixture: &str,
    manifest_dir: &Path,
    diagnostics: &mut BuildDiagnostics,
) {
    // `load_fixture` panics on .baml compile errors / missing baml_src /
    // empty fixture — those are author bugs in our repo, not env
    // issues, so we keep the hard failure.
    let loaded = load_fixture(fixtures_root, fixture);
    let fixture_root = manifest_dir.join(fixture);
    let generated = fixture_root.join("generated");
    let baml_sdk = generated.join("baml_sdk");

    if generated.exists() {
        for entry in fs::read_dir(&generated).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            // Keep Gradle's incremental state between builds; only the
            // staged inputs are regenerated.
            if matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some(".gradle") | Some("build") | Some(".kotlin")
            ) {
                continue;
            }
            if path.is_dir() {
                fs::remove_dir_all(&path).unwrap();
            } else {
                fs::remove_file(&path).unwrap();
            }
        }
    }
    fs::create_dir_all(&baml_sdk).unwrap();

    // Run codegen directly — the emitter has landed, so a panic here is a
    // real bug and should abort the build loudly rather than being captured
    // and downgraded to an empty `baml_sdk/`.
    let pool = loaded.pool;
    let baml_bytecode = loaded.baml_bytecode;
    let output = sdkgen_java::to_source_code_with_bytecode(
        &pool,
        &baml_bytecode,
        NamingConvention::PreserveCase,
    );
    write_codegen_output(&baml_sdk, output, fixture, diagnostics);

    let custom = fixture_root.join("customizable");
    if custom.exists() {
        // Copy (not symlink) into `generated/tests/` — the Gradle test
        // source root. Copying keeps javac's view of the sources inside
        // `generated/`, mirroring typescript_node's rationale. Recursive,
        // because Java sources live in package directories (e.g.
        // `roundtrip_tests/`, mirroring python_pydantic2's layout).
        let tests_dir = generated.join("tests");
        let copy_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            fs::create_dir_all(&tests_dir).unwrap();
            copy_customizable_recursive(&custom, &tests_dir);
        }));
        if copy_result.is_err() {
            diagnostics.record(
                "copy_customizable",
                fixture,
                format!(
                    "copy_customizable({}, {}) panicked",
                    custom.display(),
                    tests_dir.display()
                ),
            );
        }
    }

    let project_name = format!("sdk-tests-java-{}", fixture.replace('_', "-"));
    let settings_gradle = SETTINGS_GRADLE_KTS_TEMPLATE.replace("__PROJECT_NAME__", &project_name);
    if let Err(e) = fs::write(generated.join("settings.gradle.kts"), settings_gradle) {
        diagnostics.record(
            "gradle_files_write",
            fixture,
            format!("write settings.gradle.kts: {e}"),
        );
    }
    if let Err(e) = fs::write(generated.join("build.gradle.kts"), BUILD_GRADLE_KTS) {
        diagnostics.record(
            "gradle_files_write",
            fixture,
            format!("write build.gradle.kts: {e}"),
        );
    }
}

/// Emit `OUT_DIR/java_tests.rs` — a sequence of
/// `::sdk_test_harness_runner::*` invocations. No test bodies authored
/// here; `build_diagnostics!` and `run_test_cmd` live in
/// `sdk_test_harness_runner`.
fn write_fixtures_tests_rs(out_dir: &Path, fixtures: &[FixtureTests]) {
    let mut buf = String::new();
    buf.push_str("// Generated by sdk_test_harness_setup::java::run_all — do not edit.\n");
    buf.push_str("::sdk_test_harness_runner::build_diagnostics!();\n");
    buf.push_str(&format!(
        "::sdk_test_harness_runner::setup_guard!({SETUP_ENV_VAR:?});\n"
    ));
    for fixture in fixtures {
        let name = &fixture.name;
        buf.push_str(&format!(
            r#"
mod {fixture} {{
    fn cmd(c: &str) {{
        ::sdk_test_harness_runner::run_test_cmd(
            "{fixture}",
            c,
            "{cache_subdir}",
            "{cache_env_var}",
        );
    }}

    #[test]
{javac_ignore}    fn javac() {{
        cmd("gradle --no-daemon --console=plain compileTestJava");
    }}
"#,
            fixture = name,
            cache_subdir = CACHE_SUBDIR,
            cache_env_var = CACHE_ENV_VAR,
            javac_ignore = if GREEN_JAVAC_FIXTURES.contains(&name.as_str()) {
                String::new()
            } else {
                format!("    #[ignore = {IGNORE_REASON:?}]\n")
            },
        ));

        if fixture.has_junit_tests {
            buf.push_str(&format!(
                r#"
    #[test]
{junit_ignore}    fn junit() {{
        ::sdk_test_harness_runner::run_java_test_cmd(
            "{fixture}",
            "gradle --no-daemon --console=plain test",
            "{cache_subdir}",
            "{cache_env_var}",
        );
    }}
"#,
                junit_ignore = if GREEN_JUNIT_FIXTURES.contains(&name.as_str()) {
                    String::new()
                } else {
                    format!("    #[ignore = {IGNORE_REASON:?}]\n")
                },
                fixture = fixture.name,
                cache_subdir = CACHE_SUBDIR,
                cache_env_var = CACHE_ENV_VAR,
            ));
        }

        buf.push_str(
            r#"
}
"#,
        );
    }
    let target = out_dir.join("java_tests.rs");
    fs::write(&target, buf).unwrap();
}

/// Like [`copy_customizable`], but mirrors subdirectories into the
/// destination — Java test sources live in package directories
/// (`roundtrip_tests/TestPrimitives.java` declares
/// `package roundtrip_tests;`), so the tree must be preserved.
fn copy_customizable_recursive(customizable_dir: &Path, dst_dir: &Path) {
    for entry in fs::read_dir(customizable_dir).unwrap() {
        let entry = entry.unwrap();
        let src = entry.path();
        if src.is_dir() {
            let dst = dst_dir.join(entry.file_name());
            fs::create_dir_all(&dst).unwrap_or_else(|e| {
                panic!(
                    "Failed to create {} for customizable overlay: {e}",
                    dst.display()
                )
            });
            copy_customizable_recursive(&src, &dst);
        }
    }
    copy_customizable(customizable_dir, dst_dir);
}

fn has_junit_tests(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if has_junit_tests(&path) {
                return true;
            }
        } else if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext == "java")
        {
            return true;
        }
    }

    false
}
