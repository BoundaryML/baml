use criterion::Criterion;
use internal_baml_jinja::types::Builder;
use jsonish::from_str;
use baml_types::{FieldType, LiteralValue};

pub fn bench_lists(c: &mut Criterion) {
    let mut group = c.benchmark_group("lists");
    
    let target = FieldType::List(Box::new(FieldType::Literal(LiteralValue::String("".to_string()))));
    let of = Builder::new(target.clone()).build();
    group.bench_function("string_list", |b| b.iter(|| from_str(
        &of,
        &target,
        r#"["hello", "world", "test", "benchmark"]"#,
        false,
    )));

    let target = FieldType::List(Box::new(FieldType::Literal(LiteralValue::Int(0))));
    let of = Builder::new(target.clone()).build();
    group.bench_function("int_list", |b| b.iter(|| from_str(
        &of,
        &target,
        r#"[1, 2, 3, 4, 5]"#,
        false,
    )));

    group.finish();
} 