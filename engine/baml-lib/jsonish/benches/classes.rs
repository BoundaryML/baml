#[cfg(not(target_arch = "wasm32"))]
use baml_types::TypeIR;
#[cfg(not(target_arch = "wasm32"))]
use criterion::Criterion;
#[cfg(not(target_arch = "wasm32"))]
use internal_baml_jinja::types::Builder;
#[cfg(not(target_arch = "wasm32"))]
use jsonish::from_str;

#[cfg(not(target_arch = "wasm32"))]
pub fn bench_complex_classes(c: &mut Criterion) {
    let mut group = c.benchmark_group("complex_classes");

    let target = TypeIR::class("Book");
    let of = Builder::new(target.clone()).build();

    group.bench_function("full_book", |b| {
        b.iter(|| {
            from_str(
                &of,
                &target,
                r#"{
            "title": "The Book",
            "author": "John Doe",
            "year": 2024,
            "tags": ["fiction", "mystery"],
            "ratings": [
                {"score": 5, "reviewer": "Alice", "date": "2024-01-01"},
                {"score": 4, "reviewer": "Bob", "date": "2024-01-02"}
            ]
        }"#,
                true,
            )
        })
    });

    group.bench_function("partial_book", |b| {
        b.iter(|| {
            from_str(
                &of,
                &target,
                r#"{"title": "The Book", "author": "John Doe"}"#,
                true,
            )
        })
    });

    group.finish();
}
