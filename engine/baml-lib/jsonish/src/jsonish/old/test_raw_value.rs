#[cfg(test)]
macro_rules! validate_lazy_string {
    ($input:expr, $expected:expr) => {
        let parsed = ParsedValue::from_str($input);
        assert_eq!(
            parsed,
            ParsedValue::LazyString($input),
            "Failed to parse string.",
        );
        match parsed.as_inner() {
            Some(v) => assert_eq!(v, $expected, "Parsed value does not match expected"),
            None => assert!(false, "Failed to parse inner value {:?}", parsed),
        }
    };
}

#[cfg(test)]
macro_rules! validate_lazy_string_recursive {
    ($input:expr, $expected:expr) => {
        let parsed = ParsedValue::from_str($input);
        assert_eq!(
            parsed,
            ParsedValue::LazyString($input),
            "Failed to parse string.",
        );
        let v = parsed.as_recursive_inner();
        assert_eq!(v, $expected, "Parsed value does not match expected");
    };
}

#[cfg(test)]
mod tests {
    use crate::deserializer::raw_value::*;

    #[test]
    fn test_null() {
        assert_eq!(ParsedValue::from_str("null"), ParsedValue::Null("null"));
        assert_eq!(ParsedValue::from_str("NULL"), ParsedValue::Null("NULL"));
    }

    #[test]
    fn test_bool() {
        assert_eq!(
            ParsedValue::from_str("true"),
            ParsedValue::Bool("true", true)
        );
        assert_eq!(
            ParsedValue::from_str("false"),
            ParsedValue::Bool("false", false)
        );
        assert_eq!(
            ParsedValue::from_str(" true "),
            ParsedValue::Bool("true", true)
        );
        assert_eq!(
            ParsedValue::from_str(" false "),
            ParsedValue::Bool("false", false)
        );
        // Test case insensitivity
        assert_eq!(
            ParsedValue::from_str("True"),
            ParsedValue::Bool("True", true)
        );
        assert_eq!(
            ParsedValue::from_str("False"),
            ParsedValue::Bool("False", false)
        );
    }

    #[test]
    fn test_number() {
        assert_eq!(
            ParsedValue::from_str("123"),
            ParsedValue::Number("123", N::PosInt(123))
        );
        assert_eq!(
            ParsedValue::from_str("-123"),
            ParsedValue::Number("-123", N::NegInt(-123))
        );
        assert_eq!(
            ParsedValue::from_str("123.45"),
            ParsedValue::Number("123.45", N::Float(123.45))
        );
    }

    #[test]
    fn test_string() {
        assert_eq!(
            ParsedValue::from_str("hello"),
            ParsedValue::LazyString("hello".into())
        );
        assert_eq!(
            ParsedValue::from_str(" hello "),
            ParsedValue::LazyString(" hello ".into())
        );
    }

    #[test]
    fn test_quoted_string() {
        validate_lazy_string!(
            "\"hello\"",
            ParsedValue::ParsedString("\"hello\"", "hello".into())
        );
    }

    #[test]
    fn test_list_basic() {
        validate_lazy_string!(
            r#"[1, 2, 3]"#,
            ParsedValue::Array(
                r#"[1, 2, 3]"#,
                vec![
                    ParsedValue::Number("1", N::PosInt(1)),
                    ParsedValue::Number("2", N::PosInt(2)),
                    ParsedValue::Number("3", N::PosInt(3))
                ]
            )
        );
    }

    #[test]
    fn test_list_mixed_types() {
        validate_lazy_string!(
            r#"[1, "hello", true]"#,
            ParsedValue::Array(
                r#"[1, "hello", true]"#,
                vec![
                    ParsedValue::Number("1", N::PosInt(1)),
                    ParsedValue::ParsedString("\"hello\"", "hello".into()),
                    ParsedValue::Bool("true", true)
                ]
            )
        );
    }

    #[test]
    fn test_list_with_spaces() {
        // With spaces in front, it should still parse lazily
        validate_lazy_string!(
            r#"  [1, "hello", true]"#,
            ParsedValue::Array(
                r#"[1, "hello", true]"#,
                vec![
                    ParsedValue::Number("1", N::PosInt(1)),
                    ParsedValue::ParsedString("\"hello\"", "hello".into()),
                    ParsedValue::Bool("true", true)
                ]
            )
        );
    }

    #[test]
    fn test_object() {
        validate_lazy_string!(
            r#"{"key": "value"}"#,
            ParsedValue::Object(
                r#"{"key": "value"}"#,
                Vec::from_iter(vec![(
                    ParsedValue::ParsedString("\"key\"", "key".into()),
                    ParsedValue::ParsedString("\"value\"", "value".into()),
                )])
            )
        );
    }

    #[test]
    fn test_object_2() {
        validate_lazy_string!(
            r#"{"key": "value", "key2": 123}"#,
            ParsedValue::Object(
                r#"{"key": "value", "key2": 123}"#,
                Vec::from_iter(vec![
                    (
                        ParsedValue::ParsedString("\"key\"", "key".into()),
                        ParsedValue::ParsedString("\"value\"", "value".into()),
                    ),
                    (
                        ParsedValue::ParsedString("\"key2\"", "key2".into()),
                        ParsedValue::Number("123", N::PosInt(123))
                    )
                ])
            )
        );
    }

    #[test]
    fn test_object_with_spaces() {
        // With spaces in front, it should still parse lazily
        validate_lazy_string!(
            r#"  {"key": "value", "key2": 123}"#,
            ParsedValue::Object(
                r#"{"key": "value", "key2": 123}"#,
                Vec::from_iter(vec![
                    (
                        ParsedValue::ParsedString("\"key\"", "key".into()),
                        ParsedValue::ParsedString("\"value\"", "value".into()),
                    ),
                    (
                        ParsedValue::ParsedString("\"key2\"", "key2".into()),
                        ParsedValue::Number("123", N::PosInt(123))
                    )
                ])
            )
        );
    }

    #[test]
    fn test_object_nested() {
        validate_lazy_string_recursive!(
            r#"{"key": {"key2": 123}}"#,
            ParsedValue::Object(
                r#"{"key": {"key2": 123}}"#,
                Vec::from_iter(vec![(
                    ParsedValue::ParsedString("\"key\"", "key".into()),
                    ParsedValue::Object(
                        r#"{"key2": 123}"#,
                        Vec::from_iter(vec![(
                            ParsedValue::ParsedString("\"key2\"", "key2".into()),
                            ParsedValue::Number("123", N::PosInt(123))
                        )])
                    )
                )])
            )
        );
    }

    #[test]
    fn test_object_no_quoted_key() {
        validate_lazy_string!(
            r#"{key: "value"}"#,
            ParsedValue::Object(
                r#"{key: "value"}"#,
                Vec::from_iter(vec![(
                    ParsedValue::ParsedString("key", "key".into()),
                    ParsedValue::ParsedString("\"value\"", "value".into())
                )])
            )
        );
    }
}
