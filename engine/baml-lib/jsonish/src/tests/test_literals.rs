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
    test_literal_string_followed_by_extra_text,
    EMPTY_FILE,
    "TWO is the answer",
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
    test_literal_string_with_quotes_followed_by_extra_text,
    EMPTY_FILE,
    r#""TWO" is the answer"#,
    FieldType::Literal(LiteralValue::String("TWO".into())),
    "TWO"
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

test_failing_deserializer!(
    test_union_literal_string_with_both,
    EMPTY_FILE,
    "TWO or THREE",
    FieldType::Union(vec![
        FieldType::Literal(LiteralValue::String("TWO".into())),
        FieldType::Literal(LiteralValue::String("THREE".into())),
    ])
);
