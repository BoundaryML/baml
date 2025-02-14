use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

use baml_types::{BamlValueWithMeta, EvaluationContext, FieldType};
use internal_baml_core::ir::repr::make_test_ir;
use jsonish::{from_str, helpers::render_output_format, BamlValueWithFlags};

use baml_runtime::internal::llm_client::{parsed_value_to_response, ResponseBamlValue};

criterion_group!(benches, parse_benchmarks, response_benchmarks);
criterion_main!(benches);

fn parse(
    schema: &str,
    target_type: &FieldType,
    msg: &str,
    allow_partials: bool,
) -> BamlValueWithFlags {
    let ir = make_test_ir(schema).unwrap();
    let target = render_output_format(&ir, target_type, &EvaluationContext::default()).unwrap();
    from_str(&target, target_type, msg, allow_partials).unwrap()
}

const SCHEMA: &str = r#"
class Foo {
  i int
}

type JSONValue = int | float | bool | string | null | JSONValue[] | map<string, JSONValue>
"#;

const BIG_JSON: &str = r#"
    {
        "number": 1,
        "string": "test",
        "bool": true,
        "list": [1, 2, 3],
        "object": {
            "number": 1,
            "string": "test",
            "bool": true,
            "list": [1, 2, 3]
        },
        "json": {
            "number": 1,
            "string": "test",
            "bool": true,
            "list": [1, 2, 3],
            "object": {
                "number": 1,
                "string": "test",
                "bool": true,
                "list": [1, 2, 3]
            }
        }
    }
"#;

fn parse_benchmarks(c: &mut Criterion) {
    // c.bench_function("parse basic", |b| b.iter(|| parse(
    //   black_box(SCHEMA),
    //   black_box(&FieldType::class("Foo")),
    //   black_box(r#"{"i": 1}"#),
    //   black_box(false),
    // ))
    // );

    // c.bench_function("parse JSONValue", |b| b.iter(|| parse(
    //   SCHEMA,
    //   &FieldType::RecursiveTypeAlias("JSONValue".to_string()),
    //   BIG_JSON,
    //   false,
    // ))
    // );
}

fn response_benchmarks(c: &mut Criterion) {
    // c.bench_function("response basic", |b| b.iter(|| to_response(
    //   black_box(SCHEMA),
    //   black_box(&FieldType::class("Foo")),
    //   black_box(r#"{"i": 1}"#),
    //   black_box(false),
    // )));

    c.bench_function("response JSONValue", |b| {
        b.iter(|| {
            to_response(
                black_box(SCHEMA),
                black_box(&FieldType::RecursiveTypeAlias("JSONValue".to_string())),
                black_box(BIG_JSON),
                black_box(false),
            )
        })
    });
}

fn to_response(
    schema: &str,
    target_type: &FieldType,
    msg: &str,
    allow_partials: bool,
) -> ResponseBamlValue {
    let ir = make_test_ir(schema).unwrap();
    let target = render_output_format(&ir, target_type, &EvaluationContext::default()).unwrap();
    let parsed = from_str(&target, target_type, msg, allow_partials).unwrap();
    parsed_value_to_response(&ir, parsed, target_type, true).unwrap()
}
