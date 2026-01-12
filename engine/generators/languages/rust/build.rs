use std::{env, fs, path::Path};

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("generated_type_tests.rs");

    let code = type_test_spec::generate_test_code("rust");
    fs::write(&dest_path, code).unwrap();

    // Re-run if the spec file changes
    println!("cargo:rerun-if-changed=../../type_serialization_tests.md");
    println!("cargo:rerun-if-changed=../../utils/type_test_spec/src/lib.rs");
}
