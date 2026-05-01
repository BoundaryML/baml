use baml_tests::baml_test;
use bex_engine::{BexExternalValue, Ty};
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
            class_name: "user.Payload".to_string(),
            fields: indexmap! {
                "text".to_string() => BexExternalValue::Null,
            },
        })
    );
}

#[tokio::test]
async fn parse_companion_allows_missing_nullable_alias_field() {
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

            type MaybeText = string | null

            class Payload {
                text MaybeText
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
            class_name: "user.Payload".to_string(),
            fields: indexmap! {
                "text".to_string() => BexExternalValue::union(
                    BexExternalValue::Null,
                    [Ty::string(), Ty::null()],
                    Ty::null(),
                ),
            },
        })
    );
}
