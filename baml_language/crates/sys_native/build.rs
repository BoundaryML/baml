fn main() {
    println!(
        "cargo:rustc-env=BAML_HOST_TARGET={}",
        std::env::var("TARGET").expect("Cargo target triple")
    );
}
