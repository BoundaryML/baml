//! C++ sdk-test codegen.
//!
//! [`run_all`] codegens every shared fixture into
//! `crates/cpp/<fixture>/generated/baml_sdk/` via `sdkgen_cpp`, symlinks the
//! `customizable/` overlay (test sources live in `customizable/tests/*.cc`)
//! into the generated tree, and writes the per-fixture `test.sh`
//! compile-and-run driver the tests invoke.
//!
//! The `bridge_cffi` cdylib build is NOT run here — it lives in
//! `crates/cpp/setup.sh`, which is also what runs this.

use std::{fs, path::Path};

use sdk_test_harness_runner::fixtures;

use crate::{CodegenCtx, load_fixture, symlink_customizable, write_codegen_output};

/// Per-fixture compile-and-run driver, written to `<fixture>/generated/test.sh`.
const TEST_SH_TEMPLATE: &str = include_str!("templates/cpp_test.sh");

/// Generate every C++ fixture SDK. Called by the `cpp` subcommand of the
/// `sdk_test_codegen` binary, which `crates/cpp/setup.sh` runs before its
/// toolchain build.
///
/// Codegen failures abort rather than being recorded: this only runs when
/// someone is running the C++ suite, so a panic here should stop the setup
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
        fs::remove_dir_all(&generated).unwrap();
    }
    fs::create_dir_all(&baml_sdk).unwrap();

    let pool = loaded.pool;
    let user_baml_paths: Vec<sdkgen_cpp::UserBamlFile> = loaded
        .user_baml_files
        .into_iter()
        .map(|(rel, _)| rel)
        .collect();
    let baml_bytecode = loaded.baml_bytecode;
    let output = sdkgen_cpp::to_source_code_with_bytecode(&pool, &user_baml_paths, &baml_bytecode);
    write_codegen_output(&baml_sdk, output, fixture);

    let custom = fixture_root.join("customizable");
    if custom.exists() {
        symlink_customizable(&custom, &generated);
    }

    let test_sh = generated.join("test.sh");
    fs::write(&test_sh, TEST_SH_TEMPLATE)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", test_sh.display()));
}
