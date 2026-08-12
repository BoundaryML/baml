//! Swift sdk-test target — build-script side.
//!
//! Mirrors [`crate::python_pydantic2`] / [`crate::typescript`] but
//! for the Swift toolchain (SwiftPM + XCTest).
//!
//! `sdkgen_swift` is still an early emitter, so codegen runs under
//! `catch_unwind` and failures downgrade into [`BuildDiagnostics`]
//! records instead of aborting the build (the same soft-fail posture
//! the TypeScript target used while its emitter was a stub).
//!
//! **Native build steps live in `sdk_tests/crates/swift/setup.sh`**,
//! NOT in build.rs. `cargo nextest run` invokes it automatically via a
//! macOS-filtered setup-script binding in
//! `baml_language/.config/nextest.toml`:
//!
//! ```sh
//! cd baml_language
//! cargo nextest run -p sdk_test_swift
//! ```
//!
//! setup.sh builds the `bridge_swift` staticlib for the host arch and
//! assembles `sdks/swift/Binaries/BamlBridgeFFI.xcframework`, which
//! every fixture's path dependency on `sdks/swift` links against.
//! Re-run it after bridge Rust changes. build.rs's job is just
//! codegen and scaffold emit — it writes each fixture's
//! `Package.swift` and test overlay that `swift build` / `swift test`
//! consume.
//!
//! Fixture enforcement is per-fixture: names in `ENFORCED_FIXTURES`
//! get real `#[test]`s; everything else is `#[ignore]`d until its
//! capability phase lands. Non-macOS hosts ignore the toolchain tests
//! entirely (no swift binary on Linux CI runners).

use std::{
    env, fs, panic,
    path::{Path, PathBuf},
};

use sdkgen_swift::NamingConvention;

use crate::{
    BuildDiagnostics, copy_customizable, discover_fixtures, emit_cargo_line,
    fixtures_root_from_manifest, load_fixture, watch_dir, write_codegen_output,
};

/// Per-fixture Package.swift. `__PACKAGE_NAME__` is substituted per
/// fixture. Lives at `src/templates/Package.swift` so editors give it
/// real Swift syntax highlighting.
const PACKAGE_SWIFT_TEMPLATE: &str = include_str!("templates/Package.swift");

/// Threaded through `run_test_cmd` for API uniformity with the uv/pnpm
/// targets. SwiftPM does not read an env var for its cache location —
/// the real sharing comes from SwiftPM's default per-user cache — so
/// this is a harmless no-op var.
const CACHE_SUBDIR: &str = "swiftpm-cache";
const CACHE_ENV_VAR: &str = "BAML_SWIFTPM_CACHE_DIR";

/// Env var `crates/swift/setup.sh` writes to `$NEXTEST_ENV`; the
/// emitted `setup_guard::ran` test checks it. Must stay in sync with
/// that script.
const SETUP_ENV_VAR: &str = "SDK_TEST_SWIFT_SETUP";

/// Fixtures whose `swift_build` / `swift_test` run for real. Everything
/// else is emitted `#[ignore]`d. Flip fixtures in as their capability
/// phases land (Phase 1: type_shapes + function_calls subsets).
const ENFORCED_FIXTURES: &[&str] = &[
    "type_shapes",
    "function_calls",
    "docstrings_etc",
    "llm_functions",
    // No Swift overlay (placeholder test only): everything in it is
    // within Swift's type algebra, so it runs as a codegen-compiles
    // integrity check.
    "unsupported_only",
];

const IGNORE_REASON: &str = "sdkgen_swift emitter incomplete (bridge Phase 0)";

/// Entry point for `crates/swift/build.rs`. Discovers fixtures, runs
/// codegen for each (soft-fail — see module docs), writes the
/// per-fixture Package.swift + test overlay, and emits the test
/// scaffold. Does NOT run swift/cargo — that's setup.sh's job.
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

    for fixture in &fixtures {
        codegen_fixture(&fixtures_root, fixture, &manifest_dir, &mut diagnostics);
    }

    write_fixtures_tests_rs(&out_dir, &fixtures);
    diagnostics.finalize();

    emit_cargo_line(format_args!("cargo:rerun-if-changed=build.rs"));
    watch_dir(&fixtures_root);
    for fixture in &fixtures {
        watch_dir(&manifest_dir.join(fixture).join("customizable"));
    }
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
    let sources_baml = generated.join("Sources").join("Baml");
    let tests_dir = generated.join("Tests").join("BamlTests");

    // Wipe generated/ except SwiftPM's .build/ — preserving it keeps
    // fixture rebuilds incremental (the same reason typescript_node
    // preserves node_modules/).
    if generated.exists() {
        for entry in fs::read_dir(&generated).unwrap() {
            let path = entry.unwrap().path();
            if path.file_name().and_then(|name| name.to_str()) == Some(".build") {
                continue;
            }
            if path.is_dir() {
                fs::remove_dir_all(&path).unwrap();
            } else {
                fs::remove_file(&path).unwrap();
            }
        }
    }
    fs::create_dir_all(&sources_baml).unwrap();
    fs::create_dir_all(&tests_dir).unwrap();

    let pool = loaded.pool;
    let baml_bytecode = loaded.baml_bytecode;
    let codegen_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        sdkgen_swift::to_source_code_with_bytecode(
            &pool,
            &baml_bytecode,
            NamingConvention::PreserveCase,
        )
    }));
    match codegen_result {
        Ok(output) => write_codegen_output(&sources_baml, output, fixture, diagnostics),
        Err(_) => {
            diagnostics.record(
                "codegen",
                fixture,
                "sdkgen_swift::to_source_code_with_bytecode panicked",
            );
        }
    }

    // Copy (not symlink) the test overlay: SwiftPM target membership is
    // path-based and copies keep everything inside generated/.
    let custom = fixture_root.join("customizable");
    if custom.exists() {
        let copy_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            copy_customizable(&custom, &tests_dir);
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

    // A testTarget with zero sources fails `swift build --build-tests`,
    // so fixtures without an overlay get a placeholder case.
    if !has_swift_files(&tests_dir) {
        let placeholder = "import XCTest\n\n\
             final class PlaceholderTests: XCTestCase {\n    \
             /// Keeps the BamlTests target non-empty until this fixture's\n    \
             /// customizable/ overlay lands.\n    \
             func testScaffoldCompiles() {}\n}\n";
        if let Err(e) = fs::write(tests_dir.join("Placeholder.swift"), placeholder) {
            diagnostics.record(
                "placeholder_write",
                fixture,
                format!("write Placeholder.swift: {e}"),
            );
        }
    }

    let package_name = format!("sdk-tests-swift-{}", fixture.replace('_', "-"));
    let package_swift = PACKAGE_SWIFT_TEMPLATE.replace("__PACKAGE_NAME__", &package_name);
    if let Err(e) = fs::write(generated.join("Package.swift"), package_swift) {
        diagnostics.record(
            "package_swift_write",
            fixture,
            format!("write Package.swift: {e}"),
        );
    }
}

fn has_swift_files(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if has_swift_files(&path) {
                return true;
            }
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("swift") {
            return true;
        }
    }
    false
}

/// Emit `OUT_DIR/swift_tests.rs` — a sequence of
/// `::sdk_test_harness_runner::*` invocations. No test bodies authored
/// here; the macros and `run_test_cmd` live in `sdk_test_harness_runner`.
fn write_fixtures_tests_rs(out_dir: &Path, fixtures: &[String]) {
    let mut buf = String::new();
    buf.push_str("// Generated by sdk_test_harness_setup::swift::run_all — do not edit.\n");
    // Diagnostics + setup guard stay ignored while no fixture is
    // enforced: every fixture records stub-emitter noise, and setup.sh
    // only fires where the binding matches (macOS).
    if ENFORCED_FIXTURES.is_empty() {
        buf.push_str(&format!(
            "::sdk_test_harness_runner::build_diagnostics!(ignore = {IGNORE_REASON:?});\n"
        ));
        buf.push_str(&format!(
            "::sdk_test_harness_runner::setup_guard!(ignore = {IGNORE_REASON:?}, {SETUP_ENV_VAR:?});\n"
        ));
    } else {
        buf.push_str("::sdk_test_harness_runner::build_diagnostics!();\n");
        // The Swift setup script only runs on macOS (nextest binding is
        // host-gated); off-macOS the fixture tests are #[ignore]d, so
        // the guard must not fire there either or a Linux/Windows
        // nextest run of this crate panics before reaching the ignores.
        buf.push_str("#[cfg(target_os = \"macos\")]\n");
        buf.push_str(&format!(
            "::sdk_test_harness_runner::setup_guard!({SETUP_ENV_VAR:?});\n"
        ));
    }

    for fixture in fixtures {
        let enforced = ENFORCED_FIXTURES.contains(&fixture.as_str());
        let ignore_attr = if enforced {
            // Still ignored off-macOS: no swift toolchain on Linux CI.
            "    #[cfg_attr(not(target_os = \"macos\"), ignore = \"swift toolchain is macOS-only in CI\")]\n".to_string()
        } else {
            format!("    #[ignore = {IGNORE_REASON:?}]\n")
        };
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

    // One test per fixture on purpose: `swift test` builds first, and a
    // sibling `swift build` test would contend for the same SwiftPM
    // `.build` lock (nextest runs a fixture's tests concurrently;
    // SwiftPM serializes them, doubling wall clock — and a killed run
    // leaves the lock held by an orphaned swift-build).
    #[test]
{ignore_attr}    fn swift_test() {{
        cmd("swift test");
    }}
}}
"#,
            fixture = fixture,
            cache_subdir = CACHE_SUBDIR,
            cache_env_var = CACHE_ENV_VAR,
            ignore_attr = ignore_attr,
        ));
    }

    let target = out_dir.join("swift_tests.rs");
    fs::write(&target, buf).unwrap();
}
