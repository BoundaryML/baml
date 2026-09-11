//! Swift sdk-test codegen.
//!
//! [`run_all`] codegens every shared fixture into
//! `crates/swift/<fixture>/generated/Sources/Baml/`, copies that fixture's
//! `customizable/` overlay into `generated/Tests/BamlTests/`, and writes the
//! per-fixture `Package.swift` that `swift test` consumes.
//!
//! The Swift toolchain is never invoked here; that is `crates/swift/setup.sh`
//! (xcframework assembly), which is also what runs this. Both are macOS-only:
//! the nextest binding for this crate is host-gated, and the fixture tests are
//! `#[ignore]`d off-macOS to match.

use std::{fs, path::Path};

use sdk_test_harness_runner::fixtures;
use sdkgen_swift::NamingConvention;

use crate::{CodegenCtx, Overlay, load_fixture, write_codegen_output};

/// Per-fixture Package.swift. `__PACKAGE_NAME__` is substituted per
/// fixture. Lives at `src/templates/Package.swift` so editors give it
/// real Swift syntax highlighting.
const PACKAGE_SWIFT_TEMPLATE: &str = include_str!("templates/Package.swift");

/// Generate every Swift fixture package. Called by the `swift` subcommand of
/// the `sdk_test_codegen` binary, which `crates/swift/setup.sh` runs before
/// assembling the xcframework.
///
/// Codegen failures abort rather than being recorded: this only runs when
/// someone is running the Swift suite, so a panic here should stop the setup
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
    let sources_baml = generated.join("Sources").join("Baml");

    fs::create_dir_all(&sources_baml).unwrap();

    let output = sdkgen_swift::to_source_code_with_bytecode(
        &loaded.pool,
        &loaded.baml_bytecode,
        NamingConvention::PreserveCase,
    );
    write_codegen_output(&sources_baml, output, fixture);

    let package_name = format!("sdk-tests-swift-{}", fixture.replace('_', "-"));
    let mut overlay = Overlay::new(&fixture_root);
    // Copied (not symlinked): SwiftPM target membership is path-based, so the
    // sources have to live inside the package.
    overlay.copy_tree(&fixture_root.join("customizable"), "Tests/BamlTests");
    // A testTarget with zero sources fails `swift build --build-tests`, so a
    // fixture without an overlay gets a placeholder case.
    if !overlay.any_path_ends_with(".swift") {
        overlay.file(
            "Tests/BamlTests/Placeholder.swift",
            "import XCTest\n\n\
             final class PlaceholderTests: XCTestCase {\n    \
             /// Keeps the BamlTests target non-empty until this fixture's\n    \
             /// customizable/ overlay lands.\n    \
             func testScaffoldCompiles() {}\n}\n",
        );
    }
    overlay.file(
        "Package.swift",
        PACKAGE_SWIFT_TEMPLATE.replace("__PACKAGE_NAME__", &package_name),
    );
    overlay.install();
}
