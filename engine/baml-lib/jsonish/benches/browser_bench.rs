// Default configuration - will run in Node.js when --node is specified

// Import the key functions we want to benchmark
use baml_types::TypeIR;
use criterion::{criterion_group, Criterion};
use internal_baml_jinja::types::Builder;
use jsonish::{from_str, helpers::common::UNION_SCHEMA, jsonish as internal_jsonish};
use wasm_bindgen_test::wasm_bindgen_test;

/// Benchmark parsing a simple JSON object
fn bench_simple_json(c: &mut Criterion) {
    c.bench_function("simple_json_parse", |b| {
        b.iter(|| {
            internal_jsonish::parse(
                r#"{"name": "test", "value": 42}"#,
                internal_jsonish::ParseOptions::default(),
                true,
            )
        })
    });
}

/// Benchmark parsing a more complex nested structure
fn bench_nested_json(c: &mut Criterion) {
    let json_str = r#"{
        "user": {
            "id": 123,
            "name": "Alice",
            "settings": {
                "theme": "dark",
                "notifications": true,
                "features": ["advanced", "beta"]
            }
        },
        "data": [1, 2, 3, 4, 5]
    }"#;

    c.bench_function("nested_json_parse", |b| {
        b.iter(|| {
            internal_jsonish::parse(json_str, internal_jsonish::ParseOptions::default(), true)
        })
    });
}

/// Benchmark string list parsing
fn bench_string_list(c: &mut Criterion) {
    let target = TypeIR::literal("").as_list();
    let of = Builder::new(target.clone()).build();

    c.bench_function("string_list", |b| {
        b.iter(|| {
            from_str(
                &of,
                &target,
                r#"["hello", "world", "test", "benchmark"]"#,
                true,
            )
        })
    });
}

/// Benchmark union type parsing
fn bench_union_parsing(c: &mut Criterion) {
    let ir = jsonish::helpers::load_test_ir(UNION_SCHEMA);
    let target = TypeIR::recursive_type_alias("Story");
    let of = jsonish::helpers::render_output_format(
        &ir,
        &target,
        &Default::default(),
        baml_types::StreamingMode::NonStreaming,
    )
    .expect("Failed to create output format");

    c.bench_function("union_story", |b| {
        b.iter(|| {
            from_str(
                &of,
                &target,
                jsonish::helpers::common::JSON_STRING_STORY,
                true,
            )
        })
    });
}

/// wasm-bindgen provides the harness; we call Criterion manually
#[wasm_bindgen_test]
fn run_all_benches() {
    // No HTML plots → avoids filesystem writes in the browser
    let mut c = Criterion::default();

    // Run each benchmark individually
    bench_simple_json(&mut c);
    bench_nested_json(&mut c);
    bench_string_list(&mut c);
    bench_union_parsing(&mut c);

    // Print stats to stdout
    c.final_summary();
}

/// Simple manual benchmark using basic timing
#[wasm_bindgen_test]
fn bench_simple_manual() {
    // Simple JSON parsing benchmark using basic iteration
    let json_str = r#"{"test": "value", "number": 42, "array": [1, 2, 3]}"#;
    let iterations = 100;

    // Just run iterations without timing for now - results will show in Criterion output
    for _ in 0..iterations {
        let _ = internal_jsonish::parse(json_str, internal_jsonish::ParseOptions::default(), true);
    }

    // Simple assertion to ensure the benchmark actually works
    let result = internal_jsonish::parse(json_str, internal_jsonish::ParseOptions::default(), true);
    assert!(result.is_ok());
}

// Provide a main function for regular builds
#[cfg(not(target_arch = "wasm32"))]
fn main() {
    // For regular benchmarking, this file is intended to be run with wasm-pack
    println!("This benchmark is designed for WASM browser testing.");
    println!("Run with: wasm-pack test --headless --firefox -- --bench --release");
}

// Provide a main function for WASM builds
#[cfg(target_arch = "wasm32")]
fn main() {
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);
    // wasm-bindgen-test will provide its own runtime
}
