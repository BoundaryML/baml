use baml_tests::baml_test;
use bex_engine::BexExternalValue;

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
            client: TestClient
            prompt: `${ctx.output_format()}`
        }

        function main() -> string {
            ParseString@parse(`"Fred"`)
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
            client: TestClient
            prompt: `${ctx.output_format()}`
        }

        function main() -> string {
            ParseString@parse("Fred says hello")
        }
        "##
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Fred says hello".into()))
    );
}
