// Shared harness for `baml pack` end-to-end tests.
//
// ============================================================================
// HACK — replace `baml-pack-host` discovery with artifact deps once stable.
// ============================================================================
//
// These tests need both `baml-cli` (the CLI that does the packing) and
// `baml-pack-host` (the host binary whose bytes get embedded into packaged
// executables).
//
// `baml-cli` is *this* crate's own bin, so cargo builds it before the
// integration test runs and hands us its path — for the exact profile AND
// feature set the surrounding `cargo test` / nextest invocation used — via
// `env!("CARGO_BIN_EXE_baml-cli")`. We take that pre-built binary instead of
// rebuilding: a `cargo build -p baml_cli` inside the test would relink the
// (huge, ~300-rlib) `baml-cli` under `default` features while CI's outer build
// used `--all-features`, so the two artifact sets would coexist and exhaust the
// Windows runner's disk (`LLVM ERROR: IO failure on output stream: no space on
// device`).
//
// `baml-pack-host` lives in a *different* crate, and cargo's `CARGO_BIN_EXE_*`
// only exposes binaries from the test's own crate — so we still can't get its
// path for free. The *right* fix is cargo's artifact dependencies (RFC 3028):
//
//     [dev-dependencies]
//     baml_pack_host = { workspace = true, artifact = "bin:baml-pack-host" }
//
// With that, `env!("CARGO_BIN_FILE_BAML_PACK_HOST_baml-pack-host")` would give
// us the binary path, cargo would handle rebuilds, and we'd delete the setup
// below. But `artifact = "bin"` requires `-Z bindeps` and is nightly-only as of
// Rust 1.93 (2026-01). The workspace is pinned to stable, so nextest runs one
// filtered setup script that builds the host before it launches any test case.
// Plain `cargo test` has no setup-script facility; its single test process uses
// the fallback build in [`ensure_built`]. `baml pack` locates the host as a
// sibling of the running `baml-cli`, so both paths build it into the same
// `target/<profile>` directory `env!` points at.

#![allow(dead_code)] // Shared helpers; individual tests use a subset.
#![allow(unreachable_pub)] // Integration-test module; `pub` items are intentional.

use std::{path::PathBuf, process::Command, sync::OnceLock};

const SKILL_TEMPLATE: &str = include_str!("../../../../../skills/baml-core/SKILL.md");
const TOOLCHAIN_VERSION_PLACEHOLDER: &str = "{{BAML_TOOLCHAIN_VERSION}}";

pub fn installed_skill_content() -> String {
    SKILL_TEMPLATE.replacen(
        TOOLCHAIN_VERSION_PLACEHOLDER,
        baml_version::CANONICAL_VERSION,
        1,
    )
}

/// Memoized host discovery for this test process. Nextest's filtered setup
/// script prebuilds the host once for the whole run; plain `cargo test` falls
/// back to one build shared by every case in its single process.
static BUILT: OnceLock<BuiltPaths> = OnceLock::new();

#[derive(Clone, Debug)]
pub struct BuiltPaths {
    pub baml_cli: PathBuf,
    pub baml_pack_host: PathBuf,
}

/// Path to this crate's own `baml-cli` binary, as built by the surrounding
/// `cargo test` / nextest invocation (matching profile and feature set). No
/// rebuild — cargo guarantees the bin exists before the integration test runs.
///
/// Tests that only drive `baml-cli` (everything except `baml pack`) should use
/// this instead of [`ensure_built`]: it skips the `baml-pack-host` build they
/// don't need.
pub fn baml_cli() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_baml-cli"))
}

/// Shared on-disk bytecode-cache directory for spawned `baml-cli` invocations.
///
/// Each e2e test drives the CLI against a fresh temp project, so the default
/// cache location (`<project>/.baml/cache`) is always cold and every
/// invocation recompiles the stdlib from scratch — the dominant cost of these
/// suites. The cache is content-addressed and keyed by the compiler
/// fingerprint, so one directory shared across tests, processes, and `cargo
/// test` runs (it lives under `target/tmp`) is safe: the first invocation
/// warms the stdlib entries, every later one serves them, and a rebuilt
/// `baml-cli` invalidates itself via its fingerprint. Tests that assert on
/// cold/warm cache behavior (e.g. `test_list_discovery_cache_e2e`) must keep
/// setting their own isolated `BAML_CACHE_DIR` instead of this one.
pub fn shared_cache_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("baml-cli-e2e-cache")
}

/// Ensure `baml-pack-host` is built next to `baml-cli` and return both paths.
/// Subsequent calls reuse the cached result — cargo's own incremental cache
/// handles rebuilds when source files change.
pub fn ensure_built() -> &'static BuiltPaths {
    BUILT.get_or_init(|| {
        // `baml-cli` comes straight from cargo (no rebuild). `baml pack` reads
        // the host binary as a sibling of the running `baml-cli`, so resolve the
        // host next to it and build it into that same directory.
        let baml_cli = baml_cli();
        let bin_dir = baml_cli
            .parent()
            .expect("CARGO_BIN_EXE_baml-cli should have a parent directory")
            .to_path_buf();

        let baml_pack_host = bin_dir.join(bin_name("baml-pack-host"));
        let setup_prebuilt = std::env::var("BAML_PACK_HOST_PREBUILT").is_ok_and(|v| v == "1");
        if !setup_prebuilt {
            // Plain `cargo test` does not run nextest setup scripts. Build the
            // host for the same profile as the test binary once per process.
            let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
            let mut build = Command::new(&cargo);
            build.args(["build", "-p", "baml_pack_host"]);
            if profile() == "release" {
                build.arg("--release");
            }
            let status = build.status().expect("spawn cargo build");
            assert!(
                status.success(),
                "cargo build for baml_pack_host failed — see output above",
            );
        }

        assert!(
            baml_cli.exists(),
            "baml-cli not found at {} (CARGO_BIN_EXE_baml-cli)",
            baml_cli.display()
        );
        assert!(
            baml_pack_host.exists(),
            "baml-pack-host not found at {}; nextest setup or the plain cargo-test fallback should have built it",
            baml_pack_host.display()
        );
        BuiltPaths {
            baml_cli,
            baml_pack_host,
        }
    })
}

/// `<name>` on Unix, `<name>.exe` on Windows.
fn bin_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Test binaries run under the `debug` profile unless built under
/// `release`. `cfg!(debug_assertions)` is the conventional proxy; the
/// workspace's `release` profile turns `debug_assertions` off.
fn profile() -> String {
    if cfg!(debug_assertions) {
        "debug".to_string()
    } else {
        "release".to_string()
    }
}

/// Write a trivial `baml.toml` + `baml_src/main.baml` layout into `dir`.
/// `baml pack` requires a `[package]` table on the project manifest, so
/// we always emit one — the name is unused since `-o` overrides the
/// artifact location, but the table itself has to be present.
pub fn write_project(dir: &std::path::Path, main_source: &str) {
    std::fs::write(dir.join("baml.toml"), "[package]\nname = \"pack_e2e\"\n").unwrap();
    let src = dir.join("baml_src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("main.baml"), main_source).unwrap();
}

pub fn assert_no_compile_file_status(stderr: &str) {
    for line in stderr.lines() {
        assert!(
            !(line.contains("Compiling ") && line.contains(" file(s)")),
            "unexpected compile file-count status in stderr:\n{stderr}"
        );
        assert!(
            !(line.contains("Compiled ") && line.contains(" file(s)")),
            "unexpected compiled file-count status in stderr:\n{stderr}"
        );
    }
}
