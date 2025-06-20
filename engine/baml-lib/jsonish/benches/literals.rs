use baml_types::{FieldType, LiteralValue};
use criterion::Criterion;
use internal_baml_jinja::types::Builder;
use jsonish::from_str;

pub fn bench_literals(c: &mut Criterion) {
    let mut group = c.benchmark_group("literals");

    let target = FieldType::literal(10);
    let of = Builder::new(target.clone()).build();
    group.bench_function("parse_int", |b| {
        b.iter(|| from_str(&of, &target, "10", false))
    });

    let target = FieldType::literal("hello");
    let of = Builder::new(target.clone()).build();
    group.bench_function("parse_string", |b| {
        b.iter(|| from_str(&of, &target, r#""hello""#, false))
    });

    group.finish();
}
