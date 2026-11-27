//! Generate today's TypeScript SDK for the staged type-shapes fixture, twice:
//! as it is (over today's bridge) and as the native bindings (the same
//! files, with only the runtime underneath swapped).
//!
//! ```text
//! generate_ts <fixtures-root> <library.dylib> <interface-hash> <out>
//! ```
//!
//! `<fixtures-root>/type_shapes/baml_src` is the fixture with the namespace
//! `bridge_test` added (`scripts/stage.sh`). Writes `<out>/today` and
//! `<out>/native`, each a package with the generated SDK under
//! `node/baml_sdk`, today's tests for the fixture, the bench (`js/bench`,
//! compiled to `dist/` by `tsconfig.dist.json`), and in the native package
//! this crate's own tests (`js/test`).
//!
//! What the native copy changes in the generated files, and nothing else:
//!
//! - `_inlinedbaml.ts`: the program is a library, so it names the library and
//!   its interface hash instead of embedding bytecode;
//! - `index.ts`: the line that imports the initializer and the line that
//!   calls it.
//!
//! Both packages import `@boundaryml/baml-bridge`; `package.json` points it
//! at today's bridge or at the native runtime (`js/`).
#![allow(
    clippy::print_stdout,
    clippy::disallowed_methods,
    reason = "a command-line tool that writes files and says where"
)]

use std::{
    fs,
    path::{Path, PathBuf},
};

use sdkgen_typescript_shared::{NamingConvention, sdkgen_typescript};

/// The two lines of the generated `index.ts` the native copy changes.
const INIT_IMPORT: &str = "import { defineFunction, initializeRuntimeFromBlob, setTypeMap } from \"@boundaryml/baml-bridge\";";
const INIT_IMPORT_NATIVE: &str = "import { defineFunction, initializeNativeLibrary, setTypeMap } from \"@boundaryml/baml-bridge\";";
const INIT_CALL: &str = "initializeRuntimeFromBlob(_inlinedbaml.BYTECODE, _inlinedbaml.BAML_TOML);";
const INIT_CALL_NATIVE: &str =
    "initializeNativeLibrary(_inlinedbaml.LIBRARY, _inlinedbaml.INTERFACE_HASH);";

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    let [_, fixtures, library, hash, out] = arguments.as_slice() else {
        eprintln!("usage: generate_ts <fixtures-root> <library.dylib> <interface-hash> <out>");
        std::process::exit(2);
    };
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let loaded = sdk_test_codegen::load_fixture(Path::new(fixtures), "type_shapes");
    let output = sdkgen_typescript::to_source_code_with_bytecode(
        &loaded.pool,
        &loaded.baml_bytecode,
        NamingConvention::PreserveCase,
    );
    let out = PathBuf::from(out);
    let customizable = workspace.join("sdk_tests/crates/typescript/type_shapes/customizable");
    let templates = workspace.join("sdk_tests/codegen/src/templates");
    let own_tests = workspace.join("crates/baml_native_node/js/test");
    let bench = workspace.join("crates/baml_native_node/js/bench");
    for side in ["today", "native"] {
        let package = out.join(side);
        let sdk = package.join("node/baml_sdk");
        if sdk.exists() {
            fs::remove_dir_all(&sdk).expect("remove the old SDK");
        }
        for (relative, contents) in &output {
            let path = sdk.join(relative);
            fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
            fs::write(&path, contents).expect("write a generated file");
        }
        copy_dir(&customizable, &package.join("node"));
        if side == "native" {
            copy_dir(&own_tests, &package.join("node"));
        }
        for file in ["bench.ts", "bench_values.ts"] {
            fs::copy(bench.join(file), package.join("node").join(file)).expect("the bench");
        }
        fs::copy(
            bench.join("tsconfig.dist.json"),
            package.join("tsconfig.dist.json"),
        )
        .expect("tsconfig.dist.json");
        fs::copy(
            templates.join("test_runtime.ts"),
            package.join("node/test_runtime.ts"),
        )
        .expect("test_runtime.ts");
        fs::copy(
            templates.join("tsconfig.json"),
            package.join("tsconfig.node.json"),
        )
        .expect("tsconfig");
        fs::copy(
            templates.join("vitest_node.config.ts"),
            package.join("vitest.node.config.ts"),
        )
        .expect("vitest config");
        let bridge = if side == "today" {
            workspace.join("sdks/typescript/bridge_typescript")
        } else {
            workspace.join("crates/baml_native_node/js")
        };
        let bridge = bridge.canonicalize().expect("the bridge package");
        let package_json = fs::read_to_string(templates.join("package_node.json"))
            .expect("package template")
            .replace("__PACKAGE_NAME__", &format!("native-bridges-{side}"))
            .replace(
                "\"file:../../../../../sdks/typescript/bridge_typescript\"",
                &format!("\"link:{}\"", bridge.display()),
            );
        fs::write(package.join("package.json"), package_json).expect("package.json");
    }

    // The native copy: the program is a library.
    let sdk = out.join("native/node/baml_sdk");
    fs::copy(library, sdk.join("libtype_shapes.dylib")).expect("copy the library");
    fs::write(
        sdk.join("_inlinedbaml.ts"),
        format!(
            "// The program, compiled to a native library beside this file, and the\n\
             // interface hash the bindings were generated for.\n\
             export const LIBRARY = new URL(\"./libtype_shapes.dylib\", import.meta.url).pathname;\n\
             export const INTERFACE_HASH = \"{hash}\";\n"
        ),
    )
    .expect("_inlinedbaml.ts");
    let index_path = sdk.join("index.ts");
    let index = fs::read_to_string(&index_path).expect("index.ts");
    assert!(
        index.contains(INIT_IMPORT) && index.contains(INIT_CALL),
        "the generated index.ts no longer has the lines the native copy changes"
    );
    let index = index
        .replace(INIT_IMPORT, INIT_IMPORT_NATIVE)
        .replace(INIT_CALL, INIT_CALL_NATIVE);
    fs::write(&index_path, index).expect("index.ts");
    println!(
        "wrote {} and {}",
        out.join("today").display(),
        out.join("native").display()
    );
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("a directory");
    for entry in fs::read_dir(from).expect("a directory to copy") {
        let entry = entry.expect("an entry").path();
        let target = to.join(entry.file_name().expect("a name"));
        if entry.is_dir() {
            copy_dir(&entry, &target);
        } else {
            fs::copy(&entry, &target).expect("copy a file");
        }
    }
}
