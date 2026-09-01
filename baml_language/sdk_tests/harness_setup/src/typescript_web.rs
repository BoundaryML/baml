//! Browser and Cloudflare Workers TypeScript SDK test generation.
//!
//! This package owns no checked-in TypeScript tests. It copies the canonical
//! corpus from the sibling `sdk_tests/crates/typescript` package into its own
//! ignored Web and Workers generated trees.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use sdkgen_typescript_shared::{sdkgen_typescript::NamingConvention, sdkgen_typescript_web};

use super::typescript::{
    CACHE_ENV_VAR, CACHE_SUBDIR, TEST_RUNTIME, clean_generated, copy_customizable,
    has_vitest_tests, rewrite_test_bridge_imports,
};
use crate::{
    BuildDiagnostics, discover_fixtures, emit_cargo_line, fixtures_root_from_manifest,
    load_fixture, watch_dir, write_codegen_output,
};

const PACKAGE_JSON_TEMPLATE: &str = include_str!("templates/package_web.json");
const TSCONFIG_WEB_JSON: &str = include_str!("templates/tsconfig_web.json");
const TSCONFIG_WORKERS_JSON: &str = include_str!("templates/tsconfig_workers.json");
const VITEST_WEB_CONFIG: &str = include_str!("templates/vitest_web.config.ts");
const VITEST_WORKERS_CONFIG: &str = include_str!("templates/vitest_workers.config.ts");
const VITEST_INTEGRATION_CONFIG: &str = include_str!("templates/vitest_integration.config.ts");
const WORKER_STARTUP_TEST: &str = include_str!("templates/worker_startup.test.ts");
const SETUP_ENV_VAR: &str = "SDK_TEST_TYPESCRIPT_WEB_SETUP";

pub fn run_all_from_typescript_sources(relative_sources: &str) {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let sources_root = manifest_dir.join(relative_sources);
    let fixtures_root = fixtures_root_from_manifest(&manifest_dir);
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let fixtures = discover_fixtures(&fixtures_root);
    assert!(
        !fixtures.is_empty(),
        "no fixtures discovered under {}",
        fixtures_root.display()
    );
    assert!(
        sources_root.is_dir(),
        "canonical TypeScript test source not found at {}",
        sources_root.display()
    );

    let mut diagnostics = BuildDiagnostics::new(&out_dir);
    let mut fixture_tests = Vec::new();
    for fixture in &fixtures {
        let custom = sources_root.join(fixture).join("customizable");
        fixture_tests.push(codegen_fixture(
            &fixtures_root,
            fixture,
            &manifest_dir,
            &custom,
            &mut diagnostics,
        ));
    }

    write_fixtures_tests_rs(&out_dir, &fixture_tests);
    diagnostics.finalize();

    emit_cargo_line(format_args!("cargo:rerun-if-changed=build.rs"));
    watch_dir(&fixtures_root);
    for fixture in &fixtures {
        watch_dir(&sources_root.join(fixture).join("customizable"));
    }
}

struct FixtureTests {
    name: String,
    has_web_tests: bool,
    has_workers_tests: bool,
}

fn codegen_fixture(
    fixtures_root: &Path,
    fixture: &str,
    manifest_dir: &Path,
    custom: &Path,
    diagnostics: &mut BuildDiagnostics,
) -> FixtureTests {
    let loaded = load_fixture(fixtures_root, fixture);
    let generated = manifest_dir.join(fixture).join("generated");
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
    write_codegen_output(&web.join("baml_sdk"), output.clone(), fixture, diagnostics);
    write_codegen_output(&workers.join("baml_sdk"), output, fixture, diagnostics);

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
        ("worker_startup.test.ts", WORKER_STARTUP_TEST.to_string()),
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
            r#"import "./workers/baml_sdk/index.js";

export default {
  fetch() {
    return new Response("sdk-test-typescript-workers");
  },
};
"#
            .to_string(),
        ),
    ];
    for (relative, contents) in files {
        if let Err(error) = fs::write(generated.join(relative), contents) {
            diagnostics.record("package_json_write", fixture, error);
        }
    }

    FixtureTests {
        name: fixture.to_string(),
        has_web_tests: has_vitest_tests(&web),
        has_workers_tests: has_vitest_tests(&workers),
    }
}

fn write_fixtures_tests_rs(out_dir: &Path, fixtures: &[FixtureTests]) {
    let mut buffer = String::new();
    buffer.push_str("// Generated by sdk_test_harness_setup::typescript_web::run_all_from_typescript_sources — do not edit.\n");
    buffer.push_str("::sdk_test_harness_runner::build_diagnostics!();\n");
    buffer.push_str(&format!(
        "::sdk_test_harness_runner::setup_guard!({SETUP_ENV_VAR:?});\n"
    ));

    for fixture in fixtures {
        let name = &fixture.name;
        buffer.push_str(&format!(
            r#"
mod {name} {{
    fn cmd(command: &str) {{
        ::sdk_test_harness_runner::run_test_cmd(
            "{name}",
            command,
            "{CACHE_SUBDIR}",
            "{CACHE_ENV_VAR}",
        );
    }}

    #[test]
    fn esm_web() {{
        ::sdk_test_harness_runner::assert_typescript_web_generated_esm("{name}", "web");
    }}

    #[test]
    fn esm_workers() {{
        ::sdk_test_harness_runner::assert_typescript_web_generated_esm("{name}", "workers");
    }}

    #[test]
    fn tsc_web() {{
        cmd("node node_modules/typescript/bin/tsc --noEmit --project tsconfig.web.json");
    }}

    #[test]
    fn tsc_workers() {{
        cmd("node node_modules/typescript/bin/tsc --noEmit --project tsconfig.workers.json");
    }}
"#
        ));
        if fixture.has_web_tests {
            buffer.push_str(
                r#"
    #[test]
    fn vitest_web() {
        cmd("pnpm exec vitest run --config vitest.web.config.ts");
    }
"#,
            );
        }
        buffer.push_str(
            r#"
    #[test]
    fn vitest_workers() {
"#,
        );
        if fixture.has_workers_tests {
            buffer.push_str(
                r#"        cmd("pnpm exec vitest run --config vitest.workers.config.ts");
"#,
            );
        }
        buffer.push_str(
            r#"        cmd("pnpm exec vitest run --config vitest.integration.config.ts");
    }
"#,
        );
        buffer.push_str("\n}\n");
    }

    fs::write(out_dir.join("typescript_web_tests.rs"), buffer).unwrap();
}
