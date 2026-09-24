fn main() {
    let target = std::env::var("TARGET").expect("Cargo target triple");
    println!("cargo:rustc-env=BAML_CFFI_TARGET={target}");
    println!("cargo:rerun-if-changed=cbindgen.toml");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=build.rs");
}
