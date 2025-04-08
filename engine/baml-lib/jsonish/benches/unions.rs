use baml_types::FieldType;
use criterion::Criterion;
use internal_baml_jinja::types::Builder;
use jsonish::{from_str, helpers::common::UNION_SCHEMA, jsonish as internal_jsonish};

pub fn bench_unions(c: &mut Criterion) {
    let mut group = c.benchmark_group("unions");

    let target = FieldType::union(vec![
        FieldType::Class("VideoContent".to_string()),
        FieldType::Class("TextContent".to_string()),
        FieldType::Class("ImageContent".to_string()),
        FieldType::Class("AudioContent".to_string()),
    ]);
    let ir = jsonish::helpers::load_test_ir(UNION_SCHEMA);
    let of = jsonish::helpers::render_output_format(&ir, &target, &Default::default()).unwrap();

    // let of = Builder::new(target.clone()).build();

    group.bench_function("text_content", |b| {
        b.iter(|| from_str(&of, &target, r#"{"text": "Hello World"}"#, false))
    });

    group.bench_function("image_content", |b| {
        b.iter(|| {
            from_str(
                &of,
                &target,
                r#"{"url": "https://example.com/img.jpg", "width": 800, "height": 600}"#,
                false,
            )
        })
    });

    group.bench_function("video_content_jsonish_only", |b| {
        b.iter(|| {
            internal_jsonish::parse(
                r#"{"url": "https://example.com/video.mp4", "duration": 120}"#,
                internal_jsonish::ParseOptions::default(),
            )
        })
    });

    group.bench_function("video_content", |b| {
        b.iter(|| {
            from_str(
                &of,
                &target,
                r#"{"url": "https://example.com/video.mp4", "duration": 120}"#,
                false,
            )
        })
    });

    let target = FieldType::RecursiveTypeAlias("JSONValue".to_string());
    let of = jsonish::helpers::render_output_format(&ir, &target, &Default::default()).unwrap();

    group.bench_function("json_value_jsonish_only", |b| {
        b.iter(|| {
            internal_jsonish::parse(
                jsonish::helpers::common::JSON_STRING,
                internal_jsonish::ParseOptions::default(),
            )
        })
    });

    group.bench_function("json_value", |b| {
        b.iter(|| from_str(&of, &target, jsonish::helpers::common::JSON_STRING, true))
    });

    group.finish();
}
