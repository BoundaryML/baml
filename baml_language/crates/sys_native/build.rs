mod build_heap_free;
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build_heap_free.rs");
    println!("cargo:rerun-if-changed=src/io_impls.rs");
    let source = std::fs::read_to_string("src/io_impls.rs").expect("native provider source");
    let output = build_heap_free::generate(&source);
    std::fs::write(
        std::path::Path::new(&std::env::var_os("OUT_DIR").unwrap()).join("io_impls.rs"),
        output,
    )
    .expect("heap-free provider module");
    println!(
        "cargo:rustc-env=BAML_HOST_TARGET={}",
        std::env::var("TARGET").expect("Cargo target triple")
    );
}
