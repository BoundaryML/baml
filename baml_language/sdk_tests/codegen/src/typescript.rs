//! Node TypeScript SDK test generation.
//!
//! The canonical TypeScript test corpus lives under `sdk_tests/crates/typescript`.
//! This module emits only the Node SDK, Node package configuration, and Node Rust
//! test scaffold. Browser and Workers generation lives in [`crate::typescript_web`].

use std::{fs, path::Path};

use sdk_test_harness_runner::fixtures;
use sdkgen_typescript_shared::sdkgen_typescript::{self, NamingConvention};

use crate::{CodegenCtx, load_fixture, write_codegen_output};

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
    let generated = crate_dir.join(fixture).join("generated");
    clean_generated(&generated);

    let node = generated.join("node");
    fs::create_dir_all(node.join("baml_sdk")).unwrap();
    let output = sdkgen_typescript::to_source_code_with_bytecode(
        &loaded.pool,
        &loaded.baml_bytecode,
        NamingConvention::PreserveCase,
    );
    write_codegen_output(&node.join("baml_sdk"), output, fixture);
    if custom.exists() {
        copy_customizable(custom, &node);
    }
    fs::write(node.join("test_runtime.ts"), TEST_RUNTIME).unwrap();

    let package_name = format!("sdk-tests-typescript-{}", fixture.replace('_', "-"));
    let files = [
        (
            "package.json",
            PACKAGE_JSON_TEMPLATE.replace("__PACKAGE_NAME__", &package_name),
        ),
        ("tsconfig.node.json", TSCONFIG_JSON.to_string()),
        ("vitest.node.config.ts", VITEST_NODE_CONFIG.to_string()),
    ];
    write_all(&generated, files);
}

/// Write each `(relative path, contents)` into `root`, failing loudly.
pub(crate) fn write_all<const N: usize>(root: &Path, files: [(&str, String); N]) {
    for (relative, contents) in files {
        let path = root.join(relative);
        fs::write(&path, contents)
            .unwrap_or_else(|error| panic!("failed to write {}: {error}", path.display()));
    }
}

pub(crate) fn clean_generated(generated: &Path) {
    if !generated.exists() {
        return;
    }
    for entry in fs::read_dir(generated).unwrap() {
        let path = entry.unwrap().path();
        if path.file_name().and_then(|name| name.to_str()) == Some("node_modules") {
            continue;
        }
        if path.is_dir() {
            fs::remove_dir_all(&path).unwrap();
        } else {
            fs::remove_file(&path).unwrap();
        }
    }
}

pub(crate) fn copy_customizable(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap().flatten() {
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_customizable(&source_path, &destination_path);
        } else {
            fs::copy(&source_path, &destination_path).unwrap_or_else(|error| {
                panic!(
                    "copy {} to {}: {error}",
                    source_path.display(),
                    destination_path.display()
                )
            });
        }
    }
}

pub(crate) fn rewrite_test_bridge_imports(dir: &Path) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            let skip = matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some("baml_sdk") | Some("node_modules")
            );
            if !skip {
                rewrite_test_bridge_imports(&path);
            }
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("ts") {
            let source = fs::read_to_string(&path).unwrap();
            let web_source =
                source.replace("@boundaryml/baml-bridge", "@boundaryml/baml-bridge-web");
            if source != web_source {
                fs::write(path, web_source).unwrap();
            }
        }
    }
}
