//! Runtime tests for `Object::Type` structural equality.
//!
//! Phase 2 of the runtime-type-reflection plan adds:
//! - `==` / `!=` dispatch for two `Object::Type` operands using `baml_type::Ty`'s derived `Eq`.
//! - A `deep_equals_recursive` arm matching two `Object::Type` operands.
//!
//! The only user-visible source of `Object::Type` in the current language is
//! `baml.llm.get_return_type(name)`, so these tests use it to materialize
//! comparable values.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

const LLM_PRELUDE: &str = r##"
client<llm> TestClient {
    provider openai
    options {
        model "gpt-4o-mini"
        api_key "test-key"
        base_url "http://localhost:1234"
    }
}

class Foo { x int }
class Bar { y string }

function GetFoo() -> Foo {
    client TestClient
    prompt #"hi"#
}

function GetBar() -> Bar {
    client TestClient
    prompt #"hi"#
}
"##;

#[tokio::test]
async fn eq_type_values_same() {
    let source = format!(
        r##"
            {LLM_PRELUDE}

            function main() -> bool {{
                let a = baml.llm.get_return_type("GetFoo");
                let b = baml.llm.get_return_type("GetFoo");
                a == b
            }}
        "##
    );

    let output = baml_test!(&source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn eq_type_values_different() {
    let source = format!(
        r##"
            {LLM_PRELUDE}

            function main() -> bool {{
                let a = baml.llm.get_return_type("GetFoo");
                let b = baml.llm.get_return_type("GetBar");
                a == b
            }}
        "##
    );

    let output = baml_test!(&source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn neq_type_values_same() {
    let source = format!(
        r##"
            {LLM_PRELUDE}

            function main() -> bool {{
                let a = baml.llm.get_return_type("GetFoo");
                let b = baml.llm.get_return_type("GetFoo");
                a != b
            }}
        "##
    );

    let output = baml_test!(&source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn neq_type_values_different() {
    let source = format!(
        r##"
            {LLM_PRELUDE}

            function main() -> bool {{
                let a = baml.llm.get_return_type("GetFoo");
                let b = baml.llm.get_return_type("GetBar");
                a != b
            }}
        "##
    );

    let output = baml_test!(&source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn deep_equals_type_values_same() {
    let source = format!(
        r##"
            {LLM_PRELUDE}

            function main() -> bool {{
                let a = baml.llm.get_return_type("GetFoo");
                let b = baml.llm.get_return_type("GetFoo");
                baml.deep_equals(a, b)
            }}
        "##
    );

    let output = baml_test!(&source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn deep_equals_type_values_different() {
    let source = format!(
        r##"
            {LLM_PRELUDE}

            function main() -> bool {{
                let a = baml.llm.get_return_type("GetFoo");
                let b = baml.llm.get_return_type("GetBar");
                baml.deep_equals(a, b)
            }}
        "##
    );

    let output = baml_test!(&source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}
