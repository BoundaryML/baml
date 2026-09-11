//! Rust sdk-test codegen.
//!
//! [`run_all`] codegens every shared fixture into
//! `crates/rust/<fixture>/generated/` (a standalone Cargo crate emitted by
//! `sdkgen_rust` — manifest at the root, sources under `src/`), symlinks
//! `crates/rust/<fixture>/customizable/*` into `generated/customizable/`, and
//! writes the `generated/tests/main.rs` gate file.
//!
//! ## Test gating
//!
//! Unlike pytest, a Rust test file referencing a symbol the generator does
//! not emit yet fails to *compile*, taking the whole suite down with it. So
//! ported test files are not compiled directly from `tests/`: the only
//! auto-discovered integration-test target is `tests/main.rs`, and each
//! ported file under `customizable/` is compiled only if `main.rs` declares
//! it as a `#[path]` module. `TEST_MODS` is the single source of truth —
//! enabling a capability's tests is a one-line-per-file flip from
//! `Gate::Later` to `Gate::Now`. Gated-off files still land on disk
//! (and in `generated/customizable/`) so the cross-language suite checker
//! sees the full python-matching file set.
//!
//! ## Build cost
//!
//! Each fixture crate path-depends on `baml_bridge` and therefore compiles
//! the BEX runtime stack. All fixtures share one cargo build directory —
//! `<workspace>/target/sdk-rust-target` — threaded through the tests as
//! `CARGO_TARGET_DIR` (the same `run_test_cmd` plumbing python uses for
//! `UV_CACHE_DIR`), so that stack compiles once, not per fixture.
//! `crates/rust/setup.sh` pre-warms it serially before nextest fans out.

use std::{env, fs, path::Path};

use sdk_test_harness_runner::{fixtures, rust::GENERATED_EDITION};
use sdkgen_rust::{NamingConvention, RustGenOptions};

use crate::{
    CodegenCtx, load_fixture, symlink_customizable, write_codegen_output, write_if_changed,
};

/// Dependency spec wiring each fixture crate to the local `baml_bridge`
/// sources. 5 ancestors up from `crates/rust/<F>/generated/Cargo.toml`:
/// `generated` → `<F>` → `rust` → `crates` → `sdk_tests` → `baml_language`
/// (same relative depth as python's `[tool.uv.sources]` and node's
/// `file:` dep).
const BAML_BRIDGE_DEP: &str = r#"{ path = "../../../../../sdks/rust/bridge_rust" }"#;

/// Appended verbatim to each generated `Cargo.toml`
/// (`RustGenOptions::manifest_extra`): dependencies of the ported test
/// suite, not of the generated SDK itself.
const MANIFEST_EXTRA: &str = r#"[dev-dependencies]
tokio = { version = "1", features = ["rt", "macros"] }
"#;

/// Whether a ported test file is compiled into the fixture's test suite.
enum Gate {
    /// Declared as a module in `tests/main.rs` — compiles and runs.
    Now,
    /// Listed as a `// LATER(<reason>): …` comment in `tests/main.rs`.
    /// The reason names the capability the file is waiting on.
    Later(&'static str),
}

/// One row per ported test file: (fixture, path relative to that fixture's
/// `customizable/`, gate). File names match the python suite byte-for-byte
/// (`.py` → `.rs`) for the cross-language suite checker.
const TEST_MODS: &[(&str, &str, Gate)] = &[
    ("docstrings_etc", "test_main.rs", Gate::Now),
    (
        "function_calls",
        "optional_args_static.rs",
        Gate::Later("compile-fail probes need a trybuild-style harness"),
    ),
    ("function_calls", "test_main.rs", Gate::Now),
    (
        "function_calls",
        "test_cancellation.rs",
        Gate::Later("needs cancellation"),
    ),
    (
        "function_calls",
        "test_errors.rs",
        Gate::Later("needs rich error decoding"),
    ),
    ("function_calls", "test_generic_calls.rs", Gate::Now),
    // Intentionally empty: the Rust SDK does no inference (rustc solves type
    // params at compile time; bindings are always sent explicitly).
    ("function_calls", "test_generic_inference.rs", Gate::Now),
    ("function_calls", "test_host_callables.rs", Gate::Now),
    // Rust-only: typed error surfaces from callback-throws inference (python/TS
    // erase `throws`, so there is no cross-language counterpart).
    ("function_calls", "test_callback_throws.rs", Gate::Now),
    ("function_calls", "test_methods_on_classes.rs", Gate::Now),
    (
        "function_calls",
        "test_optional_args.rs",
        Gate::Later("needs the optional-arg matrix and methods on classes"),
    ),
    (
        "function_calls",
        "test_json.rs",
        Gate::Later("needs a canonical baml.json.json projection in sdkgen_rust"),
    ),
    ("function_calls", "test_raises.rs", Gate::Now),
    (
        "function_calls",
        "test_stdlib_entrypoints.rs",
        Gate::Later("needs stdlib entry points"),
    ),
    ("llm_functions", "replay_harness.rs", Gate::Now),
    ("llm_functions", "test_main.rs", Gate::Now),
    ("llm_functions", "test_streaming_e2e.rs", Gate::Now),
    ("type_shapes", "test_main.rs", Gate::Now),
    ("type_shapes", "test_complex_models.rs", Gate::Now),
    ("type_shapes", "test_generic.rs", Gate::Now),
    ("type_shapes", "roundtrip_tests/test_aliases.rs", Gate::Now),
    (
        "type_shapes",
        "roundtrip_tests/test_class_refs.rs",
        Gate::Now,
    ),
    ("type_shapes", "roundtrip_tests/test_enums.rs", Gate::Now),
    // `GNode<T>`'s round trip is a permanent DIVERGENCE (param used only
    // recursively — not representable as a Rust struct); the rest runs.
    (
        "type_shapes",
        "roundtrip_tests/test_forward_refs.rs",
        Gate::Now,
    ),
    ("type_shapes", "roundtrip_tests/test_generics.rs", Gate::Now),
    (
        "type_shapes",
        "roundtrip_tests/test_handles.rs",
        Gate::Later("needs handle-backed stdlib types"),
    ),
    ("type_shapes", "roundtrip_tests/test_lists.rs", Gate::Now),
    (
        "type_shapes",
        "roundtrip_tests/test_literals.rs",
        Gate::Later("needs literal types"),
    ),
    ("type_shapes", "roundtrip_tests/test_maps.rs", Gate::Now),
    (
        "type_shapes",
        "roundtrip_tests/test_media.rs",
        Gate::Later("needs media types"),
    ),
    ("type_shapes", "roundtrip_tests/test_optional.rs", Gate::Now),
    (
        "type_shapes",
        "roundtrip_tests/test_primitives.rs",
        Gate::Now,
    ),
    (
        "type_shapes",
        "roundtrip_tests/test_recursion.rs",
        Gate::Now,
    ),
    ("type_shapes", "roundtrip_tests/test_routing.rs", Gate::Now),
    ("type_shapes", "roundtrip_tests/test_streams.rs", Gate::Now),
    (
        "type_shapes",
        "roundtrip_tests/test_symbol_collisions.rs",
        Gate::Now,
    ),
    ("type_shapes", "roundtrip_tests/test_unions.rs", Gate::Now),
    ("type_shapes", "roundtrip_tests/test_void.rs", Gate::Now),
];

/// Generate every Rust fixture SDK. Called by the `rust` subcommand of the
/// `sdk_test_codegen` binary, which `crates/rust/setup.sh` runs before its
/// per-fixture `cargo test --no-run` pre-warm.
///
/// Codegen failures abort rather than being recorded: this only runs when
/// someone is running the Rust suite, so a panic here should stop the setup
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
    // `load_fixture` panics on .baml compile errors / missing baml_src /
    // empty fixture — those are author bugs in our repo, not env issues,
    // so the hard failure is kept (same policy as python_pydantic2).
    let loaded = load_fixture(fixtures_root, fixture);
    let fixture_root = crate_dir.join(fixture);
    let generated = fixture_root.join("generated");

    // Rust is the one target whose overlay lands *inside* the output writer's
    // own tree, and the writer refuses to run over symlinks it does not own.
    // So the links are cleared here and re-staged below, rather than being
    // tracked by an `Overlay` like every other generator. That costs nothing:
    // re-creating a symlink leaves the file it points at — and therefore what
    // cargo fingerprints — untouched. `Cargo.lock`, `tests/` and the writer's
    // ownership manifest all survive.
    let customizable_link_root = generated.join("customizable");
    if customizable_link_root.exists() {
        fs::remove_dir_all(&customizable_link_root).unwrap();
    }
    let options = RustGenOptions {
        naming_convention: NamingConvention::PreserveCase,
        package_name: format!("sdk-tests-rust-{}", fixture.replace('_', "-")),
        runtime_dep: BAML_BRIDGE_DEP.to_string(),
        manifest_extra: Some(MANIFEST_EXTRA.to_string()),
        edition: GENERATED_EDITION.to_string(),
    };
    let pool = loaded.pool;
    let baml_bytecode = loaded.baml_bytecode;
    let output = sdkgen_rust::to_source_code_with_bytecode(&pool, &baml_bytecode, &options);
    // Skipped symbols are the expected state while the generator's type
    // coverage grows, so summarize instead of one line per symbol (stdlib
    // pools alone would produce dozens per fixture).
    if !output.warnings.is_empty() {
        eprintln!(
            "warning: sdkgen_rust skipped {} unsupported symbol(s) in fixture `{fixture}`",
            output.warnings.len()
        );
        if env::var_os("SDKGEN_SKIP_REASONS").is_some() {
            for warning in &output.warnings {
                eprintln!("  skip {}: {}", warning.fqn, warning.reason);
            }
        }
    }
    write_codegen_output(
        &generated,
        output
            .files
            .into_iter()
            .map(|(path, content)| (path, content.into_bytes())),
        fixture,
    );

    // Overlay ported tests: customizable/ → generated/customizable/. They
    // are NOT placed under tests/ — cargo would auto-discover each file as
    // its own integration-test target and compile gated-off ports.
    let custom = fixture_root.join("customizable");
    if custom.exists() {
        fs::create_dir_all(&customizable_link_root).unwrap();
        symlink_customizable(&custom, &customizable_link_root);
    }

    let tests = generated.join("tests");
    fs::create_dir_all(&tests)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", tests.display()));
    write_if_changed(
        &tests.join("main.rs"),
        render_tests_main(fixture).as_bytes(),
    );
}

/// Render `tests/main.rs` — the single integration-test entry point and
/// the gating root (see module docs). Rows come from [`TEST_MODS`].
///
/// Like every generated Rust file, the code is built as a `TokenStream`
/// and pretty-printed through [`sdkgen_rust::render_rust_file`]; only the
/// comment block (banner + `LATER` rows, which tokens cannot carry)
/// travels as the string header. The `use baml_sdk as _;` item forces the
/// generated SDK library to build and link even while no test module is
/// enabled.
fn render_tests_main(fixture: &str) -> String {
    let mut header = String::new();
    header.push_str("// Generated by sdk_test_codegen::rust::run_all — do not edit.\n");
    header.push_str("// To enable a gated-off port, flip its row in the TEST_MODS table in\n");
    header.push_str("// sdk_tests/codegen/src/rust.rs.\n");
    let mut items = quote::quote! {
        use baml_sdk as _;
    };
    // rustfmt reorders `mod` declarations alphabetically
    // (`reorder_modules` is on by default) and this file is under the
    // suite's own `rustfmt --check` gate, so emit them pre-sorted.
    let mut enabled: Vec<(String, &str)> = Vec::new();
    for (fx, rel, gate) in TEST_MODS {
        if *fx != fixture {
            continue;
        }
        let mod_name = rel.trim_end_matches(".rs").replace('/', "_");
        match gate {
            Gate::Now => enabled.push((mod_name, rel)),
            Gate::Later(reason) => {
                header.push_str(&format!("// LATER({reason}): mod {mod_name};\n"));
            }
        }
    }
    enabled.sort();
    for (mod_name, rel) in enabled {
        let mod_ident = proc_macro2::Ident::new(&mod_name, proc_macro2::Span::call_site());
        let path = format!("../customizable/{rel}");
        items.extend(quote::quote! {
            #[path = #path]
            mod #mod_ident;
        });
    }
    header.push('\n');
    sdkgen_rust::render_rust_file(&header, items)
}
