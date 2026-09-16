//! Python + pydantic2 sdk-test codegen.
//!
//! [`run_all`] codegens every shared fixture into
//! `crates/python_pydantic2/<fixture>/generated/baml_sdk/`, symlinks that
//! fixture's `customizable/` overlay of ported tests into the generated tree,
//! and writes a per-fixture `pyproject.toml`.
//!
//! `uv sync` is NOT run here — it lives in
//! `crates/python_pydantic2/setup.sh`, which is also what runs this. Keeping
//! it there lets setup.sh pass `--reinstall-package baml_bridge` so the
//! maturin-built `.so` is rebuilt on incremental Rust edits (a plain
//! `uv sync` skips it, leaving a stale `.so`).

use std::{fs, path::Path};

use sdk_test_harness_runner::fixtures;
use sdkgen_python_pydantic2::NamingConvention;

use crate::{CodegenCtx, Overlay, load_fixture, write_codegen_output};

/// uv-friendly pyproject template. Each fixture's pyproject gets a
/// unique `name` substituted in for `__PYPROJECT_NAME__`. `baml_bridge`
/// is wired to the local `sdks/python/` source via
/// `[tool.uv.sources]` — the relative path is 5 ancestors up from
/// `crates/python_pydantic2/<F>/generated/pyproject.toml`:
/// `generated` → `<F>` → `python_pydantic2` → `crates` → `sdk_tests`
/// → `baml_language`. `pytest-asyncio` + `asyncio_mode = "auto"` are
/// included universally — harmless for fixtures without async tests.
/// Lives at `src/templates/pyproject.toml` so editors give it real
/// TOML syntax highlighting + tooling validation.
const PYPROJECT_TEMPLATE: &str = include_str!("templates/pyproject.toml");

/// Generate every Python fixture SDK. Called by the `python_pydantic2`
/// subcommand of the `sdk_test_codegen` binary, which
/// `crates/python_pydantic2/setup.sh` runs before `uv sync`.
///
/// Codegen failures abort rather than being recorded: this only runs when
/// someone is running the Python suite, so a panic here should stop the setup
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

    fs::create_dir_all(&baml_sdk).unwrap();

    let output = sdkgen_python_pydantic2::to_source_code_with_bytecode(
        &loaded.pool,
        &loaded.baml_bytecode,
        NamingConvention::PreserveCase,
    );
    write_codegen_output(&baml_sdk, output, fixture);

    let pyproject_name = format!("sdk-tests-python-pydantic2-{}", fixture.replace('_', "-"));
    let mut overlay = Overlay::new(&fixture_root);
    // Symlinked so editing a ported test is picked up without re-staging;
    // nested dirs (`roundtrip_tests/`) mirror across so pytest discovers them.
    overlay.link_tree(&fixture_root.join("customizable"), "");
    overlay.file(
        "pyproject.toml",
        PYPROJECT_TEMPLATE.replace("__PYPROJECT_NAME__", &pyproject_name),
    );
    overlay.install();
}
