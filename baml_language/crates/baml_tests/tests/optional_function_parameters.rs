use baml_tests::baml_test;
use baml_type::Ty;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn source_defaults_named_overrides_and_required_by_name() {
    let output = baml_test!(
        r#"
        function score(
          query: string,
          max_results: int = 10,
          filter: string? = null,
          chunk_size: int = query.length(),
        ) -> int {
          let filter_bonus = if (filter == null) { 0 } else { 100 }
          max_results + chunk_size + filter_bonus
        }

        function main() -> int[] {
          [
            score("cats"),
            score("cats", max_results = 5, filter = "recent", chunk_size = 2),
            score(query = "birds"),
          ]
        }
        "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::Array {
            element_type: Ty::int(),
            items: vec![
                BexExternalValue::Int(14),
                BexExternalValue::Int(107),
                BexExternalValue::Int(15),
            ],
        })
    );
}

#[tokio::test]
async fn mutable_default_array_is_fresh_per_call() {
    let output = baml_test!(
        r#"
        function push_default(value: int, items: int[] = []) -> int {
          items.push(value)
          items.length()
        }

        function main() -> int {
          push_default(1) * 10 + push_default(2)
        }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(11)));
}

#[tokio::test]
async fn optional_dropping_adapter_uses_defaults_for_hidden_params() {
    let output = baml_test!(
        r#"
        function combine(x: int, a: int = 10, b: int = 100) -> int {
          (x * 100) + (a * 10) + b
        }

        function main() -> int {
          let f: (x: int, b?: int) -> int = combine
          f(1, b = 5)
        }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(205)));
}

#[tokio::test]
async fn default_named_call_args_do_not_reuse_body_call_metadata() {
    let output = baml_test!(
        r#"
        function pair(a: int, b: int) -> int {
          (a * 10) + b
        }

        function main(value: int = pair(b = 2, a = 1)) -> int {
          pair(a = value, b = 0)
        }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(120)));
}

#[tokio::test]
async fn host_call_omitted_default_and_explicit_null_are_distinct() {
    let source = r#"
        function classify(value: int? = 7) -> int {
          if (value == null) { 1 } else { value }
        }
    "#;

    let omitted = baml_test!(baml: source, entry: "classify");
    assert_eq!(omitted.result, Ok(BexExternalValue::Int(7)));

    let explicit_null = baml_test!(
        baml: source,
        entry: "classify",
        args: { "value" => BexExternalValue::Null },
    );
    assert_eq!(explicit_null.result, Ok(BexExternalValue::Int(1)));
}
