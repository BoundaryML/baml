//! Rust sdk-test target — build-script side. Mirrors `python_pydantic2`.
//!
//! [`run_all`] is called from `crates/rust/build.rs`. It discovers every
//! fixture under `sdk_tests/fixtures/`, codegens each into
//! `crates/rust/<fixture>/generated/` (a standalone Cargo crate emitted by
//! `sdkgen_rust` — manifest at the root, sources under `src/`), symlinks
//! `crates/rust/<fixture>/customizable/*` into `generated/customizable/`,
//! writes the `generated/tests/main.rs` gate file, and emits
//! `OUT_DIR/rust_tests.rs` — the per-fixture `#[test]` scaffold the
//! `sdk_test_harness_runner::rust::test_suite!()` macro `include!`s.
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
//! `<workspace>/target/sdk-rust-target` — threaded through the emitted
//! tests as `CARGO_TARGET_DIR` (the same `run_test_cmd` plumbing python
//! uses for `UV_CACHE_DIR`), so that stack compiles once, not per fixture.
//! `crates/rust/setup.sh` pre-warms it serially before nextest fans out.

use std::{
    env, fs, panic,
    path::{Path, PathBuf},
};

use sdkgen_rust::{NamingConvention, RustGenOptions};

use crate::{
    BuildDiagnostics, discover_fixtures, emit_cargo_line, fixtures_root_from_manifest,
    load_fixture, symlink_customizable, watch_dir, write_codegen_output,
};

/// Shared cargo build dir for ALL fixture crates, as a subdir of
/// `<workspace>/target/`. Kept out of the workspace's own build dir to
/// avoid file-lock contention with developer / rust-analyzer builds and
/// fingerprint interleaving with workspace feature unification.
const CACHE_SUBDIR: &str = "sdk-rust-target";
const CACHE_ENV_VAR: &str = "CARGO_TARGET_DIR";

/// Env var the setup scripts write to `$NEXTEST_ENV` and the emitted
/// `setup_guard::ran` test checks for. Must stay in sync with both
/// `crates/rust/setup.sh` and `setup.ps1`.
const SETUP_ENV_VAR: &str = "SDK_TEST_RUST_SETUP";

/// Edition stamped into every generated fixture crate — also the
/// `--edition` the scaffold's `rustfmt` gate passes, so keep the two uses
/// flowing from this one const.
const GENERATED_EDITION: &str = "2024";

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
    (
        "llm_functions",
        "replay_harness.rs",
        Gate::Later("support module for the streaming tests, not a test file"),
    ),
    (
        "llm_functions",
        "test_main.rs",
        Gate::Later("needs LLM functions and $stream companions"),
    ),
    (
        "llm_functions",
        "test_streaming_e2e.rs",
        Gate::Later("needs streaming"),
    ),
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
    (
        "type_shapes",
        "roundtrip_tests/test_streams.rs",
        Gate::Later("needs $stream companion types"),
    ),
    (
        "type_shapes",
        "roundtrip_tests/test_symbol_collisions.rs",
        Gate::Now,
    ),
    ("type_shapes", "roundtrip_tests/test_unions.rs", Gate::Now),
    ("type_shapes", "roundtrip_tests/test_void.rs", Gate::Now),
];

/// Entry point for `crates/rust/build.rs`. Drives codegen across every
/// fixture and emits the per-fixture test scaffold to OUT_DIR.
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

    // Toolchain pre-warm (`cargo test --no-run` per fixture) is NOT run
    // here — it lives in `crates/rust/setup.sh`, fired by `cargo nextest
    // run` (see module docs), so `cargo check` / `cargo doc` of the
    // workspace never build the fixture crates.
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
    // empty fixture — those are author bugs in our repo, not env issues,
    // so the hard failure is kept (same policy as python_pydantic2).
    let loaded = load_fixture(fixtures_root, fixture);
    let fixture_root = manifest_dir.join(fixture);
    let generated = fixture_root.join("generated");

    // The shared writer owns generated SDK files and removes stale ones.
    // Clear only harness overlays; preserve Cargo.lock and the writer's
    // ownership manifest across rebuilds.
    if generated.exists() {
        for overlay in ["customizable", "tests"] {
            let path = generated.join(overlay);
            if path.is_dir() {
                fs::remove_dir_all(path).unwrap();
            } else if path.exists() {
                fs::remove_file(path).unwrap();
            }
        }
    }
    let options = RustGenOptions {
        naming_convention: NamingConvention::PreserveCase,
        package_name: format!("sdk-tests-rust-{}", fixture.replace('_', "-")),
        runtime_dep: BAML_BRIDGE_DEP.to_string(),
        manifest_extra: Some(MANIFEST_EXTRA.to_string()),
        edition: GENERATED_EDITION.to_string(),
    };
    let pool = loaded.pool;
    let interface_implementors = loaded.interface_implementors;
    let baml_bytecode = loaded.baml_bytecode;
    let codegen_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        sdkgen_rust::to_source_code_with_bytecode_and_interface_implementors(
            &pool,
            &interface_implementors,
            &baml_bytecode,
            &options,
        )
    }));
    match codegen_result {
        Ok(output) => {
            // Skipped symbols are the expected state while the generator's
            // type coverage grows, so summarize instead of one warning per
            // symbol (stdlib pools alone would produce dozens per fixture).
            if !output.warnings.is_empty() {
                emit_cargo_line(format_args!(
                    "cargo:warning=sdkgen_rust skipped {} unsupported symbol(s) in fixture `{fixture}`",
                    output.warnings.len()
                ));
                if env::var("SDKGEN_SKIP_REASONS").is_ok() {
                    for warning in &output.warnings {
                        emit_cargo_line(format_args!(
                            "cargo:warning=  skip {}: {}",
                            warning.fqn, warning.reason
                        ));
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
                diagnostics,
            );
        }
        Err(payload) => {
            // Surface the panic message: a codegen panic with an opaque
            // record is only diagnosable by re-running codegen by hand.
            let message = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("<non-string panic payload>");
            diagnostics.record(
                "codegen",
                fixture,
                format!("sdkgen_rust::to_source_code_with_bytecode panicked: {message}"),
            );
        }
    }

    // Overlay ported tests: customizable/ → generated/customizable/. They
    // are NOT placed under tests/ — cargo would auto-discover each file as
    // its own integration-test target and compile gated-off ports.
    let custom = fixture_root.join("customizable");
    if custom.exists() {
        let dst = generated.join("customizable");
        let symlink_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            fs::create_dir_all(&dst).unwrap();
            symlink_customizable(&custom, &dst);
        }));
        if symlink_result.is_err() {
            diagnostics.record(
                "symlink_customizable",
                fixture,
                format!(
                    "symlink_customizable({}, {}) panicked",
                    custom.display(),
                    dst.display()
                ),
            );
        }
    }

    let tests = generated.join("tests");
    if let Err(e) = fs::create_dir_all(&tests)
        .and_then(|()| fs::write(tests.join("main.rs"), render_tests_main(fixture)))
    {
        diagnostics.record("tests_main_write", fixture, format!("write main.rs: {e}"));
    }
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
    header.push_str("// Generated by sdk_test_harness_setup::rust::run_all — do not edit.\n");
    header.push_str("// To enable a gated-off port, flip its row in the TEST_MODS table in\n");
    header.push_str("// sdk_tests/harness_setup/src/rust.rs.\n");
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

/// Emit `OUT_DIR/rust_tests.rs` — a sequence of
/// `::sdk_test_harness_runner::*` invocations. No test bodies authored
/// here; `build_diagnostics!` and `run_test_cmd` live in
/// `sdk_test_harness_runner`.
///
/// Per fixture: `fmt` checks the hand-ported test files reachable from
/// `tests/main.rs` (rustfmt follows the enabled `mod` declarations;
/// generated `src/` is intentionally not rustfmt-checked — the emitter's
/// pretty-printer is its canonical format), `clippy` lints the generated
/// library, and `cargo_test` compiles and runs the enabled ports.
/// `cargo_test` alone gets `BAML_LIBRARY_PATH`: `baml_bridge` is
/// dylib-only, so the fixture's tests load the engine cdylib at run time
/// (built by the setup script; fmt/clippy never execute the engine).
fn write_fixtures_tests_rs(out_dir: &Path, fixtures: &[String]) {
    let header = "// Generated by sdk_test_harness_setup::rust::run_all — do not edit.\n\n";
    let mut items = quote::quote! {
        ::sdk_test_harness_runner::build_diagnostics!();
        ::sdk_test_harness_runner::setup_guard!(#SETUP_ENV_VAR);

        /// The engine cdylib the generated SDKs load at run time. This
        /// test binary lives in `<target>/<profile>/deps/`, so the
        /// sibling `<target>/<profile>/` is where `cargo build -p
        /// bridge_cffi` put the library — regardless of CARGO_TARGET_DIR
        /// or profile.
        fn engine_library() -> ::std::path::PathBuf {
            let exe = ::std::env::current_exe().expect("current test binary path");
            let profile_dir = exe
                .parent()
                .and_then(::std::path::Path::parent)
                .expect("test binary not under <target>/<profile>/deps");
            let name = if cfg!(target_os = "windows") {
                "bridge_cffi.dll"
            } else if cfg!(target_os = "macos") {
                "libbridge_cffi.dylib"
            } else {
                "libbridge_cffi.so"
            };
            let path = profile_dir.join(name);
            assert!(
                path.is_file(),
                "engine library not found at {} — run `cargo build -p bridge_cffi` first \
                 (the nextest setup script does this automatically)",
                path.display()
            );
            path
        }
    };
    for fixture in fixtures {
        let mod_ident = proc_macro2::Ident::new(fixture, proc_macro2::Span::call_site());
        let fmt_cmd = format!("rustfmt --edition {GENERATED_EDITION} --check tests/main.rs");
        items.extend(quote::quote! {
            mod #mod_ident {
                fn cmd_env(c: &str, extra_env: &[(&str, &str)]) {
                    // Never spawn cargo without the generated manifest in
                    // place: cargo discovers manifests *upward*, so in its
                    // absence a fixture-level `cargo test` would silently
                    // become a workspace-wide one — re-entering this very
                    // test suite and forking cargo processes without bound.
                    // (The `--manifest-path Cargo.toml` pin on the commands
                    // below is the second layer of the same defense.)
                    let manifest = ::std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join(#fixture)
                        .join("generated")
                        .join("Cargo.toml");
                    assert!(
                        manifest.exists(),
                        "{} is missing — codegen failed for this fixture \
                         (see the build_diagnostics test); refusing to run \
                         cargo without it",
                        manifest.display(),
                    );
                    ::sdk_test_harness_runner::run_test_cmd_with_env(
                        #fixture,
                        c,
                        #CACHE_SUBDIR,
                        #CACHE_ENV_VAR,
                        extra_env,
                    );
                }

                fn cmd(c: &str) {
                    cmd_env(c, &[]);
                }

                #[test]
                fn fmt() {
                    cmd(#fmt_cmd);
                }

                #[test]
                fn clippy() {
                    cmd("cargo clippy --manifest-path Cargo.toml -- -D warnings");
                }

                #[test]
                fn cargo_test() {
                    let engine = super::engine_library();
                    cmd_env(
                        "cargo test --manifest-path Cargo.toml",
                        &[
                            (
                                "BAML_LIBRARY_PATH",
                                engine.to_str().expect("engine path is valid UTF-8"),
                            ),
                            ("BAML_LIBRARY_DISABLE_DOWNLOAD", "true"),
                        ],
                    );
                }
            }
        });
    }
    let target = out_dir.join("rust_tests.rs");
    fs::write(&target, sdkgen_rust::render_rust_file(header, items)).unwrap();
}
