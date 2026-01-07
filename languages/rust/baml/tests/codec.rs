//! Tests for `BamlEncode` and `BamlDecode` traits.
#![allow(clippy::approx_constant, clippy::cast_possible_wrap)]

mod common;

use baml::{
    __internal::{CffiStreamState, host_value},
    BamlDecode, BamlEncode, CheckStatus, Checked, StreamState, StreamingState,
};
use common::{
    make_bool_holder, make_checked_holder, make_float_holder, make_int_holder, make_list_holder,
    make_null_holder, make_stream_state_holder, make_string_holder,
};

// =============================================================================
// BamlDecode tests
// =============================================================================

mod decode {
    use super::*;

    #[test]
    fn string() {
        let holder = make_string_holder("hello");
        let result: String = BamlDecode::baml_decode(&holder).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn int() {
        let holder = make_int_holder(42);
        let result: i64 = BamlDecode::baml_decode(&holder).unwrap();
        assert_eq!(result, 42);
    }

    #[test]
    fn float() {
        let holder = make_float_holder(3.14);
        let result: f64 = BamlDecode::baml_decode(&holder).unwrap();
        assert!((result - 3.14).abs() < f64::EPSILON);
    }

    #[test]
    fn bool() {
        let holder = make_bool_holder(true);
        let result: bool = BamlDecode::baml_decode(&holder).unwrap();
        assert!(result);
    }

    #[test]
    fn vec() {
        let holder = make_list_holder(vec![
            make_int_holder(1),
            make_int_holder(2),
            make_int_holder(3),
        ]);
        let result: Vec<i64> = BamlDecode::baml_decode(&holder).unwrap();
        assert_eq!(result, vec![1, 2, 3]);
    }

    #[test]
    fn option_some() {
        let holder = make_string_holder("value");
        let result: Option<String> = BamlDecode::baml_decode(&holder).unwrap();
        assert_eq!(result, Some("value".to_string()));
    }

    #[test]
    fn option_none() {
        let holder = make_null_holder();
        let result: Option<String> = BamlDecode::baml_decode(&holder).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn type_mismatch_returns_error() {
        let holder = make_string_holder("hello");
        let result: Result<i64, _> = BamlDecode::baml_decode(&holder);
        assert!(result.is_err());
    }
}

// =============================================================================
// BamlEncode tests
// =============================================================================

mod encode {
    use super::*;

    #[test]
    fn string() {
        let value = "hello".to_string();
        let encoded = value.baml_encode();
        assert!(matches!(
            encoded.value,
            Some(host_value::Value::StringValue(s)) if s == "hello"
        ));
    }

    #[test]
    fn str_ref() {
        let encoded = "hello".baml_encode();
        assert!(matches!(
            encoded.value,
            Some(host_value::Value::StringValue(s)) if s == "hello"
        ));
    }

    #[test]
    fn i64() {
        let encoded = 42i64.baml_encode();
        assert!(matches!(
            encoded.value,
            Some(host_value::Value::IntValue(42))
        ));
    }

    #[test]
    fn i32() {
        let encoded = 42i32.baml_encode();
        assert!(matches!(
            encoded.value,
            Some(host_value::Value::IntValue(42))
        ));
    }

    #[test]
    fn f64() {
        let encoded = 3.14f64.baml_encode();
        if let Some(host_value::Value::FloatValue(f)) = encoded.value {
            assert!((f - 3.14).abs() < f64::EPSILON);
        } else {
            panic!("expected float value");
        }
    }

    #[test]
    fn bool() {
        let encoded = true.baml_encode();
        assert!(matches!(
            encoded.value,
            Some(host_value::Value::BoolValue(true))
        ));
    }

    #[test]
    fn vec() {
        let vec = vec![1i64, 2, 3];
        let encoded = vec.baml_encode();
        if let Some(host_value::Value::ListValue(list)) = encoded.value {
            assert_eq!(list.values.len(), 3);
        } else {
            panic!("expected list value");
        }
    }

    #[test]
    fn option_some() {
        let opt: Option<String> = Some("hello".to_string());
        let encoded = opt.baml_encode();
        assert!(matches!(
            encoded.value,
            Some(host_value::Value::StringValue(s)) if s == "hello"
        ));
    }

    #[test]
    fn option_none() {
        let opt: Option<String> = None;
        let encoded = opt.baml_encode();
        assert!(encoded.value.is_none());
    }
}

// =============================================================================
// Helper function tests
// =============================================================================

mod helpers {
    use baml::{encode_class, encode_enum};

    use super::*;

    #[test]
    fn encode_class_creates_class_value() {
        let encoded = encode_class(
            "Person",
            vec![
                ("name", "Alice".baml_encode()),
                ("age", 30i64.baml_encode()),
            ],
        );
        if let Some(host_value::Value::ClassValue(class)) = encoded.value {
            assert_eq!(class.name, "Person");
            assert_eq!(class.fields.len(), 2);
        } else {
            panic!("expected class value");
        }
    }

    #[test]
    fn encode_enum_creates_enum_value() {
        let encoded = encode_enum("Color", "Red");
        if let Some(host_value::Value::EnumValue(e)) = encoded.value {
            assert_eq!(e.name, "Color");
            assert_eq!(e.value, "Red");
        } else {
            panic!("expected enum value");
        }
    }
}

// =============================================================================
// Checked<T> tests
// =============================================================================

mod checked {
    use super::*;

    #[test]
    fn decode_with_all_passed_checks() {
        let inner = make_string_holder("test value");
        let holder = make_checked_holder(
            inner,
            vec![
                ("check1", "value.len() > 0", "passed"),
                ("check2", "value != null", "PASSED"),
            ],
        );

        let result: Checked<String> = BamlDecode::baml_decode(&holder).unwrap();
        assert_eq!(result.value, "test value");
        assert_eq!(result.checks.len(), 2);
        assert!(result.all_passed());
        assert!(!result.any_failed());
    }

    #[test]
    fn decode_with_failed_check() {
        let inner = make_int_holder(5);
        let holder = make_checked_holder(
            inner,
            vec![
                ("min_check", "value >= 10", "failed"),
                ("type_check", "is_int(value)", "passed"),
            ],
        );

        let result: Checked<i64> = BamlDecode::baml_decode(&holder).unwrap();
        assert_eq!(result.value, 5);
        assert!(!result.all_passed());
        assert!(result.any_failed());
    }

    #[test]
    fn get_check_returns_correct_check() {
        let inner = make_string_holder("hello");
        let holder = make_checked_holder(
            inner,
            vec![
                ("length_check", "value.len() <= 10", "passed"),
                ("format_check", "is_alpha(value)", "failed"),
            ],
        );

        let result: Checked<String> = BamlDecode::baml_decode(&holder).unwrap();

        let length_check = result.get_check("length_check").unwrap();
        assert_eq!(length_check.name, "length_check");
        assert_eq!(length_check.expression, "value.len() <= 10");
        assert_eq!(length_check.status, CheckStatus::Succeeded);

        let format_check = result.get_check("format_check").unwrap();
        assert_eq!(format_check.status, CheckStatus::Failed);

        assert!(result.get_check("nonexistent").is_none());
    }

    #[test]
    fn decode_checked_with_empty_checks() {
        let inner = make_float_holder(3.14);
        let holder = make_checked_holder(inner, vec![]);

        let result: Checked<f64> = BamlDecode::baml_decode(&holder).unwrap();
        assert!((result.value - 3.14).abs() < f64::EPSILON);
        assert!(result.checks.is_empty());
        assert!(result.all_passed()); // vacuously true
        assert!(!result.any_failed());
    }
}

// =============================================================================
// StreamState<T> tests
// =============================================================================

mod stream_state {
    use super::*;

    #[test]
    fn decode_pending_state() {
        let inner = make_string_holder("partial");
        let holder = make_stream_state_holder(inner, CffiStreamState::Pending);

        let result: StreamState<String> = BamlDecode::baml_decode(&holder).unwrap();
        assert_eq!(result.value, "partial");
        assert_eq!(result.state, StreamingState::Pending);
    }

    #[test]
    fn decode_started_state() {
        let inner = make_int_holder(42);
        let holder = make_stream_state_holder(inner, CffiStreamState::Started);

        let result: StreamState<i64> = BamlDecode::baml_decode(&holder).unwrap();
        assert_eq!(result.value, 42);
        assert_eq!(result.state, StreamingState::Started);
    }

    #[test]
    fn decode_done_state() {
        let inner = make_bool_holder(true);
        let holder = make_stream_state_holder(inner, CffiStreamState::Done);

        let result: StreamState<bool> = BamlDecode::baml_decode(&holder).unwrap();
        assert!(result.value);
        assert_eq!(result.state, StreamingState::Done);
    }

    #[test]
    fn decode_stream_state_with_list() {
        let inner = make_list_holder(vec![make_string_holder("a"), make_string_holder("b")]);
        let holder = make_stream_state_holder(inner, CffiStreamState::Started);

        let result: StreamState<Vec<String>> = BamlDecode::baml_decode(&holder).unwrap();
        assert_eq!(result.value, vec!["a", "b"]);
        assert_eq!(result.state, StreamingState::Started);
    }
}
