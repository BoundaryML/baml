//! WASM integration tests for baml_playground_wasm
//!
//! Run with: wasm-pack test --node

use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_node_experimental);

use baml_runtime_wasm::{sam_sandbox::CasingVariants, BamlRuntime};

#[wasm_bindgen_test]
fn test_casing_variants_original() {
    let variants = CasingVariants::new("HelloWorld");
    assert_eq!(variants.original(), "HelloWorld");
}

#[wasm_bindgen_test]
fn test_casing_variants_lower() {
    // convert_case::Case::Lower splits on word boundaries and lowercases
    let variants = CasingVariants::new("HelloWorld");
    assert_eq!(variants.lower(), "hello world");
}

#[wasm_bindgen_test]
fn test_casing_variants_upper() {
    // convert_case::Case::Upper splits on word boundaries and uppercases
    let variants = CasingVariants::new("HelloWorld");
    assert_eq!(variants.upper(), "HELLO WORLD");
}

#[wasm_bindgen_test]
fn test_casing_variants_camel() {
    let variants = CasingVariants::new("hello_world");
    assert_eq!(variants.camel(), "helloWorld");
}

#[wasm_bindgen_test]
fn test_casing_variants_pascal() {
    let variants = CasingVariants::new("hello_world");
    assert_eq!(variants.pascal(), "HelloWorld");
}

#[wasm_bindgen_test]
fn test_casing_variants_kebab() {
    let variants = CasingVariants::new("HelloWorld");
    assert_eq!(variants.kebab(), "hello-world");
}

#[wasm_bindgen_test]
fn test_casing_variants_title() {
    let variants = CasingVariants::new("hello_world");
    assert_eq!(variants.title(), "Hello World");
}

#[wasm_bindgen_test]
fn test_baml_runtime_new() {
    let rt = BamlRuntime::new("test source".to_string());
    assert_eq!(rt.baml_src(), "test source");
}

#[wasm_bindgen_test]
fn test_baml_runtime_set_source() {
    let mut rt = BamlRuntime::new("initial".to_string());
    assert_eq!(rt.baml_src(), "initial");

    rt.set_source("updated".to_string());
    assert_eq!(rt.baml_src(), "updated");
}

#[wasm_bindgen_test]
fn test_baml_runtime_render() {
    let rt = BamlRuntime::new("HelloWorld".to_string());
    let variants = rt.render();
    assert_eq!(variants.original(), "HelloWorld");
    assert_eq!(variants.lower(), "hello world");
    assert_eq!(variants.upper(), "HELLO WORLD");
}

#[wasm_bindgen_test]
fn test_baml_runtime_function_names_empty() {
    // Empty BAML source should return empty function names (plus debug injection)
    let rt = BamlRuntime::new("".to_string());
    let names = rt.function_names();
    // Currently includes "injected-hot-reload2" debug value
    assert!(names.contains(&"injected-hot-reload2".to_string()));
}
