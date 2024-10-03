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
    test_literal_boolean,
    EMPTY_FILE,
    "true",
    FieldType::Literal(LiteralValue::Bool(true)),
    true
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
