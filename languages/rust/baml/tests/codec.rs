//! Tests for BamlEncode and BamlDecode traits.

mod common;

use baml::__internal::host_value;
use baml::{BamlDecode, BamlEncode};
use common::{
    make_bool_holder, make_float_holder, make_int_holder, make_list_holder, make_null_holder,
    make_string_holder,
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
    use super::*;
    use baml::{encode_class, encode_enum};

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
