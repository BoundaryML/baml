use baml_types::LiteralValue;

use super::*;

test_deserializer!(
    test_literal_integer,
    EMPTY_FILE,
    "2",
    FieldType::Literal(LiteralValue::Int(2)),
    2
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
    r#"TWO"#,
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
