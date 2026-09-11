//! Browser and Cloudflare Workers TypeScript SDK test generation.
//!
//! This package owns no checked-in TypeScript tests. It copies the canonical
//! corpus from the sibling `sdk_tests/crates/typescript` package into its own
//! ignored Web and Workers generated trees.

use std::{fs, path::Path};

use sdk_test_harness_runner::fixtures;
use sdkgen_typescript_shared::{sdkgen_typescript::NamingConvention, sdkgen_typescript_web};

use super::typescript::{
    TEST_RUNTIME, clean_generated, copy_customizable, rewrite_test_bridge_imports, write_all,
};
use crate::{CodegenCtx, load_fixture, write_codegen_output};

const PACKAGE_JSON_TEMPLATE: &str = include_str!("templates/package_web.json");
const TSCONFIG_WEB_JSON: &str = include_str!("templates/tsconfig_web.json");
const TSCONFIG_WORKERS_JSON: &str = include_str!("templates/tsconfig_workers.json");
const VITEST_WEB_CONFIG: &str = include_str!("templates/vitest_web.config.ts");
const VITEST_WORKERS_CONFIG: &str = include_str!("templates/vitest_workers.config.ts");
const VITEST_INTEGRATION_CONFIG: &str = include_str!("templates/vitest_integration.config.ts");
const WORKER_STARTUP_TEST: &str = include_str!("templates/worker_startup.test.ts");

/// Generate every Web and Workers fixture SDK. Called by the
/// `typescript_web` subcommand of the `sdk_test_codegen` binary, which
/// `crates/typescript_web/setup.sh` runs before `pnpm install`.
///
/// This crate owns no checked-in TypeScript tests: the canonical corpus lives
/// in the sibling `crates/typescript` package, and each fixture's overlay is
/// copied from there into local Web and Workers trees. Nothing is ever written
/// back into the sibling.
pub fn run_all(ctx: &CodegenCtx) {
    let discovered = fixtures::discover_shared(&ctx.fixtures_root);
    assert_eq!(
        discovered,
        fixtures::SHARED,
        "the fixture corpus at {} has drifted from `fixtures::SHARED`",
        ctx.fixtures_root.display()
    );

    let sources_root = ctx
        .crate_dir
        .parent()
        .unwrap_or_else(|| unreachable!("a generator crate is not under sdk_tests/crates"))
        .join("typescript");
    assert!(
        sources_root.is_dir(),
        "canonical TypeScript test source not found at {}",
        sources_root.display()
    );

    for fixture in fixtures::SHARED {
        let custom = sources_root.join(fixture).join("customizable");
        codegen_fixture(&ctx.fixtures_root, fixture, &ctx.crate_dir, &custom);
    }
}

fn codegen_fixture(fixtures_root: &Path, fixture: &str, crate_dir: &Path, custom: &Path) {
    let loaded = load_fixture(fixtures_root, fixture);
    let generated = crate_dir.join(fixture).join("generated");
    clean_generated(&generated);
    let web = generated.join("web");
    let workers = generated.join("workers");
    fs::create_dir_all(web.join("baml_sdk")).unwrap();
    fs::create_dir_all(workers.join("baml_sdk")).unwrap();

    let output = sdkgen_typescript_web::to_source_code_with_bytecode(
        &loaded.pool,
        &loaded.baml_bytecode,
        NamingConvention::PreserveCase,
    );
    write_codegen_output(&web.join("baml_sdk"), output.clone(), fixture);
    write_codegen_output(&workers.join("baml_sdk"), output, fixture);

    for runtime in [&web, &workers] {
        if custom.exists() {
            copy_customizable(custom, runtime);
        }
        fs::write(runtime.join("test_runtime.ts"), TEST_RUNTIME).unwrap();
        rewrite_test_bridge_imports(runtime);
    }

    let package_name = format!("sdk-tests-typescript-web-{}", fixture.replace('_', "-"));
    let files = [
        (
            "package.json",
            PACKAGE_JSON_TEMPLATE.replace("__PACKAGE_NAME__", &package_name),
        ),
        ("tsconfig.web.json", TSCONFIG_WEB_JSON.to_string()),
        ("tsconfig.workers.json", TSCONFIG_WORKERS_JSON.to_string()),
        ("vitest.web.config.ts", VITEST_WEB_CONFIG.to_string()),
        (
            "vitest.workers.config.ts",
            VITEST_WORKERS_CONFIG.to_string(),
        ),
        (
            "vitest.integration.config.ts",
            VITEST_INTEGRATION_CONFIG.to_string(),
        ),
        (
            "worker_startup.test.ts",
            WORKER_STARTUP_TEST.replace(
                "__EXPECTED_BODY__",
                if fixture == "function_calls" {
                    "hello world"
                } else {
                    "sdk-test-typescript-workers"
                },
            ),
        ),
        (
            "wrangler.jsonc",
            r#"{
  "$schema": "node_modules/wrangler/config-schema.json",
  "name": "sdk-test-typescript-workers",
  "main": "worker.js",
  "compatibility_date": "2026-07-15",
  "compatibility_flags": ["nodejs_compat"]
}
"#
            .to_string(),
        ),
        (
            "worker.js",
            if fixture == "function_calls" {
                r#"import { worker_runtime_smoke } from "./workers/baml_sdk/index.js";

export default {
  fetch() {
    return new Response(worker_runtime_smoke());
  },
};
"#
            } else {
                r#"import "./workers/baml_sdk/index.js";

export default {
  fetch() {
    return new Response("sdk-test-typescript-workers");
  },
};
"#
            }
            .to_string(),
        ),
    ];
    write_all(&generated, files);
}
