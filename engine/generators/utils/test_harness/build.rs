use std::{env, fs, path::Path};

fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();

    // Get the cargo root directory (3 levels up from the test_harness crate)
    let cargo_manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let cargo_root = Path::new(&cargo_manifest_dir)
        .join("../../..")
        .canonicalize()
        .expect("Failed to canonicalize cargo root path");

    // 3. Figure out the filename for this platform
    let dylib_name = if cfg!(target_os = "macos") {
        "libbaml_cffi.dylib"
    } else if cfg!(target_os = "windows") {
        "baml_cffi.dll"
    } else {
        "libbaml_cffi.so"
    };

    // Check if we're in a build where baml_cffi has already been built
    // When baml_cffi is built as a dependency with links="baml_cffi",
    // Cargo doesn't provide the dylib location through DEP_ variables for cdylib crates
    // So we still need to look for it in the target directory
    // Try multiple locations where the dylib might be
    let possible_paths = [
        cargo_root.join("target/debug/deps").join(dylib_name),
        cargo_root.join("target/debug").join(dylib_name),
    ];

    let dylib_path = possible_paths
        .iter()
        .find(|p| p.exists())
        .cloned()
        .unwrap_or_else(|| cargo_root.join("target/debug").join(dylib_name));

    println!("dylib_path: {}", dylib_path.display());

    let dest = Path::new(&out_dir).join(dylib_name);

    fs::create_dir_all(dest.parent().unwrap()).unwrap();

    // Only copy if the dylib exists
    if dylib_path.exists() {
        fs::copy(&dylib_path, &dest).unwrap();

        // Also copy to the expected location for tests
        let expected_path = cargo_root.join("target/debug").join(dylib_name);
        if dylib_path != expected_path {
            fs::create_dir_all(expected_path.parent().unwrap()).unwrap();
            fs::copy(&dylib_path, &expected_path).unwrap();
            println!(
                "Copied dylib to expected location: {}",
                expected_path.display()
            );
        }
    } else {
        // Since baml_cffi is a cdylib, we need to ensure it's built first
        // The links field will ensure build ordering, but the dylib might not exist yet
        println!(
            "cargo:warning=baml_cffi dylib not found at {}. Make sure to build baml_cffi first: cargo build --package baml_cffi",
            dylib_path.display()
        );
    }

    let dest_path = Path::new(&out_dir).join("generated_macro.rs");

    let data_dir = cargo_root.join("generators/data");

    // Read all directory names in generators/data
    let mut test_dirs = Vec::new();
    if let Ok(entries) = fs::read_dir(&data_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                    // Skip hidden directories and .gitignore files
                    if !dir_name.starts_with('.') {
                        test_dirs.push(dir_name.to_string());
                    }
                }
            }
        }
    }

    // Sort for consistent output
    test_dirs.sort();

    // Generate the macro code
    let mut macro_code = String::new();
    macro_code.push_str("#[macro_export]\n");
    macro_code.push_str("macro_rules! create_code_gen_test_suites {\n");
    macro_code.push_str("    ($generator_type:ty) => {\n");

    for test_dir in &test_dirs {
        let test_fn_name = format!("test_{test_dir}_evaluate");
        macro_code.push_str("        #[test]\n");
        macro_code.push_str(&format!(
            "        fn {test_fn_name}() -> anyhow::Result<()> {{\n"
        ));
        macro_code.push_str(&format!("            let test_harness = test_harness::TestHarness::load_test(\"{test_dir}\", <$generator_type>::default(), true)?;\n"));
        macro_code.push_str("            test_harness.run()\n");
        macro_code.push_str("        }\n\n");

        // and another test that ensures the files are the same
        let test_fn_name = format!("test_{test_dir}_consistent");
        macro_code.push_str("        #[test]\n");
        macro_code.push_str(&format!(
            "        fn {test_fn_name}() -> anyhow::Result<()> {{\n"
        ));
        macro_code.push_str(&format!("            let test_harness = test_harness::TestHarness::load_test(\"{test_dir}\", <$generator_type>::default(), false)?;\n"));
        macro_code.push_str("            test_harness.ensure_consistent_codegen()\n");
        macro_code.push_str("        }\n\n");
    }

    macro_code.push_str("    };\n");
    macro_code.push_str("}\n");

    // Write the generated macro to a file
    fs::write(&dest_path, macro_code).unwrap();

    // Tell cargo to rerun this build script if the data directory changes
    println!("cargo:rerun-if-changed={}", data_dir.display());

    // Also watch for changes to individual test directories
    for test_dir in &test_dirs {
        let test_path = data_dir.join(test_dir);
        println!("cargo:rerun-if-changed={}", test_path.display());
    }
}
