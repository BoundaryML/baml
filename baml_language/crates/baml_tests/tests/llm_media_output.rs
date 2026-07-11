//! Regression tests for native LLM media results crossing back into the VM.
//!
//! Provider parsers must return the corresponding builtin BAML wrapper
//! instance, not a bare media ADT that lands as RustData and cannot receive
//! `image`/`pdf` methods.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

async fn expect_string(src: &str) -> String {
    let output = baml_test!(src);
    match output.result.unwrap() {
        BexExternalValue::String(value) => value.to_string(),
        other => panic!("expected string, got {other:?}"),
    }
}

#[tokio::test]
async fn parsed_image_is_a_usable_baml_media_instance() {
    let out = expect_string(
        r#"
        function main() -> string {
            let provider = baml.llm.from_shorthand("ai-gateway-images/bfl/flux-2-pro");
            let img = provider.parse<image>(`{"images":["aW1hZ2U="]}`);
            (img.mime_type() ?? "") + ":" + img.base64()
        }
        "#,
    )
    .await;
    assert_eq!(out, "image/png:aW1hZ2U=");
}

#[tokio::test]
async fn parsed_image_list_wraps_every_media_element() {
    let out = expect_string(
        r#"
        function main() -> string {
            let provider = baml.llm.from_shorthand("ai-gateway-images/bfl/flux-2-pro");
            let images = provider.parse<image[]>(`{"images":["b25l","dHdv"]}`);
            images[0].base64() + ":" + images[1].base64()
        }
        "#,
    )
    .await;
    assert_eq!(out, "b25l:dHdv");
}

#[tokio::test]
async fn parsed_optional_image_wraps_its_present_media_value() {
    let out = expect_string(
        r#"
        function main() -> string {
            let provider = baml.llm.from_shorthand("ai-gateway-images/bfl/flux-2-pro");
            let maybe_image = provider.parse<image?>(`{"images":["aW1hZ2U="]}`);
            match (maybe_image) {
                null => "missing",
                let present: image => present.base64(),
            }
        }
        "#,
    )
    .await;
    assert_eq!(out, "aW1hZ2U=");
}

#[tokio::test]
async fn responses_image_generation_result_preserves_output_mime_type() {
    let out = expect_string(
        r#"
        function main() -> string {
            let provider = baml.llm.from_shorthand("openai-responses/gpt-5.6-luna");
            let img = provider.parse<image>(`{
                "id":"resp_1",
                "status":"completed",
                "model":"gpt-5.6-luna",
                "output":[{
                    "type":"image_generation_call",
                    "id":"ig_1",
                    "status":"completed",
                    "result":"aW1hZ2U=",
                    "output_format":"webp"
                }]
            }`);
            (img.mime_type() ?? "") + ":" + img.base64()
        }
        "#,
    )
    .await;
    assert_eq!(out, "image/webp:aW1hZ2U=");
}
