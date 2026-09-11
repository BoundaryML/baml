//! Java sdk-test codegen (Gradle + javac + JUnit 5).
//!
//! [`run_all`] codegens every shared fixture into
//! `crates/java/<fixture>/generated/baml_sdk/`, copies that fixture's
//! `customizable/` overlay into `generated/tests/` (the Gradle test source
//! root), and writes the per-fixture `build.gradle.kts` /
//! `settings.gradle.kts` the Gradle invocations consume.
//!
//! `sdkgen_java::to_source_code_with_bytecode` runs directly: the emitter has
//! landed, so a panic is a real bug and should abort loudly. Which fixtures'
//! `javac` / `junit` gates actually run — as opposed to staying `#[ignore]`d
//! while the generated API fills in — is declared in
//! `crates/java/src/lib.rs`, next to the tests themselves.
//!
//! Gradle itself is never invoked here; that is `crates/java/setup.sh`'s job,
//! and setup.sh is also what runs this.

use std::{fs, path::Path};

use sdk_test_harness_runner::fixtures;
use sdkgen_java::NamingConvention;

use crate::{CodegenCtx, copy_customizable, load_fixture, write_codegen_output};

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

/// Generate every Java fixture SDK. Called by the `java` subcommand of the
/// `sdk_test_codegen` binary, which `crates/java/setup.sh` runs before its
/// Gradle steps.
///
/// Codegen failures abort rather than being recorded: this only runs when
/// someone is running the Java suite, so a panic here should stop the setup
/// script outright instead of surfacing later as a separate test.
pub fn run_all(ctx: &CodegenCtx) {
    let discovered = fixtures::discover_shared(&ctx.fixtures_root);
    assert_eq!(
        discovered,
        fixtures::SHARED,
        "the fixture corpus at {} has drifted from `fixtures::SHARED`",
        ctx.fixtures_root.display()
    );
    for fixture in fixtures::SHARED {
        codegen_fixture(&ctx.fixtures_root, fixture, &ctx.crate_dir);
    }
}

fn codegen_fixture(fixtures_root: &Path, fixture: &str, crate_dir: &Path) {
    let loaded = load_fixture(fixtures_root, fixture);
    let fixture_root = crate_dir.join(fixture);
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
    write_codegen_output(&baml_sdk, output, fixture);

    let custom = fixture_root.join("customizable");
    if custom.exists() {
        // Copy (not symlink) into `generated/tests/` — the Gradle test
        // source root. Copying keeps javac's view of the sources inside
        // `generated/`, mirroring typescript_node's rationale. Recursive,
        // because Java sources live in package directories (e.g.
        // `roundtrip_tests/`, mirroring python_pydantic2's layout).
        let tests_dir = generated.join("tests");
        fs::create_dir_all(&tests_dir).unwrap();
        copy_customizable(&custom, &tests_dir);
    }

    let project_name = format!("sdk-tests-java-{}", fixture.replace('_', "-"));
    let settings_gradle = SETTINGS_GRADLE_KTS_TEMPLATE.replace("__PROJECT_NAME__", &project_name);
    for (name, contents) in [
        ("settings.gradle.kts", settings_gradle.as_str()),
        ("build.gradle.kts", BUILD_GRADLE_KTS),
    ] {
        let path = generated.join(name);
        fs::write(&path, contents)
            .unwrap_or_else(|error| panic!("failed to write {}: {error}", path.display()));
    }
}
