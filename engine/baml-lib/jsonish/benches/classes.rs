use baml_types::TypeIR;
use criterion::Criterion;
use internal_baml_jinja::types::Builder;
use jsonish::from_str;

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
            )
        })
    });

    group.bench_function("partial_book", |b| {
        b.iter(|| {
            from_str(
                &of,
                &target,
                r#"{"title": "The Book", "author": "John Doe"}"#,
            )
        })
    });

    group.finish();
}
