mod build_heap_free;
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build_heap_free.rs");
    for file in ["io_impls.rs", "installation.rs"] {
        println!("cargo:rerun-if-changed=src/{file}");
        let source =
            std::fs::read_to_string(format!("src/{file}")).expect("native provider source");
        let output = build_heap_free::generate(&source);
        std::fs::write(
            std::path::Path::new(&std::env::var_os("OUT_DIR").unwrap()).join(file),
            output,
        )
        .expect("heap-free provider module");
    }
    println!(
        "cargo:rustc-env=BAML_HOST_TARGET={}",
        std::env::var("TARGET").expect("Cargo target triple")
    );
}
