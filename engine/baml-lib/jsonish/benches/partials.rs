#[cfg(not(target_arch = "wasm32"))]
use baml_types::TypeIR;
#[cfg(not(target_arch = "wasm32"))]
use criterion::Criterion;
#[cfg(not(target_arch = "wasm32"))]
use internal_baml_jinja::types::Builder;
#[cfg(not(target_arch = "wasm32"))]
use jsonish::from_str;

const PARTIAL_SCHEMA: &str = r#"
class NestedObject {
    id int
    name string
    metadata Metadata
}

class Metadata {
    created_at string
    updated_at string
    tags string[]
    settings Settings
}

class Settings {
    is_active bool
    config Config
    preferences string[]
}

class Config {
    api_key string
    rate_limit int
    features string[]
}

class ComplexPartial {
    required_field string
    optional_field string?
    nested NestedObject
    list_field string[]
    map_field Map<string>
}
"#;

#[cfg(not(target_arch = "wasm32"))]
pub fn bench_partials(c: &mut Criterion) {
    let mut group = c.benchmark_group("partials");

    // Test partial parsing of a deeply nested object
    let target = TypeIR::class("NestedObject");
    let of = Builder::new(target.clone()).build();

    group.bench_function("partial_nested_shallow", |b| {
        b.iter(|| {
            from_str(
                &of,
                &target,
                r#"{
            "id": 1,
            "name": "test"
        }"#,
                true,
            )
        })
    });

    group.bench_function("partial_nested_mid", |b| {
        b.iter(|| {
            from_str(
                &of,
                &target,
                r#"{
            "id": 1,
            "name": "test",
            "metadata": {
                "created_at": "2024-01-10",
                "tags": ["tag1", "tag2"]
            }
        }"#,
                true,
            )
        })
    });

    // Test partial with optional fields
    let target = TypeIR::class("ComplexPartial");
    let of = Builder::new(target.clone()).build();

    group.bench_function("partial_with_optional", |b| {
        b.iter(|| {
            from_str(
                &of,
                &target,
                r#"{
            "required_field": "required",
            "list_field": ["item1"]
        }"#,
                true,
            )
        })
    });

    // Test partial with array fields
    group.bench_function("partial_with_arrays", |b| {
        b.iter(|| {
            from_str(
                &of,
                &target,
                r#"{
            "required_field": "required",
            "list_field": ["item1", "item2", "item3", "item4", "item5"],
            "nested": {
                "id": 1,
                "name": "test",
                "metadata": {
                    "tags": ["tag1", "tag2", "tag3"]
                }
            }
        }"#,
                true,
            )
        })
    });

    // Test partial with map fields
    group.bench_function("partial_with_maps", |b| {
        b.iter(|| {
            from_str(
                &of,
                &target,
                r#"{
            "required_field": "required",
            "map_field": {
                "key1": "value1",
                "key2": "value2",
                "key3": "value3"
            }
        }"#,
                true,
            )
        })
    });

    // Test partial with mixed fields
    group.bench_function("partial_mixed", |b| {
        b.iter(|| {
            from_str(
                &of,
                &target,
                r#"{
            "required_field": "required",
            "optional_field": "optional",
            "list_field": ["item1", "item2"],
            "map_field": {
                "key1": "value1"
            },
            "nested": {
                "id": 1,
                "name": "test"
            }
        }"#,
                true,
            )
        })
    });

    group.finish();
}
