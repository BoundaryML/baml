#[cfg(not(target_arch = "wasm32"))]
use baml_types::{LiteralValue, TypeIR};
#[cfg(not(target_arch = "wasm32"))]
use criterion::Criterion;
#[cfg(not(target_arch = "wasm32"))]
use internal_baml_jinja::types::Builder;
#[cfg(not(target_arch = "wasm32"))]
use jsonish::from_str;

#[cfg(not(target_arch = "wasm32"))]
pub fn bench_literals(c: &mut Criterion) {
    let mut group = c.benchmark_group("literals");

    let target = TypeIR::literal(10);
    let of = Builder::new(target.clone()).build();
    group.bench_function("parse_int", |b| {
        b.iter(|| from_str(&of, &target, "10", false))
    });

    let target = TypeIR::literal("hello");
    let of = Builder::new(target.clone()).build();
    group.bench_function("parse_string", |b| {
        b.iter(|| from_str(&of, &target, r#""hello""#, false))
    });

    group.finish();
}
