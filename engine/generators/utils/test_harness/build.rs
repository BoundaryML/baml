use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("generated_macro.rs");
    
    // Get the cargo root directory (3 levels up from the test_harness crate)
    let cargo_manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let cargo_root = Path::new(&cargo_manifest_dir)
        .join("../../..")
        .canonicalize()
        .expect("Failed to canonicalize cargo root path");
    
    let data_dir = cargo_root.join("generators/data");
    
    // Read all directory names in generators/data
    let mut test_dirs = Vec::new();
    if let Ok(entries) = fs::read_dir(&data_dir) {
        for entry in entries {
            if let Ok(entry) = entry {
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
    }
    
    // Sort for consistent output
    test_dirs.sort();
    
    // Generate the macro code
    let mut macro_code = String::new();
    macro_code.push_str("#[macro_export]\n");
    macro_code.push_str("macro_rules! create_code_gen_test_suites {\n");
    macro_code.push_str("    ($generator_type:ty) => {\n");
    
    for test_dir in &test_dirs {
        let test_fn_name = format!("test_{}", test_dir);
        macro_code.push_str(&format!("        #[test]\n"));
        macro_code.push_str(&format!("        fn {}() -> anyhow::Result<()> {{\n", test_fn_name));
        macro_code.push_str(&format!("            let test_harness = test_harness::TestHarness::load_test(\"{}\", <$generator_type>::default(), true)?;\n", test_dir));
        macro_code.push_str("            test_harness.run()\n");
        macro_code.push_str("        }\n\n");

        // and another test that ensures the files are the same
        let test_fn_name = format!("test_{}_consistent", test_dir);
        macro_code.push_str(&format!("        #[test]\n"));
        macro_code.push_str(&format!("        fn {}() -> anyhow::Result<()> {{\n", test_fn_name));
        macro_code.push_str(&format!("            let test_harness = test_harness::TestHarness::load_test(\"{}\", <$generator_type>::default(), false)?;\n", test_dir));
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
