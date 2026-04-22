#[cfg(test)]
mod tests {
    use bex_engine::BexExternalValue;

    const SOURCE: &str = r#"
function escaped_newline_length() -> int {
  "a\nb".length()
}

function literal_newline_sequence_length() -> int {
  "\n".length()
}

function escaped_tab_length() -> int {
  "a\tb".length()
}

function escaped_backslash_length() -> int {
  "a\\b".length()
}

function escaped_quote_length() -> int {
  "a\"b".length()
}
"#;

    #[tokio::test]
    async fn quoted_string_escapes_are_decoded() {
        assert_eq!(
            baml_test!(baml: SOURCE, entry: "escaped_newline_length").result,
            Ok(BexExternalValue::Int(3))
        );
        assert_eq!(
            baml_test!(baml: SOURCE, entry: "literal_newline_sequence_length").result,
            Ok(BexExternalValue::Int(1))
        );
        assert_eq!(
            baml_test!(baml: SOURCE, entry: "escaped_tab_length").result,
            Ok(BexExternalValue::Int(3))
        );
        assert_eq!(
            baml_test!(baml: SOURCE, entry: "escaped_backslash_length").result,
            Ok(BexExternalValue::Int(3))
        );
        assert_eq!(
            baml_test!(baml: SOURCE, entry: "escaped_quote_length").result,
            Ok(BexExternalValue::Int(3))
        );
    }
}
