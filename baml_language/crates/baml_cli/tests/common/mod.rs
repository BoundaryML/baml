// Shared harness for `baml pack` end-to-end tests.
//
// ============================================================================
// HACK — replace this with artifact deps once stable.
// ============================================================================
//
// These tests need both `baml-cli` (the CLI that does the packing) and
// `baml-pack-host` (the host binary whose bytes get embedded into packaged
// executables). They live in different crates. Cargo's
// `CARGO_BIN_EXE_<name>` env var only exposes binaries from the test's
// *own* crate, so we can't get `baml-pack-host`'s path for free.
//
// The *right* fix is cargo's artifact dependencies (RFC 3028):
//
//     [dev-dependencies]
//     baml_pack_host = { workspace = true, artifact = "bin" }
//
// With that, `env!("CARGO_BIN_FILE_baml_pack_host_baml-pack-host")` would
// give us the binary path, cargo would handle rebuilds, and we'd delete
// this whole file. But `artifact = "bin"` requires `-Z bindeps` and is
// nightly-only as of Rust 1.93 (2026-01). The workspace is pinned to
// stable, so this doesn't work yet.
//
// Until artifact deps stabilize, we shell out to `cargo build` from
// inside the test. Pros: no new dev-deps, real cargo freshness tracking.
// Cons:
//   - First test in a run pays the cargo-build cost (cold: ~1 min; warm:
//     near-instant via cargo's own incremental cache).
//   - Nested cargo invocations share the target-dir build lock; running
//     `cargo test` while another cargo is building the same workspace
//     can stall briefly. In practice this is rare and self-resolving.
//   - Target-dir discovery walks up from `CARGO_MANIFEST_DIR` to find
//     the workspace root; honors `CARGO_TARGET_DIR` if set.
//
// When bindeps stabilizes, delete this module and replace all callers
// with the `CARGO_BIN_FILE_*` env vars. The test code itself (just a
// `Command::new(path)` pattern) won't change.

#![allow(dead_code)] // Shared helpers; individual tests use a subset.
#![allow(unreachable_pub)] // Integration-test module; `pub` items are intentional.

use std::{path::PathBuf, process::Command, sync::OnceLock};

/// Memoized build: `cargo build -p baml_cli -p baml_pack_host` runs at
/// most once per test binary regardless of how many tests call in.
static BUILT: OnceLock<BuiltPaths> = OnceLock::new();

#[derive(Clone, Debug)]
pub struct BuiltPaths {
    pub baml_cli: PathBuf,
    pub baml_pack_host: PathBuf,
}

/// Ensure both binaries are built and return their paths. Subsequent
/// calls reuse the cached result — cargo's own incremental cache handles
/// rebuilds when source files change.
pub fn ensure_built() -> &'static BuiltPaths {
    BUILT.get_or_init(|| {
        // Build for the same profile the test binary is using — otherwise
        // `cargo test --release` would build into `target/debug` while we
        // resolve paths from `target/release` and fail.
        let profile = profile();
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
        let mut build = Command::new(&cargo);
        build.args(["build", "-p", "baml_cli", "-p", "baml_pack_host"]);
        if profile == "release" {
            build.arg("--release");
        }
        let status = build.status().expect("spawn cargo build");
        assert!(
            status.success(),
            "cargo build for baml_cli + baml_pack_host failed — see output above",
        );

        let target = target_dir();
        let bin_dir = target.join(&profile);
        let baml_cli = bin_dir.join(bin_name("baml-cli"));
        let baml_pack_host = bin_dir.join(bin_name("baml-pack-host"));
        assert!(
            baml_cli.exists(),
            "baml-cli not found at {} after build",
            baml_cli.display()
        );
        assert!(
            baml_pack_host.exists(),
            "baml-pack-host not found at {} after build",
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

/// Locate the workspace's `target/` directory.
///
/// Honors `CARGO_TARGET_DIR` if set; otherwise walks up from
/// `CARGO_MANIFEST_DIR` looking for a `Cargo.toml` with `[workspace]`
/// and appends `target/`. Good enough for the standard layouts CI uses.
fn target_dir() -> PathBuf {
    if let Ok(explicit) = std::env::var("CARGO_TARGET_DIR") {
        return PathBuf::from(explicit);
    }
    // CARGO_MANIFEST_DIR is the crate containing these tests: baml_cli.
    // Walk up until we find a workspace manifest, then use its sibling
    // `target/` directory.
    let start = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut cur: &std::path::Path = &start;
    loop {
        let manifest = cur.join("Cargo.toml");
        if manifest.exists() {
            let text = std::fs::read_to_string(&manifest).unwrap_or_default();
            if text.contains("[workspace]") {
                return cur.join("target");
            }
        }
        match cur.parent() {
            Some(p) => cur = p,
            None => panic!(
                "could not locate workspace root walking up from {}",
                start.display()
            ),
        }
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
