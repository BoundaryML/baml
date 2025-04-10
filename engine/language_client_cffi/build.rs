use std::{path::Path, process::Command};

use cbindgen;
use flatc::flatc;
use flatc_rust;

fn main() {
    // Re-run build.rs if these files change.
    println!("cargo:rerun-if-changed=types/cffi.fbs");
    println!("cargo:rerun-if-changed=cbindgen.toml");
    println!("cargo:rerun-if-changed=src/lib.rs");

    let flat_bin = flatc_rust::Flatc::from_path(flatc());

    let args = flatc_rust::Args {
        lang: "rust",
        inputs: &[Path::new("types/cffi.fbs")],
        out_dir: Path::new("src/cffi"),
        ..Default::default()
    };
    flat_bin
        .run(args)
        .expect("Failed to generate Rust bindings");

    let args: flatc_rust::Args<'_> = flatc_rust::Args {
        lang: "go",
        inputs: &[Path::new("types/cffi.fbs")],
        out_dir: Path::new("../language_client_go/pkg"),
        ..Default::default()
    };
    flat_bin
        .run(args)
        .expect("Failed to generate Rust bindings");

    // Use cbindgen to generate the C header for your Rust library.
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");

    for out_path in
        [Path::new(&crate_dir).join("../language_client_go/include/baml_cffi_generated.h")]
    {
        let res = cbindgen::Builder::new()
            .with_config(cbindgen::Config::from_file("cbindgen.toml").unwrap())
            .with_crate(".")
            .generate()
            .expect("Failed to generate C header")
            .write_to_file(out_path);
        if std::env::var("CI").is_ok() {
            if res {
                panic!("cbindgen generated a diff");
            }
        }
    }
}
