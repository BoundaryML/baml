//! Node TypeScript SDK test generation.
//!
//! The canonical TypeScript test corpus lives under `sdk_tests/crates/typescript`.
//! This module emits only the Node SDK, Node package configuration, and Node Rust
//! test scaffold. Browser and Workers generation lives in [`crate::typescript_web`].

use std::{fs, path::Path};

use sdk_test_harness_runner::fixtures;
use sdkgen_typescript_shared::sdkgen_typescript::{self, NamingConvention};

use crate::{CodegenCtx, Overlay, load_fixture, write_codegen_output};

const PACKAGE_JSON_TEMPLATE: &str = include_str!("templates/package_node.json");
const TSCONFIG_JSON: &str = include_str!("templates/tsconfig.json");
const VITEST_NODE_CONFIG: &str = include_str!("templates/vitest_node.config.ts");
pub(crate) const TEST_RUNTIME: &str = include_str!("templates/test_runtime.ts");

/// Generate every Node TypeScript fixture SDK. Called by the `typescript`
/// subcommand of the `sdk_test_codegen` binary, which
/// `crates/typescript/setup.sh` runs before `pnpm install`.
///
/// Codegen failures abort rather than being recorded: this only runs when
/// someone is running the Node suite, so a panic here should stop the setup
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
        let custom = ctx.crate_dir.join(fixture).join("customizable");
        codegen_fixture(&ctx.fixtures_root, fixture, &ctx.crate_dir, &custom);
    }
}

fn codegen_fixture(fixtures_root: &Path, fixture: &str, crate_dir: &Path, custom: &Path) {
    let loaded = load_fixture(fixtures_root, fixture);
    let fixture_root = crate_dir.join(fixture);
    let node = fixture_root.join("generated").join("node");
    fs::create_dir_all(node.join("baml_sdk")).unwrap();

    let output = sdkgen_typescript::to_source_code_with_bytecode(
        &loaded.pool,
        &loaded.baml_bytecode,
        NamingConvention::PreserveCase,
    );
    write_codegen_output(&node.join("baml_sdk"), output, fixture);

    let package_name = format!("sdk-tests-typescript-{}", fixture.replace('_', "-"));
    let mut overlay = Overlay::new(&fixture_root);
    // Copied (not symlinked): node follows symlinks during module resolution
    // and would resolve `node_modules` from `customizable/`, which has none.
    overlay.copy_tree(custom, "node");
    overlay.file("node/test_runtime.ts", TEST_RUNTIME);
    overlay.file(
        "package.json",
        PACKAGE_JSON_TEMPLATE.replace("__PACKAGE_NAME__", &package_name),
    );
    overlay.file("tsconfig.node.json", TSCONFIG_JSON);
    overlay.file("vitest.node.config.ts", VITEST_NODE_CONFIG);
    overlay.install();
}
