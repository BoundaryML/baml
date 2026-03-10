use crate::{baml_db, baml_tyannotated};

test_deserializer!(
    test_literal_integer_positive,
    "2",
    baml_tyannotated!(2),
    baml_db! {},
    2
);

test_deserializer!(
    test_literal_integer_negative,
    "-42",
    baml_tyannotated!(-42),
    baml_db! {},
    -42
);

test_deserializer!(
    test_literal_integer_zero,
    "0",
    baml_tyannotated!(0),
    baml_db! {},
    0
);

test_deserializer!(
    test_literal_boolean_true,
    "true",
    baml_tyannotated!(true),
    baml_db! {},
    true
);

test_deserializer!(
    test_literal_boolean_false,
    "false",
    baml_tyannotated!(false),
    baml_db! {},
    false
);

test_deserializer!(
    test_literal_string_uppercase_with_double_quotes,
    r#""TWO""#,
    baml_tyannotated!("TWO"),
    baml_db! {},
    "TWO"
);

test_deserializer!(
    test_literal_string_uppercase_without_quotes,
    "TWO",
    baml_tyannotated!("TWO"),
    baml_db! {},
    "TWO"
);

test_deserializer!(
    test_literal_string_mismatched_case,
    "Two",
    baml_tyannotated!("TWO"),
    baml_db! {},
    "TWO"
);

test_deserializer!(
    test_literal_string_lowercase,
    "two",
    baml_tyannotated!("TWO"),
    baml_db! {},
    "TWO"
);

test_deserializer!(
    test_literal_string_preceded_by_extra_text,
    "The answer is TWO",
    baml_tyannotated!("TWO"),
    baml_db! {},
    "TWO"
);

test_deserializer!(
    test_literal_string_preceded_by_extra_text_case_mismatch,
    "The answer is Two",
    baml_tyannotated!("TWO"),
    baml_db! {},
    "TWO"
);

test_deserializer!(
    test_literal_string_followed_by_extra_text,
    "TWO is the answer",
    baml_tyannotated!("TWO"),
    baml_db! {},
    "TWO"
);

test_deserializer!(
    test_literal_string_followed_by_extra_text_case_mismatch,
    "Two is the answer",
    baml_tyannotated!("TWO"),
    baml_db! {},
    "TWO"
);

test_deserializer!(
    test_literal_string_with_quotes_preceded_by_extra_text,
    r#"The answer is "TWO""#,
    baml_tyannotated!("TWO"),
    baml_db! {},
    "TWO"
);

test_deserializer!(
    test_literal_string_with_quotes_preceded_by_extra_text_case_mismatch,
    r#"The answer is "two""#,
    baml_tyannotated!("TWO"),
    baml_db! {},
    "TWO"
);

test_deserializer!(
    test_literal_string_with_quotes_followed_by_extra_text,
    r#""TWO" is the answer"#,
    baml_tyannotated!("TWO"),
    baml_db! {},
    "TWO"
);

test_deserializer!(
    test_literal_string_with_quotes_followed_by_extra_text_case_mismatch,
    r#""Two" is the answer"#,
    baml_tyannotated!("TWO"),
    baml_db! {},
    "TWO"
);

test_deserializer!(
    test_literal_string_case_mismatch_upper,
    // Came up with this example unintentionally but this causes ambiguity
    // issues with unions ("two" | "one"), see the TODO at the end of this file.
    r#"The ansewr "TWO" is the correct one"#,
    baml_tyannotated!("two"),
    baml_db! {},
    "two"
);

test_deserializer!(
    test_literal_string_with_special_characters,
    r#""TWO!@#""#,
    baml_tyannotated!("TWO"),
    baml_db! {},
    "TWO"
);

test_deserializer!(
    test_literal_string_with_whitespace,
    r#""  TWO  ""#,
    baml_tyannotated!("TWO"),
    baml_db! {},
    "TWO"
);

test_deserializer!(
    test_union_literal_integer_positive,
    "2",
    baml_tyannotated!(2 | 3),
    baml_db! {},
    2
);

test_failing_deserializer!(
    test_union_literal_integer_positive_with_both,
    "2 or 3",
    baml_tyannotated!(2 | 3),
    baml_db! {}
);

test_failing_deserializer!(
    test_union_literal_bool_with_both,
    "true or false",
    baml_tyannotated!(2 | 3),
    baml_db! {}
);

// TODO: This one should fail because of ambiguity but we end up picking
// the first option (TWO). For enums it does fail because they are treated
// as one single type whereas unions of literals are treated as separate
// types so the substring match strategy works here.
test_deserializer!(
    test_union_literal_string_with_both,
    "TWO or THREE",
    baml_tyannotated!(("TWO" | "THREE")),
    baml_db! {},
    "TWO"
);

test_deserializer!(
    test_union_literal_with_multiple_types_from_object,
    r#"{
  "status": 1
}"#,
    baml_tyannotated!(1 | true | "THREE"),
    baml_db! {},
    1
);

// Test with integer value
test_deserializer!(
    test_union_literal_with_multiple_types_from_object_int,
    r#"{
  "status": 1
}"#,
    baml_tyannotated!(1 | true | "THREE"),
    baml_db! {},
    1
);

// Test with boolean value
test_deserializer!(
    test_union_literal_with_multiple_types_from_object_bool,
    r#"{
  "result": true
}"#,
    baml_tyannotated!(1 | true | "THREE"),
    baml_db! {},
    true
);

// Test with string value
test_deserializer!(
    test_union_literal_with_multiple_types_from_object_string,
    r#"{
  "value": "THREE"
}"#,
    baml_tyannotated!(1 | true | "THREE"),
    baml_db! {},
    "THREE"
);

test_deserializer!(
    test_ambiguous_literal_string_complete_string,
    r#"
        "pay"
    "#,
    baml_tyannotated!("pay" | "pay_without_credit_card"),
    baml_db! {},
    "pay"
);

test_partial_none_deserializer!(
    test_ambiguous_literal_string,
    r#"
        "pay
    "#,
    baml_tyannotated!("pay" @in_progress(never) | "pay_without_credit_card" @in_progress(never)),
    baml_db! {}
);

// Test with object that has multiple keys (should fail)
test_failing_deserializer!(
    test_union_literal_with_multiple_types_from_multi_key_object,
    r#"{
  "status": 1,
  "message": "success"
}"#,
    baml_tyannotated!(1 | true | "THREE"),
    baml_db! {}
);

// Test with nested object (should fail)
test_failing_deserializer!(
    test_union_literal_with_multiple_types_from_nested_object,
    r#"{
  "status": {
    "code": 1
  }
}"#,
    baml_tyannotated!(1 | true | "THREE"),
    baml_db! {}
);

// Test with quoted string value
test_deserializer!(
    test_union_literal_with_multiple_types_from_object_quoted_string,
    r#"{
  "value": "\"THREE\""
}"#,
    baml_tyannotated!(1 | true | "THREE"),
    baml_db! {},
    "THREE"
);

// Test with string value and extra text
test_deserializer!(
    test_union_literal_with_multiple_types_from_object_string_extra,
    r#"{
  "value": "The answer is THREE"
}"#,
    baml_tyannotated!(1 | true | "THREE"),
    baml_db! {},
    "THREE"
);

// Test with array value (should fail)
test_failing_deserializer!(
    test_union_literal_with_multiple_types_from_object_array,
    r#"{
  "values": [1]
}"#,
    baml_tyannotated!(1 | true | "THREE"),
    baml_db! {}
);

test_partial_deserializer!(
    test_partial_class_with_null_literal,
    r#"{"#,
    baml_tyannotated!(Foo),
    baml_db!{
        class Foo {
            bar: "hello" @class_in_progress_field_missing(null),
        }
    },
    { "bar": null }
);
