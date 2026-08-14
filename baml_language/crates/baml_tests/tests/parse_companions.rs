use baml_tests::baml_test;
use bex_engine::BexExternalValue;
use indexmap::indexmap;

#[tokio::test]
async fn parse_companion_allows_missing_optional_class_field() {
    let output = baml_test!(
        r##"
            client TestClient = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = "http://localhost:1234",
            );

            class Payload {
                text string?
            }

            function ParsePayload() -> Payload {
                client TestClient
                prompt `${ctx.output_format}`
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
            type_args: vec![],
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
            client TestClient = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = "http://localhost:1234",
            );

            type MaybeText = string | null

            class Payload {
                text MaybeText
            }

            function ParsePayload() -> Payload {
                client TestClient
                prompt `${ctx.output_format}`
            }

            function main() -> Payload {
                ParsePayload$parse("{}")
            }
        "##
    );

    // `type MaybeText = string | null` is now identical to `string?` — a
    // nullable union is optionality, so a missing/null value is bare `Null`
    // (not wrapped in union metadata), matching the `string?` case above.
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Instance {
            class_name: "user.Payload".to_string(),
            type_args: vec![],
            fields: indexmap! {
                "text".to_string() => BexExternalValue::Null,
            },
        })
    );
}

#[tokio::test]
async fn sap_parse_decodes_a_complete_top_level_json_string() {
    let output = baml_test!(
        r##"
        client TestClient = openai.ResponsesClient.new(
            model = "gpt-4o-mini",
            api_key = "test-key",
            base_url = "http://localhost:1234",
        );

        function ParseString() -> string {
            client TestClient
            prompt `${ctx.output_format}`
        }

        function main() -> string {
            ParseString$parse(`"Fred"`)
        }
        "##
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("Fred".into())));
}

#[tokio::test]
async fn sap_parse_preserves_plain_llm_text() {
    let output = baml_test!(
        r##"
        client TestClient = openai.ResponsesClient.new(
            model = "gpt-4o-mini",
            api_key = "test-key",
            base_url = "http://localhost:1234",
        );

        function ParseString() -> string {
            client TestClient
            prompt `${ctx.output_format}`
        }

        function main() -> string {
            ParseString$parse("Fred says hello")
        }
        "##
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Fred says hello".into()))
    );
}
