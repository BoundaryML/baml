use std::time::UNIX_EPOCH;

fn main() {
    if std::env::var("BRIDGE_WASM_FORCE_RERUN").is_ok() {
        // Point at a non-existent file so cargo always re-runs this build script,
        // even when only dependency crates changed (not files in bridge_wasm itself).
        println!("cargo:rerun-if-changed=FORCE_RERUN");
    }

    // Build script runs on the host; [`std::time::SystemTime`] is correct at this callsite.
    #[allow(clippy::disallowed_types)]
    let ts = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    println!("cargo:rustc-env=BRIDGE_WASM_BUILD_TS={ts}");
}
