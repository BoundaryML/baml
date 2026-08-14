//! Regression tests for string character access and iteration.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

fn ok_string(s: &str) -> Result<BexExternalValue, String> {
    Ok(BexExternalValue::String(s.to_string().into()))
}

fn ok_int(i: i64) -> Result<BexExternalValue, String> {
    Ok(BexExternalValue::Int(i))
}

#[tokio::test]
async fn split_empty_delimiter_has_no_padding() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let chars = "aé😀".split("");
            baml.json.stringify(chars.length()) + ":" + chars[0] + chars[1] + chars[2]
        }
    "#
    );
    assert_eq!(
        output.result.map_err(|e| format!("{e:?}")),
        ok_string("3:aé😀")
    );
}

#[tokio::test]
async fn split_empty_string_returns_empty_array() {
    let output = baml_test!(
        r#"
        function main() -> int {
            "".split("").length()
        }
    "#
    );
    assert_eq!(output.result.map_err(|e| format!("{e:?}")), ok_int(0));
}

#[tokio::test]
async fn string_is_iterable_by_character() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let out = "";
            for (let c in "aé😀") {
                out += "[" + c + "]";
            }
            out
        }
    "#
    );
    assert_eq!(
        output.result.map_err(|e| format!("{e:?}")),
        ok_string("[a][é][😀]")
    );
}

#[tokio::test]
async fn chars_returns_character_array() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let chars = "aé😀".chars();
            baml.json.stringify(chars.length()) + ":" + chars[0] + chars[1] + chars[2]
        }
    "#
    );
    assert_eq!(
        output.result.map_err(|e| format!("{e:?}")),
        ok_string("3:aé😀")
    );
}
