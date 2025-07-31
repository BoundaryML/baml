#[cfg(not(target_arch = "wasm32"))]
use baml_types::{ir_type::UnionConstructor, TypeIR};
#[cfg(not(target_arch = "wasm32"))]
use criterion::{criterion_group, criterion_main, Criterion};
#[cfg(not(target_arch = "wasm32"))]
use internal_baml_jinja::types::Builder;
#[cfg(not(target_arch = "wasm32"))]
use jsonish::{from_str, helpers::common::UNION_SCHEMA, jsonish as internal_jsonish};

#[cfg(not(target_arch = "wasm32"))]
pub fn bench_unions(c: &mut Criterion) {
    let mut group = c.benchmark_group("unions");
    let ir = jsonish::helpers::load_test_ir(UNION_SCHEMA);

    // let target = TypeIR::union(vec![
    //     TypeIR::class("VideoContent"),
    //     TypeIR::class("TextContent"),
    //     TypeIR::class("ImageContent"),
    //     TypeIR::class("AudioContent"),
    // ]);
    // let of = jsonish::helpers::render_output_format(
    //     &ir,
    //     &target,
    //     &Default::default(),
    //     baml_types::StreamingMode::NonStreaming,
    // )
    // .unwrap();

    // // let of = Builder::new(target.clone()).build();

    // group.bench_function("text_content", |b| {
    //     b.iter(|| from_str(&of, &target, r#"{"text": "Hello World"}"#, true))
    // });

    // group.bench_function("image_content", |b| {
    //     b.iter(|| {
    //         from_str(
    //             &of,
    //             &target,
    //             r#"{"url": "https://example.com/img.jpg", "width": 800, "height": 600}"#,
    //             true,
    //         )
    //     })
    // });

    // group.bench_function("video_content_jsonish_only", |b| {
    //     b.iter(|| {
    //         internal_jsonish::parse(
    //             r#"{"url": "https://example.com/video.mp4", "duration": 120}"#,
    //             internal_jsonish::ParseOptions::default(),
    //             true,
    //         )
    //     })
    // });

    // group.bench_function("video_content", |b| {
    //     b.iter(|| {
    //         from_str(
    //             &of,
    //             &target,
    //             r#"{"url": "https://example.com/video.mp4", "duration": 120}"#,
    //             true,
    //         )
    //     })
    // });

    // let target = TypeIR::recursive_type_alias("JSONValue");
    // let of = jsonish::helpers::render_output_format(
    //     &ir,
    //     &target,
    //     &Default::default(),
    //     baml_types::StreamingMode::NonStreaming,
    // )
    // .unwrap();

    // group.bench_function("json_value_jsonish_only", |b| {
    //     b.iter(|| {
    //         internal_jsonish::parse(
    //             jsonish::helpers::common::JSON_STRING,
    //             internal_jsonish::ParseOptions::default(),
    //             true,
    //         )
    //     })
    // });

    // group.bench_function("json_value", |b| {
    //     b.iter(|| from_str(&of, &target, jsonish::helpers::common::JSON_STRING, true))
    // });

    let target = TypeIR::recursive_type_alias("Story");
    let of = jsonish::helpers::render_output_format(
        &ir,
        &target,
        &Default::default(),
        baml_types::StreamingMode::NonStreaming,
    )
    .unwrap();
    group.bench_function("story", |b| {
        b.iter(|| {
            from_str(
                &of,
                &target,
                jsonish::helpers::common::JSON_STRING_STORY,
                true,
            )
        })
    });

    group.finish();
}

#[cfg(not(target_arch = "wasm32"))]
criterion_group!(benches, bench_unions);
#[cfg(not(target_arch = "wasm32"))]
criterion_main!(benches);

#[cfg(target_arch = "wasm32")]
fn main() {
    // No-op for WASM builds
}
