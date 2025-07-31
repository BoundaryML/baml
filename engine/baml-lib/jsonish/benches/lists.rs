#[cfg(not(target_arch = "wasm32"))]
use baml_types::{LiteralValue, TypeIR};
#[cfg(not(target_arch = "wasm32"))]
use criterion::Criterion;
#[cfg(not(target_arch = "wasm32"))]
use internal_baml_jinja::types::Builder;
#[cfg(not(target_arch = "wasm32"))]
use jsonish::from_str;

#[cfg(not(target_arch = "wasm32"))]
pub fn bench_lists(c: &mut Criterion) {
    let mut group = c.benchmark_group("lists");

    let target = TypeIR::literal("").as_list();
    let of = Builder::new(target.clone()).build();
    group.bench_function("string_list", |b| {
        b.iter(|| {
            from_str(
                &of,
                &target,
                r#"["hello", "world", "test", "benchmark"]"#,
                true,
            )
        })
    });

    let target = TypeIR::literal(0).as_list();
    let of = Builder::new(target.clone()).build();
    group.bench_function("int_list", |b| {
        b.iter(|| from_str(&of, &target, r#"[1, 2, 3, 4, 5]"#, true))
    });

    group.finish();
}
