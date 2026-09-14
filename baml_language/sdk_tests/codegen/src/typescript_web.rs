//! Browser and Cloudflare Workers TypeScript SDK test generation.
//!
//! This package owns no checked-in TypeScript tests. It copies the canonical
//! corpus from the sibling `sdk_tests/crates/typescript` package into its own
//! ignored Web and Workers generated trees.

use std::{fs, path::Path};

use sdk_test_harness_runner::fixtures;
use sdkgen_typescript_shared::{sdkgen_typescript::NamingConvention, sdkgen_typescript_web};

use super::typescript::TEST_RUNTIME;
use crate::{CodegenCtx, Overlay, load_fixture, write_codegen_output};

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
    let fixture_root = crate_dir.join(fixture);
    let generated = fixture_root.join("generated");
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

    let mut overlay = Overlay::new(&fixture_root);
    for runtime in ["web", "workers"] {
        overlay.copy_tree(custom, runtime);
        overlay.file(format!("{runtime}/test_runtime.ts"), TEST_RUNTIME);
    }
    // The corpus is written against the native bridge; the Web builds import
    // the Wasm one. Only the overlay is rewritten — `baml_sdk/` belongs to the
    // output writer and already emits the right specifier.
    overlay.rewrite_content(|path, contents| {
        if !path.ends_with(".ts") {
            return None;
        }
        let source = std::str::from_utf8(contents).ok()?;
        let rewritten = source.replace("@boundaryml/baml-bridge", "@boundaryml/baml-bridge-web");
        (rewritten != source).then(|| rewritten.into_bytes())
    });

    let package_name = format!("sdk-tests-typescript-web-{}", fixture.replace('_', "-"));
    overlay.file(
        "package.json",
        PACKAGE_JSON_TEMPLATE.replace("__PACKAGE_NAME__", &package_name),
    );
    overlay.file("tsconfig.web.json", TSCONFIG_WEB_JSON);
    overlay.file("tsconfig.workers.json", TSCONFIG_WORKERS_JSON);
    overlay.file("vitest.web.config.ts", VITEST_WEB_CONFIG);
    overlay.file("vitest.workers.config.ts", VITEST_WORKERS_CONFIG);
    overlay.file("vitest.integration.config.ts", VITEST_INTEGRATION_CONFIG);
    overlay.file(
        "worker_startup.test.ts",
        WORKER_STARTUP_TEST.replace(
            "__EXPECTED_BODY__",
            if fixture == "function_calls" {
                "hello world"
            } else {
                "sdk-test-typescript-workers"
            },
        ),
    );
    overlay.file(
        "wrangler.jsonc",
        r#"{
  "$schema": "node_modules/wrangler/config-schema.json",
  "name": "sdk-test-typescript-workers",
  "main": "worker.js",
  "compatibility_date": "2026-07-15",
  "compatibility_flags": ["nodejs_compat"]
}
"#,
    );
    overlay.file(
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
        },
    );
    overlay.install();
}
