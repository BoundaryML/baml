use bex_engine::BexExternalValue;

const SOURCE: &str = r#"
class Num {
  value float
}

function classify_float(x: float) -> int {
  match (x) {
    -1.5 => 1,
    0.0 => 2,
    _ => 3,
  }
}

function classify_num(x: Num) -> int {
  match (x) {
    Num { value: 0.0 } => 1,
    _ => 2,
  }
}

function negative_literal() -> int {
  classify_float(-1.5)
}

function positive_zero_literal() -> int {
  classify_float(0.0)
}

function negative_zero_matches_positive_pattern() -> int {
  classify_float(-0.0)
}

function positive_zero_matches_negative_pattern() -> int {
  let x: float = 0.0;
  match (x) {
    -0.0 => 1,
    _ => 2,
  }
}

function nan_falls_through() -> int {
  match (float.nan()) {
    0.0 => 1,
    _ => 2,
  }
}

function class_field_zero() -> int {
  classify_num(Num { value: -0.0 })
}
"#;

async fn assert_entry(entry: &str, expected: i64) {
    let output = baml_test!(baml: SOURCE, entry: entry);
    assert_eq!(output.result, Ok(BexExternalValue::Int(expected)));
}

#[tokio::test]
async fn top_level_float_literal_patterns_use_ieee_equality() {
    assert_entry("negative_literal", 1).await;
    assert_entry("positive_zero_literal", 2).await;
    assert_entry("negative_zero_matches_positive_pattern", 2).await;
    assert_entry("positive_zero_matches_negative_pattern", 1).await;
    assert_entry("nan_falls_through", 2).await;
}

#[tokio::test]
async fn class_field_float_literal_patterns_use_ieee_equality() {
    assert_entry("class_field_zero", 1).await;
}
