use super::*;

test_deserializer!(
    test_literal_integer_positive,
    "2",
    literal_int(2),
    empty_db(),
    2
);

test_deserializer!(
    test_literal_integer_negative,
    "-42",
    literal_int(-42),
    empty_db(),
    -42
);

test_deserializer!(
    test_literal_integer_zero,
    "0",
    literal_int(0),
    empty_db(),
    0
);

test_deserializer!(
    test_literal_boolean_true,
    "true",
    literal_bool(true),
    empty_db(),
    true
);

test_deserializer!(
    test_literal_boolean_false,
    "false",
    literal_bool(false),
    empty_db(),
    false
);

test_deserializer!(
    test_literal_string_uppercase_with_double_quotes,
    r#""TWO""#,
    literal_string("TWO"),
    empty_db(),
    "TWO"
);

test_deserializer!(
    test_literal_string_uppercase_without_quotes,
    "TWO",
    literal_string("TWO"),
    empty_db(),
    "TWO"
);

test_deserializer!(
    test_literal_string_mismatched_case,
    "Two",
    literal_string("TWO"),
    empty_db(),
    "TWO"
);

test_deserializer!(
    test_literal_string_lowercase,
    "two",
    literal_string("TWO"),
    empty_db(),
    "TWO"
);

test_deserializer!(
    test_literal_string_preceded_by_extra_text,
    "The answer is TWO",
    literal_string("TWO"),
    empty_db(),
    "TWO"
);

test_deserializer!(
    test_literal_string_preceded_by_extra_text_case_mismatch,
    "The answer is Two",
    literal_string("TWO"),
    empty_db(),
    "TWO"
);

test_deserializer!(
    test_literal_string_followed_by_extra_text,
    "TWO is the answer",
    literal_string("TWO"),
    empty_db(),
    "TWO"
);

test_deserializer!(
    test_literal_string_followed_by_extra_text_case_mismatch,
    "Two is the answer",
    literal_string("TWO"),
    empty_db(),
    "TWO"
);

test_deserializer!(
    test_literal_string_with_quotes_preceded_by_extra_text,
    r#"The answer is "TWO""#,
    literal_string("TWO"),
    empty_db(),
    "TWO"
);

test_deserializer!(
    test_literal_string_with_quotes_preceded_by_extra_text_case_mismatch,
    r#"The answer is "two""#,
    literal_string("TWO"),
    empty_db(),
    "TWO"
);

test_deserializer!(
    test_literal_string_with_quotes_followed_by_extra_text,
    r#""TWO" is the answer"#,
    literal_string("TWO"),
    empty_db(),
    "TWO"
);

test_deserializer!(
    test_literal_string_with_quotes_followed_by_extra_text_case_mismatch,
    r#""Two" is the answer"#,
    literal_string("TWO"),
    empty_db(),
    "TWO"
);

test_deserializer!(
    test_literal_string_case_mismatch_upper,
    // Came up with this example unintentionally but this causes ambiguity
    // issues with unions ("two" | "one"), see the TODO at the end of this file.
    r#"The ansewr "TWO" is the correct one"#,
    literal_string("two"),
    empty_db(),
    "two"
);

test_deserializer!(
    test_literal_string_with_special_characters,
    r#""TWO!@#""#,
    literal_string("TWO"),
    empty_db(),
    "TWO"
);

test_deserializer!(
    test_literal_string_with_whitespace,
    r#""  TWO  ""#,
    literal_string("TWO"),
    empty_db(),
    "TWO"
);

test_deserializer!(
    test_union_literal_integer_positive,
    "2",
    union_of(vec![annotated(literal_int(2)), annotated(literal_int(3)),]),
    empty_db(),
    2
);

test_failing_deserializer!(
    test_union_literal_integer_positive_with_both,
    "2 or 3",
    union_of(vec![annotated(literal_int(2)), annotated(literal_int(3)),]),
    empty_db()
);

test_failing_deserializer!(
    test_union_literal_bool_with_both,
    "true or false",
    union_of(vec![annotated(literal_int(2)), annotated(literal_int(3)),]),
    empty_db()
);

// TODO: This one should fail because of ambiguity but we end up picking
// the first option (TWO). For enums it does fail because they are treated
// as one single type whereas unions of literals are treated as separate
// types so the substring match strategy works here.
test_deserializer!(
    test_union_literal_string_with_both,
    "TWO or THREE",
    union_of(vec![
        annotated(literal_string("TWO")),
        annotated(literal_string("THREE")),
    ]),
    empty_db(),
    "TWO"
);

test_deserializer!(
    test_union_literal_with_multiple_types_from_object,
    r#"{
  "status": 1
}"#,
    union_of(vec![
        annotated(literal_int(1)),
        annotated(literal_bool(true)),
        annotated(literal_string("THREE")),
    ]),
    empty_db(),
    1
);

// Test with integer value
test_deserializer!(
    test_union_literal_with_multiple_types_from_object_int,
    r#"{
  "status": 1
}"#,
    union_of(vec![
        annotated(literal_int(1)),
        annotated(literal_bool(true)),
        annotated(literal_string("THREE")),
    ]),
    empty_db(),
    1
);

// Test with boolean value
test_deserializer!(
    test_union_literal_with_multiple_types_from_object_bool,
    r#"{
  "result": true
}"#,
    union_of(vec![
        annotated(literal_int(1)),
        annotated(literal_bool(true)),
        annotated(literal_string("THREE")),
    ]),
    empty_db(),
    true
);

// Test with string value
test_deserializer!(
    test_union_literal_with_multiple_types_from_object_string,
    r#"{
  "value": "THREE"
}"#,
    union_of(vec![
        annotated(literal_int(1)),
        annotated(literal_bool(true)),
        annotated(literal_string("THREE")),
    ]),
    empty_db(),
    "THREE"
);

test_deserializer!(
    test_ambiguous_literal_string_complete_string,
    r#"
        "pay"
    "#,
    union_of(vec![
        annotated(literal_string("pay")),
        annotated(literal_string("pay_without_credit_card")),
    ]),
    empty_db(),
    "pay"
);

test_partial_failing_deserializer!(
    test_ambiguous_literal_string,
    r#"
        "pay
    "#,
    union_of(vec![
        annotated(literal_string("pay")),
        annotated(literal_string("pay_without_credit_card")),
    ]),
    empty_db()
);

// Test with object that has multiple keys (should fail)
test_failing_deserializer!(
    test_union_literal_with_multiple_types_from_multi_key_object,
    r#"{
  "status": 1,
  "message": "success"
}"#,
    union_of(vec![
        annotated(literal_int(1)),
        annotated(literal_bool(true)),
        annotated(literal_string("THREE")),
    ]),
    empty_db()
);

// Test with nested object (should fail)
test_failing_deserializer!(
    test_union_literal_with_multiple_types_from_nested_object,
    r#"{
  "status": {
    "code": 1
  }
}"#,
    union_of(vec![
        annotated(literal_int(1)),
        annotated(literal_bool(true)),
        annotated(literal_string("THREE")),
    ]),
    empty_db()
);

// Test with quoted string value
test_deserializer!(
    test_union_literal_with_multiple_types_from_object_quoted_string,
    r#"{
  "value": "\"THREE\""
}"#,
    union_of(vec![
        annotated(literal_int(1)),
        annotated(literal_bool(true)),
        annotated(literal_string("THREE")),
    ]),
    empty_db(),
    "THREE"
);

// Test with string value and extra text
test_deserializer!(
    test_union_literal_with_multiple_types_from_object_string_extra,
    r#"{
  "value": "The answer is THREE"
}"#,
    union_of(vec![
        annotated(literal_int(1)),
        annotated(literal_bool(true)),
        annotated(literal_string("THREE")),
    ]),
    empty_db(),
    "THREE"
);

// Test with array value (should fail)
test_failing_deserializer!(
    test_union_literal_with_multiple_types_from_object_array,
    r#"{
  "values": [1]
}"#,
    union_of(vec![
        annotated(literal_int(1)),
        annotated(literal_bool(true)),
        annotated(literal_string("THREE")),
    ]),
    empty_db()
);

test_partial_deserializer!(
    test_partial_class_with_null_literal,
    r#"{}"#,
    class_ty("Foo", vec![
        field("bar", literal_string("hello")),
    ]),
    empty_db(),
    { "bar": null }
);
