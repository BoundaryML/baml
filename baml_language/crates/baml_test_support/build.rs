//! Compile one shared stdlib artifact per optimization level for Rust tests.

use std::{env, fs, path::PathBuf};

#[path = "build_stdlib_prefix_config.rs"]
mod stdlib_prefix_config;

/// Compile the stdlib once and embed it, so the compile helpers in
/// `src/lib.rs` can splice it in instead of re-deriving it per test.
/// See that module for why this cannot be an in-process cache.
fn generate_stdlib_prefix() {
    use baml_db::stdlib_prefix::{OptLevel, build_stdlib_prefix};

    println!("cargo:rerun-if-changed=build_stdlib_prefix_config.rs");

    let out_dir =
        PathBuf::from(env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR for build scripts"));
    let mut interfaces = None;
    for raw in stdlib_prefix_config::OPT_LEVELS {
        let opt = match raw {
            0 => OptLevel::Zero,
            1 => OptLevel::One,
            2 => OptLevel::Two,
            other => panic!("OPT_LEVELS lists {other}, which is not an OptLevel"),
        };
        let prefix = build_stdlib_prefix(opt);
        // Interfaces do not depend on optimization. Preserve the combined
        // artifact's consistency check while storing each level separately.
        match &interfaces {
            None => interfaces = Some(prefix.interfaces.clone()),
            Some(first) => assert_eq!(
                first, &prefix.interfaces,
                "stdlib package interfaces differ between optimization levels"
            ),
        }
        let bytes = baml_db::stdlib_prefix::encode_artifact(
            &stdlib_prefix_config::artifact_key(raw),
            vec![prefix],
        );
        fs::write(out_dir.join(format!("stdlib_prefix_{raw}.borsh")), bytes)
            .expect("write stdlib prefix artifact");
    }
}

fn main() {
    generate_stdlib_prefix();
}
