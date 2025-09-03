use std::{env, fs, io::Write as _, path};

const VALIDATIONS_ROOT_DIR: &str = "tests/validation_files";
const HIR_ROOT_DIR: &str = "tests/hir_files";
const BYTECODE_ROOT_DIR: &str = "tests/bytecode_files";
const CARGO_MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");
const BAML_CLI_INIT_DIR: &str = "/../../baml-runtime/src/cli/initial_project/baml_src";
const PROMPT_FIDDLE_EXAMPLE_DIR: &str =
    "/../../../typescript/apps/fiddle-web-app/public/_examples/all-projects/baml_src";

fn main() {
    if std::env::var("SKIP_BAML_VALIDATION").unwrap_or_default() == "1" {
        println!("cargo:warning=Skipping BAML validation during build");
    } else {
        build_folder_tests(
            BAML_CLI_INIT_DIR,
            "tests/validation_files/baml_cli_init.baml",
        );
        build_folder_tests(
            PROMPT_FIDDLE_EXAMPLE_DIR,
            "tests/validation_files/prompt_fiddle_example.baml",
        );
        build_validation_tests();
        build_hir_tests();
        build_bytecode_tests();
        // build_reformat_tests();
    }
}

fn build_validation_tests() {
    println!("cargo:rerun-if-changed={VALIDATIONS_ROOT_DIR}");
    let mut all_schemas = Vec::new();
    find_all_schemas("", &mut all_schemas, VALIDATIONS_ROOT_DIR);

    let mut out_file = out_file("validation_tests.rs");

    // Only generate tests for .baml files, and skip custom headers fixtures (covered by mermaid_headers.rs)
    for schema_path in all_schemas
        .iter()
        .filter(|p| p.ends_with(".baml") && !p.starts_with("/headers/"))
    {
        let test_name = test_name(schema_path);
        let file_path = schema_path.trim_start_matches('/');
        writeln!(
            out_file,
            "#[test] fn {test_name}() {{ run_validation_test(\"{file_path}\"); }}"
        )
        .unwrap();
    }
}

fn build_folder_tests(dir: &'static str, out_file_name: &str) {
    println!("cargo:rerun-if-changed={dir}");
    let mut all_schemas = Vec::new();
    find_all_schemas("", &mut all_schemas, dir);
    all_schemas.sort();

    // concatenate all the files in the directory into a single file
    let mut out_file = fs::File::create(format!("{CARGO_MANIFEST_DIR}/{out_file_name}")).unwrap();
    for schema_path in &all_schemas {
        let file_path = format!("{CARGO_MANIFEST_DIR}/{dir}{schema_path}");
        println!("Reading file: {file_path}");
        let file_content = fs::read_to_string(&file_path).unwrap();
        writeln!(out_file, "{file_content}").unwrap();
    }
}

fn find_all_schemas(prefix: &str, all_schemas: &mut Vec<String>, root_dir: &'static str) {
    for entry in fs::read_dir(format!("{CARGO_MANIFEST_DIR}/{root_dir}/{prefix}")).unwrap() {
        let entry = entry.unwrap();
        let file_name = entry.file_name();
        let file_name = file_name.to_str().unwrap();
        let entry_path = format!("{prefix}/{file_name}");
        let file_type = entry.file_type().unwrap();

        if file_name == "." || file_name == ".." {
            continue;
        }

        if file_type.is_file() {
            all_schemas.push(entry_path);
        } else if file_type.is_dir() {
            find_all_schemas(&entry_path, all_schemas, root_dir);
        }
    }
}

fn out_file(name: &str) -> std::fs::File {
    let out_dir = env::var("OUT_DIR").unwrap();
    let out_file_path = path::Path::new(&out_dir).join(name);
    fs::File::create(out_file_path).unwrap()
}

fn test_name(schema_file_path: &str) -> String {
    schema_file_path
        .trim_start_matches('/')
        .trim_end_matches(".baml")
        .replace(['/', '\\'], "_")
}

fn build_hir_tests() {
    println!("cargo:rerun-if-changed={HIR_ROOT_DIR}");
    let mut all_schemas = Vec::new();
    find_all_schemas("", &mut all_schemas, HIR_ROOT_DIR);

    let mut out_file = out_file("hir_tests.rs");

    for schema_path in &all_schemas {
        let test_name = test_name(schema_path);
        let file_path = schema_path.trim_start_matches('/');
        writeln!(
            out_file,
            "#[test] fn {test_name}() {{ run_hir_test(\"{file_path}\", include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/{HIR_ROOT_DIR}/{file_path}\"))); }}"
        )
        .unwrap();
    }
}

fn build_bytecode_tests() {
    println!("cargo:rerun-if-changed={BYTECODE_ROOT_DIR}");
    let mut all_schemas = Vec::new();
    find_all_schemas("", &mut all_schemas, BYTECODE_ROOT_DIR);

    let mut out_file = out_file("bytecode_tests.rs");

    for schema_path in &all_schemas {
        let test_name = test_name(schema_path);
        let file_path = schema_path.trim_start_matches('/');
        writeln!(
            out_file,
            "#[test] fn {test_name}() {{ run_bytecode_test(\"{file_path}\", include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/{BYTECODE_ROOT_DIR}/{file_path}\"))); }}"
        )
        .unwrap();
    }
}
