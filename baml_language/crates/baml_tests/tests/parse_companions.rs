use baml_tests::baml_test;
use bex_engine::BexExternalValue;
use indexmap::indexmap;

#[tokio::test]
async fn parse_companion_allows_missing_optional_class_field() {
    let output = baml_test!(
        r##"
            client<llm> TestClient {
                provider openai
                options {
                    model "gpt-4o-mini"
                    api_key "test-key"
                    base_url "http://localhost:1234"
                }
            }

            class Payload {
                text string?
            }

            function ParsePayload() -> Payload {
                client TestClient
                prompt #"{{ ctx.output_format }}"#
            }

            function main() -> Payload {
                ParsePayload$parse("{}")
            }
        "##
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::Instance {
            class_name: "Payload".to_string(),
            fields: indexmap! {
                "text".to_string() => BexExternalValue::Null,
            },
        })
    );
}
