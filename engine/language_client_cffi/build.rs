use std::path::Path;

use flatc::flatc;

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

    #[allow(clippy::single_element_loop)]
    for out_path in
        [Path::new(&crate_dir).join("../language_client_go/include/baml_cffi_generated.h")]
    {
        let outpath_content =
            std::fs::read_to_string(&out_path).unwrap_or_else(|_| String::from(""));
        let res = cbindgen::Builder::new()
            .with_config(cbindgen::Config::from_file("cbindgen.toml").unwrap())
            .with_crate(".")
            .generate()
            .expect("Failed to generate C header")
            .write_to_file(out_path.clone());
        if std::env::var("CI").is_ok() && res {
            let new_content = std::fs::read_to_string(&out_path).unwrap();
            println!("New header content: \n==============\n{new_content}");
            println!("\n\n");
            println!("Old header content: \n==============\n{outpath_content}");
            panic!("cbindgen generated a diff");
        }
    }
}
