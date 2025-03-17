use baml_types::LiteralValue;

use super::*;

test_deserializer!(
    test_literal_integer_positive,
    EMPTY_FILE,
    "2",
    FieldType::Literal(LiteralValue::Int(2)),
    2
);

test_deserializer!(
    test_literal_integer_negative,
    EMPTY_FILE,
    "-42",
    FieldType::Literal(LiteralValue::Int(-42)),
    -42
);

test_deserializer!(
    test_literal_integer_zero,
    EMPTY_FILE,
    "0",
    FieldType::Literal(LiteralValue::Int(0)),
    0
);

test_deserializer!(
    test_literal_boolean_true,
    EMPTY_FILE,
    "true",
    FieldType::Literal(LiteralValue::Bool(true)),
    true
);

test_deserializer!(
    test_literal_boolean_false,
    EMPTY_FILE,
    "false",
    FieldType::Literal(LiteralValue::Bool(false)),
    false
);

test_deserializer!(
    test_literal_string_uppercase_with_double_quotes,
    EMPTY_FILE,
    r#""TWO""#,
    FieldType::Literal(LiteralValue::String("TWO".into())),
    "TWO"
);

test_deserializer!(
    test_literal_string_uppercase_without_quotes,
    EMPTY_FILE,
    "TWO",
    FieldType::Literal(LiteralValue::String("TWO".into())),
    "TWO"
);

test_deserializer!(
    test_literal_string_mismatched_case,
    EMPTY_FILE,
    "Two",
    FieldType::Literal(LiteralValue::String("TWO".into())),
    "TWO"
);

test_deserializer!(
    test_literal_string_lowercase,
    EMPTY_FILE,
    "two",
    FieldType::Literal(LiteralValue::String("TWO".into())),
    "TWO"
);

test_deserializer!(
    test_literal_string_preceded_by_extra_text,
    EMPTY_FILE,
    "The answer is TWO",
    FieldType::Literal(LiteralValue::String("TWO".into())),
    "TWO"
);

test_deserializer!(
    test_literal_string_preceded_by_extra_text_case_mismatch,
    EMPTY_FILE,
    "The answer is Two",
    FieldType::Literal(LiteralValue::String("TWO".into())),
    "TWO"
);

test_deserializer!(
    test_literal_string_followed_by_extra_text,
    EMPTY_FILE,
    "TWO is the answer",
    FieldType::Literal(LiteralValue::String("TWO".into())),
    "TWO"
);

test_deserializer!(
    test_literal_string_followed_by_extra_text_case_mismatch,
    EMPTY_FILE,
    "Two is the answer",
    FieldType::Literal(LiteralValue::String("TWO".into())),
    "TWO"
);

test_deserializer!(
    test_literal_string_with_quotes_preceded_by_extra_text,
    EMPTY_FILE,
    r#"The answer is "TWO""#,
    FieldType::Literal(LiteralValue::String("TWO".into())),
    "TWO"
);

test_deserializer!(
    test_literal_string_with_quotes_preceded_by_extra_text_case_mismatch,
    EMPTY_FILE,
    r#"The answer is "two""#,
    FieldType::Literal(LiteralValue::String("TWO".into())),
    "TWO"
);

test_deserializer!(
    test_literal_string_with_quotes_followed_by_extra_text,
    EMPTY_FILE,
    r#""TWO" is the answer"#,
    FieldType::Literal(LiteralValue::String("TWO".into())),
    "TWO"
);

test_deserializer!(
    test_literal_string_with_quotes_followed_by_extra_text_case_mismatch,
    EMPTY_FILE,
    r#""Two" is the answer"#,
    FieldType::Literal(LiteralValue::String("TWO".into())),
    "TWO"
);

test_deserializer!(
    test_literal_string_case_mismatch_upper,
    EMPTY_FILE,
    // Came up with this example unintentioanlly but this causes ambiguity
    // issues with unions ("two" | "one"), see the TODO at the end of this file.
    r#"The ansewr "TWO" is the correct one"#,
    FieldType::Literal(LiteralValue::String("two".into())),
    "two"
);

test_deserializer!(
    test_literal_string_with_special_characters,
    EMPTY_FILE,
    r#""TWO!@#""#,
    FieldType::Literal(LiteralValue::String("TWO".into())),
    "TWO"
);

test_deserializer!(
    test_literal_string_with_whitespace,
    EMPTY_FILE,
    r#""  TWO  ""#,
    FieldType::Literal(LiteralValue::String("TWO".into())),
    "TWO"
);

test_deserializer!(
    test_union_literal_integer_positive,
    EMPTY_FILE,
    "2",
    FieldType::Union(vec![
        FieldType::Literal(LiteralValue::Int(2)),
        FieldType::Literal(LiteralValue::Int(3)),
    ]),
    2
);

test_failing_deserializer!(
    test_union_literal_integer_positive_with_both,
    EMPTY_FILE,
    "2 or 3",
    FieldType::Union(vec![
        FieldType::Literal(LiteralValue::Int(2)),
        FieldType::Literal(LiteralValue::Int(3)),
    ])
);

test_failing_deserializer!(
    test_union_literal_bool_with_both,
    EMPTY_FILE,
    "true or false",
    FieldType::Union(vec![
        FieldType::Literal(LiteralValue::Int(2)),
        FieldType::Literal(LiteralValue::Int(3)),
    ])
);

// TODO: This one should fail because of ambiguity but we end up picking
// the first option (TWO). For enums it does fail because they are treated
// as one single type whereas unions of literals are treated as separate
// types so the substring match strategy works here.
test_deserializer!(
    test_union_literal_string_with_both,
    EMPTY_FILE,
    "TWO or THREE",
    FieldType::Union(vec![
        FieldType::Literal(LiteralValue::String("TWO".into())),
        FieldType::Literal(LiteralValue::String("THREE".into())),
    ]),
    "TWO"
);

test_deserializer!(
    test_union_literal_with_multiple_types_from_object,
    EMPTY_FILE,
    r#"{
  "status": 1
}"#,
    FieldType::Union(vec![
        FieldType::Literal(LiteralValue::Int(1)),
        FieldType::Literal(LiteralValue::Bool(true)),
        FieldType::Literal(LiteralValue::String("THREE".into())),
    ]),
    1
);

// Test with integer value
test_deserializer!(
    test_union_literal_with_multiple_types_from_object_int,
    EMPTY_FILE,
    r#"{
  "status": 1
}"#,
    FieldType::Union(vec![
        FieldType::Literal(LiteralValue::Int(1)),
        FieldType::Literal(LiteralValue::Bool(true)),
        FieldType::Literal(LiteralValue::String("THREE".into())),
    ]),
    1
);

// Test with boolean value
test_deserializer!(
    test_union_literal_with_multiple_types_from_object_bool,
    EMPTY_FILE,
    r#"{
  "result": true
}"#,
    FieldType::Union(vec![
        FieldType::Literal(LiteralValue::Int(1)),
        FieldType::Literal(LiteralValue::Bool(true)),
        FieldType::Literal(LiteralValue::String("THREE".into())),
    ]),
    true
);

// Test with string value
test_deserializer!(
    test_union_literal_with_multiple_types_from_object_string,
    EMPTY_FILE,
    r#"{
  "value": "THREE"
}"#,
    FieldType::Union(vec![
        FieldType::Literal(LiteralValue::Int(1)),
        FieldType::Literal(LiteralValue::Bool(true)),
        FieldType::Literal(LiteralValue::String("THREE".into())),
    ]),
    "THREE"
);

test_deserializer!(
    test_ambiguous_literal_string_complete_string,
    EMPTY_FILE,
    r#"
        "pay"
    "#,
    FieldType::Union(vec![
        FieldType::Literal(LiteralValue::String("pay".into())),
        FieldType::Literal(LiteralValue::String("pay_without_credit_card".into())),
    ]),
    "pay"
);

test_partial_deserializer_streaming_failure!(
    test_ambiguous_literal_string,
    EMPTY_FILE,
    r#"
        "pay
    "#,
    FieldType::Union(vec![
        FieldType::Literal(LiteralValue::String("pay".into())),
        FieldType::Literal(LiteralValue::String("pay_without_credit_card".into())),
    ])
);

// Test with object that has multiple keys (should fail)
test_failing_deserializer!(
    test_union_literal_with_multiple_types_from_multi_key_object,
    EMPTY_FILE,
    r#"{
  "status": 1,
  "message": "success"
}"#,
    FieldType::Union(vec![
        FieldType::Literal(LiteralValue::Int(1)),
        FieldType::Literal(LiteralValue::Bool(true)),
        FieldType::Literal(LiteralValue::String("THREE".into())),
    ])
);

// Test with nested object (should fail)
test_failing_deserializer!(
    test_union_literal_with_multiple_types_from_nested_object,
    EMPTY_FILE,
    r#"{
  "status": {
    "code": 1
  }
}"#,
    FieldType::Union(vec![
        FieldType::Literal(LiteralValue::Int(1)),
        FieldType::Literal(LiteralValue::Bool(true)),
        FieldType::Literal(LiteralValue::String("THREE".into())),
    ])
);

// Test with quoted string value
test_deserializer!(
    test_union_literal_with_multiple_types_from_object_quoted_string,
    EMPTY_FILE,
    r#"{
  "value": "\"THREE\""
}"#,
    FieldType::Union(vec![
        FieldType::Literal(LiteralValue::Int(1)),
        FieldType::Literal(LiteralValue::Bool(true)),
        FieldType::Literal(LiteralValue::String("THREE".into())),
    ]),
    "THREE"
);

// Test with string value and extra text
test_deserializer!(
    test_union_literal_with_multiple_types_from_object_string_extra,
    EMPTY_FILE,
    r#"{
  "value": "The answer is THREE"
}"#,
    FieldType::Union(vec![
        FieldType::Literal(LiteralValue::Int(1)),
        FieldType::Literal(LiteralValue::Bool(true)),
        FieldType::Literal(LiteralValue::String("THREE".into())),
    ]),
    "THREE"
);

// Test with array value (should fail)
test_failing_deserializer!(
    test_union_literal_with_multiple_types_from_object_array,
    EMPTY_FILE,
    r#"{
  "values": [1]
}"#,
    FieldType::Union(vec![
        FieldType::Literal(LiteralValue::Int(1)),
        FieldType::Literal(LiteralValue::Bool(true)),
        FieldType::Literal(LiteralValue::String("THREE".into())),
    ])
);

test_partial_deserializer!(
    test_partial_class_with_null_literal,
    r#"
    class Foo {
      bar "hello"
    }
    "#,
    r#"{}"#,
    FieldType::class("Foo"),
    { "bar": null }
);
