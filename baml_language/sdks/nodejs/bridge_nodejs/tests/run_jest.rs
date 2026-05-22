//! Runs the Jest test suite for bridge_nodejs.
//! Mirrors bridge_python/tests/run_pytest.rs.

use std::process::Command;

#[test]
#[ignore]
fn jest() {
    let manifest_dir = std::env!("CARGO_MANIFEST_DIR");

    // Step 1: Install npm dependencies
    let install = Command::new("pnpm")
        .arg("install")
        .current_dir(manifest_dir)
        .status()
        .expect("failed to run pnpm install");
    assert!(install.success(), "pnpm install failed");

    // Step 2: Build native addon + TypeScript
    let build = Command::new("pnpm")
        .args(["build:debug"])
        .current_dir(manifest_dir)
        .status()
        .expect("failed to run pnpm build:debug");
    assert!(build.success(), "pnpm build:debug failed");

    // Step 3: Run Jest tests
    let test = Command::new("pnpm")
        .arg("test")
        .current_dir(manifest_dir)
        .status()
        .expect("failed to run pnpm test");
    assert!(test.success(), "Jest tests failed");
}
