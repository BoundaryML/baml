//! Comprehensive tests for `BamlValue` decoding and `FromBamlValue` trait.
#![allow(clippy::approx_constant, clippy::cast_possible_wrap)]

mod common;

use std::collections::HashMap;

use baml::{
    __internal::CffiStreamState, BamlDecode, BamlValue, CheckStatus, DynamicClass, DynamicEnum,
    DynamicUnion, KnownTypes, StreamingState,
};
use common::{
    make_bool_holder, make_checked_holder, make_class_holder, make_enum_holder, make_float_holder,
    make_int_holder, make_list_holder, make_literal_bool_holder, make_literal_int_holder,
    make_literal_string_holder, make_map_holder, make_null_holder, make_stream_state_holder,
    make_string_holder, make_union_holder,
};

// =============================================================================
// Mock KnownTypes for testing
// =============================================================================

/// Empty enum used as a mock for `KnownTypes` in tests.
/// In real usage, `CodeGen` generates Types and `StreamTypes` enums.
#[derive(Debug, Clone)]
enum MockTypes {}

impl KnownTypes for MockTypes {
    fn as_any(&self) -> &dyn std::any::Any {
        match *self {}
    }

    fn type_name(&self) -> &'static str {
        match *self {}
    }
}

#[derive(Debug, Clone)]
enum MockStreamTypes {}

impl KnownTypes for MockStreamTypes {
    fn as_any(&self) -> &dyn std::any::Any {
        match *self {}
    }

    fn type_name(&self) -> &'static str {
        match *self {}
    }
}

/// Type alias for convenience
type TestBamlValue = BamlValue<MockTypes, MockStreamTypes>;

// =============================================================================
// Primitive BamlDecode tests
// =============================================================================

mod decode_primitives {
    use super::*;

    #[test]
    fn string() {
        let holder = make_string_holder("hello world");
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        assert!(matches!(result, BamlValue::String(s) if s == "hello world"));
    }

    #[test]
    fn string_empty() {
        let holder = make_string_holder("");
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        assert!(matches!(result, BamlValue::String(s) if s.is_empty()));
    }

    #[test]
    fn string_unicode() {
        let holder = make_string_holder("Hello \u{1F600} World \u{4E2D}\u{6587}");
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        assert!(
            matches!(result, BamlValue::String(s) if s == "Hello \u{1F600} World \u{4E2D}\u{6587}")
        );
    }

    #[test]
    fn int_positive() {
        let holder = make_int_holder(42);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        assert!(matches!(result, BamlValue::Int(42)));
    }

    #[test]
    fn int_negative() {
        let holder = make_int_holder(-100);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        assert!(matches!(result, BamlValue::Int(-100)));
    }

    #[test]
    fn int_zero() {
        let holder = make_int_holder(0);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        assert!(matches!(result, BamlValue::Int(0)));
    }

    #[test]
    fn int_max() {
        let holder = make_int_holder(i64::MAX);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        assert!(matches!(result, BamlValue::Int(i) if i == i64::MAX));
    }

    #[test]
    fn int_min() {
        let holder = make_int_holder(i64::MIN);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        assert!(matches!(result, BamlValue::Int(i) if i == i64::MIN));
    }

    #[test]
    fn float_positive() {
        let holder = make_float_holder(3.14159);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::Float(f) = result {
            assert!((f - 3.14159).abs() < f64::EPSILON);
        } else {
            panic!("expected Float");
        }
    }

    #[test]
    fn float_negative() {
        let holder = make_float_holder(-273.15);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::Float(f) = result {
            assert!((f - (-273.15)).abs() < f64::EPSILON);
        } else {
            panic!("expected Float");
        }
    }

    #[test]
    fn float_zero() {
        let holder = make_float_holder(0.0);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::Float(f) = result {
            assert!((f - 0.0).abs() < f64::EPSILON);
        } else {
            panic!("expected Float");
        }
    }

    #[test]
    fn bool_true() {
        let holder = make_bool_holder(true);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        assert!(matches!(result, BamlValue::Bool(true)));
    }

    #[test]
    fn bool_false() {
        let holder = make_bool_holder(false);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        assert!(matches!(result, BamlValue::Bool(false)));
    }

    #[test]
    fn null() {
        let holder = make_null_holder();
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        assert!(matches!(result, BamlValue::Null));
    }
}

// =============================================================================
// Container BamlDecode tests
// =============================================================================

mod decode_containers {
    use super::*;

    #[test]
    fn list_of_ints() {
        let holder = make_list_holder(vec![
            make_int_holder(1),
            make_int_holder(2),
            make_int_holder(3),
        ]);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::List(items) = result {
            assert_eq!(items.len(), 3);
            assert!(matches!(items[0], BamlValue::Int(1)));
            assert!(matches!(items[1], BamlValue::Int(2)));
            assert!(matches!(items[2], BamlValue::Int(3)));
        } else {
            panic!("expected List");
        }
    }

    #[test]
    fn list_of_strings() {
        let holder = make_list_holder(vec![
            make_string_holder("a"),
            make_string_holder("b"),
            make_string_holder("c"),
        ]);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::List(items) = result {
            assert_eq!(items.len(), 3);
            assert!(matches!(&items[0], BamlValue::String(s) if s == "a"));
            assert!(matches!(&items[1], BamlValue::String(s) if s == "b"));
            assert!(matches!(&items[2], BamlValue::String(s) if s == "c"));
        } else {
            panic!("expected List");
        }
    }

    #[test]
    fn list_empty() {
        let holder = make_list_holder(vec![]);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::List(items) = result {
            assert!(items.is_empty());
        } else {
            panic!("expected List");
        }
    }

    #[test]
    fn list_mixed_types() {
        let holder = make_list_holder(vec![
            make_string_holder("hello"),
            make_int_holder(42),
            make_bool_holder(true),
            make_null_holder(),
        ]);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::List(items) = result {
            assert_eq!(items.len(), 4);
            assert!(matches!(&items[0], BamlValue::String(s) if s == "hello"));
            assert!(matches!(items[1], BamlValue::Int(42)));
            assert!(matches!(items[2], BamlValue::Bool(true)));
            assert!(matches!(items[3], BamlValue::Null));
        } else {
            panic!("expected List");
        }
    }

    #[test]
    fn list_nested() {
        let holder = make_list_holder(vec![
            make_list_holder(vec![make_int_holder(1), make_int_holder(2)]),
            make_list_holder(vec![make_int_holder(3), make_int_holder(4)]),
        ]);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::List(outer) = result {
            assert_eq!(outer.len(), 2);
            if let BamlValue::List(inner1) = &outer[0] {
                assert_eq!(inner1.len(), 2);
                assert!(matches!(inner1[0], BamlValue::Int(1)));
                assert!(matches!(inner1[1], BamlValue::Int(2)));
            } else {
                panic!("expected inner List");
            }
        } else {
            panic!("expected outer List");
        }
    }

    #[test]
    fn map_of_ints() {
        let holder = make_map_holder(vec![
            ("one", make_int_holder(1)),
            ("two", make_int_holder(2)),
            ("three", make_int_holder(3)),
        ]);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::Map(map) = result {
            assert_eq!(map.len(), 3);
            assert!(matches!(map.get("one"), Some(BamlValue::Int(1))));
            assert!(matches!(map.get("two"), Some(BamlValue::Int(2))));
            assert!(matches!(map.get("three"), Some(BamlValue::Int(3))));
        } else {
            panic!("expected Map");
        }
    }

    #[test]
    fn map_empty() {
        let holder = make_map_holder(vec![]);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::Map(map) = result {
            assert!(map.is_empty());
        } else {
            panic!("expected Map");
        }
    }

    #[test]
    fn map_with_nested_list() {
        let holder = make_map_holder(vec![(
            "items",
            make_list_holder(vec![make_string_holder("a"), make_string_holder("b")]),
        )]);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::Map(map) = result {
            if let Some(BamlValue::List(items)) = map.get("items") {
                assert_eq!(items.len(), 2);
            } else {
                panic!("expected List at 'items'");
            }
        } else {
            panic!("expected Map");
        }
    }
}

// =============================================================================
// Class/Enum/Union BamlDecode tests
// =============================================================================

mod decode_dynamic_types {
    use super::*;

    #[test]
    fn class_simple() {
        let holder = make_class_holder(
            "Person",
            vec![
                ("name", make_string_holder("Alice")),
                ("age", make_int_holder(30)),
            ],
        );
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::DynamicClass(dc) = result {
            assert_eq!(dc.name(), "Person");
            assert!(dc.has_field("name"));
            assert!(dc.has_field("age"));
        } else {
            panic!("expected DynamicClass");
        }
    }

    #[test]
    fn class_empty_fields() {
        let holder = make_class_holder("EmptyClass", vec![]);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::DynamicClass(dc) = result {
            assert_eq!(dc.name(), "EmptyClass");
            assert!(!dc.has_field("anything"));
        } else {
            panic!("expected DynamicClass");
        }
    }

    #[test]
    fn class_nested() {
        let inner_class = make_class_holder("Address", vec![("city", make_string_holder("NYC"))]);
        let holder = make_class_holder(
            "Person",
            vec![
                ("name", make_string_holder("Bob")),
                ("address", inner_class),
            ],
        );
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::DynamicClass(dc) = result {
            assert_eq!(dc.name(), "Person");
            if let Ok(BamlValue::DynamicClass(addr)) = dc.get::<TestBamlValue>("address") {
                assert_eq!(addr.name(), "Address");
            } else {
                panic!("expected nested DynamicClass");
            }
        } else {
            panic!("expected DynamicClass");
        }
    }

    #[test]
    fn class_with_list_field() {
        let holder = make_class_holder(
            "Container",
            vec![(
                "items",
                make_list_holder(vec![make_int_holder(1), make_int_holder(2)]),
            )],
        );
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::DynamicClass(dc) = result {
            if let Ok(BamlValue::List(items)) = dc.get::<TestBamlValue>("items") {
                assert_eq!(items.len(), 2);
            } else {
                panic!("expected List field");
            }
        } else {
            panic!("expected DynamicClass");
        }
    }

    #[test]
    fn enum_simple() {
        let holder = make_enum_holder("Color", "Red");
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::DynamicEnum(de) = result {
            assert_eq!(de.name(), "Color");
            assert_eq!(de.value, "Red");
        } else {
            panic!("expected DynamicEnum");
        }
    }

    #[test]
    fn enum_with_complex_value() {
        let holder = make_enum_holder("HttpStatus", "NOT_FOUND");
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::DynamicEnum(de) = result {
            assert_eq!(de.name(), "HttpStatus");
            assert_eq!(de.value, "NOT_FOUND");
        } else {
            panic!("expected DynamicEnum");
        }
    }

    #[test]
    fn union_with_string() {
        let holder = make_union_holder("StringOrInt", "String", make_string_holder("hello"));
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::DynamicUnion(du) = result {
            assert_eq!(du.name(), "StringOrInt");
            assert_eq!(du.variant_name, "String");
            assert!(matches!(*du.value, BamlValue::String(s) if s == "hello"));
        } else {
            panic!("expected DynamicUnion");
        }
    }

    #[test]
    fn union_with_int() {
        let holder = make_union_holder("StringOrInt", "Int", make_int_holder(42));
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::DynamicUnion(du) = result {
            assert_eq!(du.name(), "StringOrInt");
            assert_eq!(du.variant_name, "Int");
            assert!(matches!(*du.value, BamlValue::Int(42)));
        } else {
            panic!("expected DynamicUnion");
        }
    }

    #[test]
    fn union_with_class() {
        let inner_class = make_class_holder("Person", vec![("name", make_string_holder("Alice"))]);
        let holder = make_union_holder("PersonOrError", "Person", inner_class);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::DynamicUnion(du) = result {
            assert_eq!(du.name(), "PersonOrError");
            assert_eq!(du.variant_name, "Person");
            assert!(matches!(*du.value, BamlValue::DynamicClass(_)));
        } else {
            panic!("expected DynamicUnion");
        }
    }
}

// =============================================================================
// Wrapper (Checked/StreamState) BamlDecode tests
// =============================================================================

mod decode_wrappers {
    use super::*;

    #[test]
    fn checked_string_all_passed() {
        let inner = make_string_holder("valid");
        let holder = make_checked_holder(
            inner,
            vec![
                ("length_check", "len > 0", "passed"),
                ("format_check", "is_alpha", "PASSED"),
            ],
        );
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::Checked(checked) = result {
            assert!(matches!(*checked.value, BamlValue::String(s) if s == "valid"));
            assert_eq!(checked.checks.len(), 2);
            assert_eq!(
                checked.checks["length_check"].status,
                CheckStatus::Succeeded
            );
            assert_eq!(
                checked.checks["format_check"].status,
                CheckStatus::Succeeded
            );
        } else {
            panic!("expected Checked");
        }
    }

    #[test]
    fn checked_int_with_failed() {
        let inner = make_int_holder(5);
        let holder = make_checked_holder(
            inner,
            vec![
                ("min_check", "value >= 10", "failed"),
                ("type_check", "is_int", "passed"),
            ],
        );
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::Checked(checked) = result {
            assert!(matches!(*checked.value, BamlValue::Int(5)));
            assert_eq!(checked.checks["min_check"].status, CheckStatus::Failed);
            assert_eq!(checked.checks["type_check"].status, CheckStatus::Succeeded);
        } else {
            panic!("expected Checked");
        }
    }

    #[test]
    fn checked_empty_checks() {
        let inner = make_float_holder(3.14);
        let holder = make_checked_holder(inner, vec![]);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::Checked(checked) = result {
            if let BamlValue::Float(f) = *checked.value {
                assert!((f - 3.14).abs() < f64::EPSILON);
            } else {
                panic!("expected Float inner");
            }
            assert!(checked.checks.is_empty());
        } else {
            panic!("expected Checked");
        }
    }

    #[test]
    fn checked_with_class_value() {
        let inner = make_class_holder("Person", vec![("name", make_string_holder("Alice"))]);
        let holder = make_checked_holder(inner, vec![("not_null", "value != null", "passed")]);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::Checked(checked) = result {
            assert!(matches!(*checked.value, BamlValue::DynamicClass(_)));
        } else {
            panic!("expected Checked");
        }
    }

    #[test]
    fn stream_state_pending() {
        let inner = make_string_holder("partial");
        let holder = make_stream_state_holder(inner, CffiStreamState::Pending);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::StreamState(ss) = result {
            assert!(matches!(*ss.value, BamlValue::String(s) if s == "partial"));
            assert_eq!(ss.state, StreamingState::Pending);
        } else {
            panic!("expected StreamState");
        }
    }

    #[test]
    fn stream_state_started() {
        let inner = make_int_holder(42);
        let holder = make_stream_state_holder(inner, CffiStreamState::Started);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::StreamState(ss) = result {
            assert!(matches!(*ss.value, BamlValue::Int(42)));
            assert_eq!(ss.state, StreamingState::Started);
        } else {
            panic!("expected StreamState");
        }
    }

    #[test]
    fn stream_state_done() {
        let inner = make_bool_holder(true);
        let holder = make_stream_state_holder(inner, CffiStreamState::Done);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::StreamState(ss) = result {
            assert!(matches!(*ss.value, BamlValue::Bool(true)));
            assert_eq!(ss.state, StreamingState::Done);
        } else {
            panic!("expected StreamState");
        }
    }

    #[test]
    fn stream_state_with_list() {
        let inner = make_list_holder(vec![make_string_holder("a"), make_string_holder("b")]);
        let holder = make_stream_state_holder(inner, CffiStreamState::Started);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::StreamState(ss) = result {
            if let BamlValue::List(items) = &*ss.value {
                assert_eq!(items.len(), 2);
            } else {
                panic!("expected List inner");
            }
            assert_eq!(ss.state, StreamingState::Started);
        } else {
            panic!("expected StreamState");
        }
    }

    #[test]
    fn stream_state_with_class() {
        let inner = make_class_holder("Person", vec![("name", make_string_holder("Alice"))]);
        let holder = make_stream_state_holder(inner, CffiStreamState::Done);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::StreamState(ss) = result {
            assert!(matches!(*ss.value, BamlValue::DynamicClass(_)));
            assert_eq!(ss.state, StreamingState::Done);
        } else {
            panic!("expected StreamState");
        }
    }

    #[test]
    fn stream_state_with_map() {
        let inner = make_map_holder(vec![
            ("key1", make_int_holder(1)),
            ("key2", make_int_holder(2)),
        ]);
        let holder = make_stream_state_holder(inner, CffiStreamState::Started);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::StreamState(ss) = result {
            if let BamlValue::Map(map) = &*ss.value {
                assert_eq!(map.len(), 2);
                assert!(matches!(map.get("key1"), Some(BamlValue::Int(1))));
                assert!(matches!(map.get("key2"), Some(BamlValue::Int(2))));
            } else {
                panic!("expected Map inner");
            }
            assert_eq!(ss.state, StreamingState::Started);
        } else {
            panic!("expected StreamState");
        }
    }

    #[test]
    fn stream_state_with_null() {
        let inner = make_null_holder();
        let holder = make_stream_state_holder(inner, CffiStreamState::Pending);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::StreamState(ss) = result {
            assert!(matches!(*ss.value, BamlValue::Null));
            assert_eq!(ss.state, StreamingState::Pending);
        } else {
            panic!("expected StreamState");
        }
    }

    #[test]
    fn stream_state_with_enum() {
        let inner = make_enum_holder("Status", "Active");
        let holder = make_stream_state_holder(inner, CffiStreamState::Done);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::StreamState(ss) = result {
            if let BamlValue::DynamicEnum(de) = &*ss.value {
                assert_eq!(de.name(), "Status");
                assert_eq!(de.value, "Active");
            } else {
                panic!("expected DynamicEnum inner");
            }
            assert_eq!(ss.state, StreamingState::Done);
        } else {
            panic!("expected StreamState");
        }
    }

    #[test]
    fn checked_with_list_value() {
        let inner = make_list_holder(vec![
            make_int_holder(1),
            make_int_holder(2),
            make_int_holder(3),
        ]);
        let holder = make_checked_holder(inner, vec![("not_empty", "len > 0", "passed")]);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::Checked(checked) = result {
            if let BamlValue::List(items) = &*checked.value {
                assert_eq!(items.len(), 3);
            } else {
                panic!("expected List inner");
            }
            assert_eq!(checked.checks.len(), 1);
        } else {
            panic!("expected Checked");
        }
    }

    #[test]
    fn checked_with_map_value() {
        let inner = make_map_holder(vec![("name", make_string_holder("test"))]);
        let holder = make_checked_holder(
            inner,
            vec![
                ("has_name", "contains(name)", "passed"),
                ("valid_format", "format_ok", "passed"),
            ],
        );
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::Checked(checked) = result {
            if let BamlValue::Map(map) = &*checked.value {
                assert_eq!(map.len(), 1);
                assert!(matches!(map.get("name"), Some(BamlValue::String(s)) if s == "test"));
            } else {
                panic!("expected Map inner");
            }
            assert_eq!(checked.checks.len(), 2);
        } else {
            panic!("expected Checked");
        }
    }

    #[test]
    fn checked_all_failed() {
        let inner = make_string_holder("invalid");
        let holder = make_checked_holder(
            inner,
            vec![
                ("check1", "condition1", "failed"),
                ("check2", "condition2", "FAILED"),
                ("check3", "condition3", "Failed"),
            ],
        );
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::Checked(checked) = result {
            assert_eq!(checked.checks.len(), 3);
            for check in checked.checks.values() {
                assert_eq!(check.status, CheckStatus::Failed);
            }
        } else {
            panic!("expected Checked");
        }
    }

    #[test]
    fn checked_with_null_value() {
        let inner = make_null_holder();
        let holder = make_checked_holder(inner, vec![("is_null", "value == null", "passed")]);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::Checked(checked) = result {
            assert!(matches!(*checked.value, BamlValue::Null));
            assert_eq!(checked.checks["is_null"].status, CheckStatus::Succeeded);
        } else {
            panic!("expected Checked");
        }
    }

    #[test]
    fn checked_with_enum_value() {
        let inner = make_enum_holder("Priority", "High");
        let holder = make_checked_holder(inner, vec![("valid_priority", "in_range", "passed")]);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::Checked(checked) = result {
            if let BamlValue::DynamicEnum(de) = &*checked.value {
                assert_eq!(de.value, "High");
            } else {
                panic!("expected DynamicEnum inner");
            }
        } else {
            panic!("expected Checked");
        }
    }
}

// =============================================================================
// Additional Container BamlDecode tests
// =============================================================================

mod decode_containers_extended {
    use super::*;

    #[test]
    fn list_deeply_nested() {
        // List of lists of lists
        let level3 = make_list_holder(vec![make_int_holder(1), make_int_holder(2)]);
        let level2 = make_list_holder(vec![level3.clone(), level3]);
        let level1 = make_list_holder(vec![level2.clone(), level2]);

        let result: TestBamlValue = BamlDecode::baml_decode(&level1).unwrap();
        if let BamlValue::List(outer) = result {
            assert_eq!(outer.len(), 2);
            if let BamlValue::List(middle) = &outer[0] {
                assert_eq!(middle.len(), 2);
                if let BamlValue::List(inner) = &middle[0] {
                    assert_eq!(inner.len(), 2);
                    assert!(matches!(inner[0], BamlValue::Int(1)));
                } else {
                    panic!("expected inner List");
                }
            } else {
                panic!("expected middle List");
            }
        } else {
            panic!("expected outer List");
        }
    }

    #[test]
    fn map_with_various_value_types() {
        let holder = make_map_holder(vec![
            ("string_val", make_string_holder("hello")),
            ("int_val", make_int_holder(42)),
            ("bool_val", make_bool_holder(true)),
            ("float_val", make_float_holder(3.14)),
            ("null_val", make_null_holder()),
        ]);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::Map(map) = result {
            assert_eq!(map.len(), 5);
            assert!(matches!(map.get("string_val"), Some(BamlValue::String(s)) if s == "hello"));
            assert!(matches!(map.get("int_val"), Some(BamlValue::Int(42))));
            assert!(matches!(map.get("bool_val"), Some(BamlValue::Bool(true))));
            assert!(matches!(map.get("null_val"), Some(BamlValue::Null)));
            if let Some(BamlValue::Float(f)) = map.get("float_val") {
                assert!((*f - 3.14).abs() < f64::EPSILON);
            } else {
                panic!("expected Float");
            }
        } else {
            panic!("expected Map");
        }
    }

    #[test]
    fn map_with_nested_map() {
        let inner_map = make_map_holder(vec![("nested_key", make_string_holder("nested_value"))]);
        let holder = make_map_holder(vec![("outer_key", inner_map)]);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::Map(outer) = result {
            if let Some(BamlValue::Map(inner)) = outer.get("outer_key") {
                assert!(
                    matches!(inner.get("nested_key"), Some(BamlValue::String(s)) if s == "nested_value")
                );
            } else {
                panic!("expected inner Map");
            }
        } else {
            panic!("expected outer Map");
        }
    }

    #[test]
    fn map_with_class_values() {
        let class1 = make_class_holder("Person", vec![("name", make_string_holder("Alice"))]);
        let class2 = make_class_holder("Person", vec![("name", make_string_holder("Bob"))]);
        let holder = make_map_holder(vec![("person1", class1), ("person2", class2)]);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::Map(map) = result {
            assert_eq!(map.len(), 2);
            if let Some(BamlValue::DynamicClass(dc)) = map.get("person1") {
                let name: String = dc.get("name").unwrap();
                assert_eq!(name, "Alice");
            } else {
                panic!("expected DynamicClass at person1");
            }
        } else {
            panic!("expected Map");
        }
    }

    #[test]
    fn list_with_maps() {
        let map1 = make_map_holder(vec![("id", make_int_holder(1))]);
        let map2 = make_map_holder(vec![("id", make_int_holder(2))]);
        let holder = make_list_holder(vec![map1, map2]);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::List(items) = result {
            assert_eq!(items.len(), 2);
            if let BamlValue::Map(map) = &items[0] {
                assert!(matches!(map.get("id"), Some(BamlValue::Int(1))));
            } else {
                panic!("expected Map at index 0");
            }
        } else {
            panic!("expected List");
        }
    }

    #[test]
    fn list_single_element() {
        let holder = make_list_holder(vec![make_string_holder("only one")]);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::List(items) = result {
            assert_eq!(items.len(), 1);
            assert!(matches!(&items[0], BamlValue::String(s) if s == "only one"));
        } else {
            panic!("expected List");
        }
    }

    #[test]
    fn map_single_entry() {
        let holder = make_map_holder(vec![("only_key", make_int_holder(999))]);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::Map(map) = result {
            assert_eq!(map.len(), 1);
            assert!(matches!(map.get("only_key"), Some(BamlValue::Int(999))));
        } else {
            panic!("expected Map");
        }
    }

    #[test]
    fn list_with_enums() {
        let holder = make_list_holder(vec![
            make_enum_holder("Color", "Red"),
            make_enum_holder("Color", "Green"),
            make_enum_holder("Color", "Blue"),
        ]);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::List(items) = result {
            assert_eq!(items.len(), 3);
            if let BamlValue::DynamicEnum(de) = &items[0] {
                assert_eq!(de.value, "Red");
            } else {
                panic!("expected DynamicEnum");
            }
        } else {
            panic!("expected List");
        }
    }
}

// =============================================================================
// Literal BamlDecode tests
// =============================================================================

mod decode_literals {
    use super::*;

    #[test]
    fn literal_string() {
        let holder = make_literal_string_holder("literal value");
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        assert!(matches!(result, BamlValue::String(s) if s == "literal value"));
    }

    #[test]
    fn literal_string_empty() {
        let holder = make_literal_string_holder("");
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        assert!(matches!(result, BamlValue::String(s) if s.is_empty()));
    }

    #[test]
    fn literal_int_positive() {
        let holder = make_literal_int_holder(42);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        assert!(matches!(result, BamlValue::Int(42)));
    }

    #[test]
    fn literal_int_negative() {
        let holder = make_literal_int_holder(-100);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        assert!(matches!(result, BamlValue::Int(-100)));
    }

    #[test]
    fn literal_int_zero() {
        let holder = make_literal_int_holder(0);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        assert!(matches!(result, BamlValue::Int(0)));
    }

    #[test]
    fn literal_bool_true() {
        let holder = make_literal_bool_holder(true);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        assert!(matches!(result, BamlValue::Bool(true)));
    }

    #[test]
    fn literal_bool_false() {
        let holder = make_literal_bool_holder(false);
        let result: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        assert!(matches!(result, BamlValue::Bool(false)));
    }
}

// =============================================================================
// FromBamlValue extraction tests
// =============================================================================

mod from_baml_value {
    use super::*;

    #[test]
    fn extract_string() {
        let holder = make_string_holder("hello");
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let s: String = value.get().unwrap();
        assert_eq!(s, "hello");
    }

    #[test]
    fn extract_int() {
        let holder = make_int_holder(42);
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let i: i64 = value.get().unwrap();
        assert_eq!(i, 42);
    }

    #[test]
    fn extract_float() {
        let holder = make_float_holder(3.14);
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let f: f64 = value.get().unwrap();
        assert!((f - 3.14).abs() < f64::EPSILON);
    }

    #[test]
    fn extract_bool() {
        let holder = make_bool_holder(true);
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let b: bool = value.get().unwrap();
        assert!(b);
    }

    #[test]
    fn extract_unit_from_null() {
        let holder = make_null_holder();
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let _: () = value.get().unwrap();
    }

    #[test]
    fn extract_option_some() {
        let holder = make_string_holder("value");
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let opt: Option<String> = value.get().unwrap();
        assert_eq!(opt, Some("value".to_string()));
    }

    #[test]
    fn extract_option_none() {
        let holder = make_null_holder();
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let opt: Option<String> = value.get().unwrap();
        assert_eq!(opt, None);
    }

    #[test]
    fn extract_vec() {
        let holder = make_list_holder(vec![
            make_int_holder(1),
            make_int_holder(2),
            make_int_holder(3),
        ]);
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let vec: Vec<i64> = value.get().unwrap();
        assert_eq!(vec, vec![1, 2, 3]);
    }

    #[test]
    fn extract_hashmap() {
        let holder = make_map_holder(vec![("a", make_int_holder(1)), ("b", make_int_holder(2))]);
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let map: HashMap<String, i64> = value.get().unwrap();
        assert_eq!(map.get("a"), Some(&1));
        assert_eq!(map.get("b"), Some(&2));
    }

    #[test]
    fn extract_dynamic_class() {
        let holder = make_class_holder(
            "Person",
            vec![
                ("name", make_string_holder("Alice")),
                ("age", make_int_holder(30)),
            ],
        );
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let dc: DynamicClass<MockTypes, MockStreamTypes> = value.get().unwrap();
        assert_eq!(dc.name(), "Person");

        let name: String = dc.get("name").unwrap();
        assert_eq!(name, "Alice");

        let age: i64 = dc.get("age").unwrap();
        assert_eq!(age, 30);
    }

    #[test]
    fn extract_dynamic_enum() {
        let holder = make_enum_holder("Color", "Blue");
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let de: DynamicEnum = value.get().unwrap();
        assert_eq!(de.name(), "Color");
        assert_eq!(de.value, "Blue");
    }

    #[test]
    fn extract_dynamic_union() {
        let holder = make_union_holder("StringOrInt", "String", make_string_holder("hello"));
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let du: DynamicUnion<MockTypes, MockStreamTypes> = value.get().unwrap();
        assert_eq!(du.name(), "StringOrInt");
        assert_eq!(du.variant_name, "String");
    }

    #[test]
    fn extract_identity() {
        let holder = make_string_holder("test");
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let same: TestBamlValue = value.get().unwrap();
        assert!(matches!(same, BamlValue::String(s) if s == "test"));
    }
}

// =============================================================================
// FromBamlValueRef extraction tests
// =============================================================================

mod from_baml_value_ref {
    use super::*;

    #[test]
    fn borrow_str() {
        let holder = make_string_holder("hello");
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let s: &str = value.get_ref().unwrap();
        assert_eq!(s, "hello");
    }

    #[test]
    fn borrow_int() {
        let holder = make_int_holder(42);
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let i: i64 = value.get_ref().unwrap();
        assert_eq!(i, 42);
    }

    #[test]
    fn borrow_float() {
        let holder = make_float_holder(3.14);
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let f: f64 = value.get_ref().unwrap();
        assert!((f - 3.14).abs() < f64::EPSILON);
    }

    #[test]
    fn borrow_bool() {
        let holder = make_bool_holder(true);
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let b: bool = value.get_ref().unwrap();
        assert!(b);
    }

    #[test]
    fn borrow_list_slice() {
        let holder = make_list_holder(vec![make_int_holder(1), make_int_holder(2)]);
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let slice: &[TestBamlValue] = value.get_ref().unwrap();
        assert_eq!(slice.len(), 2);
    }

    #[test]
    fn borrow_map_ref() {
        let holder = make_map_holder(vec![("key", make_int_holder(42))]);
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let map: &HashMap<String, TestBamlValue> = value.get_ref().unwrap();
        assert!(map.contains_key("key"));
    }

    #[test]
    fn borrow_dynamic_class_ref() {
        let holder = make_class_holder("Person", vec![("name", make_string_holder("Alice"))]);
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let dc: &DynamicClass<MockTypes, MockStreamTypes> = value.get_ref().unwrap();
        assert_eq!(dc.name(), "Person");
    }

    #[test]
    fn borrow_dynamic_enum_ref() {
        let holder = make_enum_holder("Color", "Red");
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let de: &DynamicEnum = value.get_ref().unwrap();
        assert_eq!(de.name(), "Color");
    }
}

// =============================================================================
// DynamicClass accessor tests
// =============================================================================

mod dynamic_class_accessors {
    use super::*;

    #[test]
    fn get_string_field() {
        let holder = make_class_holder("Person", vec![("name", make_string_holder("Alice"))]);
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let dc: DynamicClass<MockTypes, MockStreamTypes> = value.get().unwrap();

        let name: String = dc.get("name").unwrap();
        assert_eq!(name, "Alice");
    }

    #[test]
    fn get_ref_string_field() {
        let holder = make_class_holder("Person", vec![("name", make_string_holder("Alice"))]);
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let dc: DynamicClass<MockTypes, MockStreamTypes> = value.get().unwrap();

        let name: &str = dc.get_ref("name").unwrap();
        assert_eq!(name, "Alice");
    }

    #[test]
    fn pop_field() {
        let holder = make_class_holder("Person", vec![("name", make_string_holder("Alice"))]);
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let mut dc: DynamicClass<MockTypes, MockStreamTypes> = value.get().unwrap();

        assert!(dc.has_field("name"));
        let name: String = dc.pop("name").unwrap();
        assert_eq!(name, "Alice");
        assert!(!dc.has_field("name"));
    }

    #[test]
    fn get_nested_class() {
        let inner = make_class_holder("Address", vec![("city", make_string_holder("NYC"))]);
        let holder = make_class_holder("Person", vec![("address", inner)]);
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let dc: DynamicClass<MockTypes, MockStreamTypes> = value.get().unwrap();

        let addr: DynamicClass<MockTypes, MockStreamTypes> = dc.get("address").unwrap();
        let city: String = addr.get("city").unwrap();
        assert_eq!(city, "NYC");
    }

    #[test]
    fn get_optional_field_present() {
        let holder = make_class_holder("Person", vec![("nickname", make_string_holder("Bob"))]);
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let dc: DynamicClass<MockTypes, MockStreamTypes> = value.get().unwrap();

        let nickname: Option<String> = dc.get("nickname").unwrap();
        assert_eq!(nickname, Some("Bob".to_string()));
    }

    #[test]
    fn get_optional_field_null() {
        let holder = make_class_holder("Person", vec![("nickname", make_null_holder())]);
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let dc: DynamicClass<MockTypes, MockStreamTypes> = value.get().unwrap();

        let nickname: Option<String> = dc.get("nickname").unwrap();
        assert_eq!(nickname, None);
    }

    #[test]
    fn get_list_field() {
        let holder = make_class_holder(
            "Container",
            vec![(
                "items",
                make_list_holder(vec![
                    make_int_holder(1),
                    make_int_holder(2),
                    make_int_holder(3),
                ]),
            )],
        );
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let dc: DynamicClass<MockTypes, MockStreamTypes> = value.get().unwrap();

        let items: Vec<i64> = dc.get("items").unwrap();
        assert_eq!(items, vec![1, 2, 3]);
    }

    #[test]
    fn iterate_fields() {
        let holder = make_class_holder(
            "Person",
            vec![
                ("name", make_string_holder("Alice")),
                ("age", make_int_holder(30)),
            ],
        );
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let dc: DynamicClass<MockTypes, MockStreamTypes> = value.get().unwrap();

        let field_names: Vec<&str> = dc.fields().map(|(k, _)| k).collect();
        assert!(field_names.contains(&"name"));
        assert!(field_names.contains(&"age"));
    }

    #[test]
    fn missing_field_error() {
        let holder = make_class_holder("Person", vec![]);
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let dc: DynamicClass<MockTypes, MockStreamTypes> = value.get().unwrap();

        let result: Result<String, _> = dc.get("nonexistent");
        assert!(result.is_err());
    }
}

// =============================================================================
// Error case tests
// =============================================================================

mod error_cases {
    use super::*;

    #[test]
    fn type_mismatch_string_to_int() {
        let holder = make_string_holder("hello");
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let result: Result<i64, _> = value.get();
        assert!(result.is_err());
    }

    #[test]
    fn type_mismatch_int_to_string() {
        let holder = make_int_holder(42);
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let result: Result<String, _> = value.get();
        assert!(result.is_err());
    }

    #[test]
    fn type_mismatch_bool_to_float() {
        let holder = make_bool_holder(true);
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let result: Result<f64, _> = value.get();
        assert!(result.is_err());
    }

    #[test]
    fn type_mismatch_list_to_map() {
        let holder = make_list_holder(vec![make_int_holder(1)]);
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let result: Result<HashMap<String, i64>, _> = value.get();
        assert!(result.is_err());
    }

    #[test]
    fn type_mismatch_map_to_list() {
        let holder = make_map_holder(vec![("key", make_int_holder(1))]);
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let result: Result<Vec<i64>, _> = value.get();
        assert!(result.is_err());
    }

    #[test]
    fn type_mismatch_class_to_enum() {
        let holder = make_class_holder("Person", vec![]);
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let result: Result<DynamicEnum, _> = value.get();
        assert!(result.is_err());
    }

    #[test]
    fn type_mismatch_enum_to_class() {
        let holder = make_enum_holder("Color", "Red");
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let result: Result<DynamicClass<MockTypes, MockStreamTypes>, _> = value.get();
        assert!(result.is_err());
    }

    #[test]
    fn nested_type_mismatch_in_list() {
        let holder = make_list_holder(vec![
            make_string_holder("a"),
            make_int_holder(42), // Wrong type!
        ]);
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let result: Result<Vec<String>, _> = value.get();
        assert!(result.is_err());
    }

    #[test]
    fn nested_type_mismatch_in_map() {
        let holder = make_map_holder(vec![
            ("good", make_int_holder(1)),
            ("bad", make_string_holder("not an int")), // Wrong type!
        ]);
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let result: Result<HashMap<String, i64>, _> = value.get();
        assert!(result.is_err());
    }
}

// =============================================================================
// DynamicUnion unwrapping tests
// =============================================================================

mod dynamic_union_unwrapping {
    use super::*;

    #[test]
    fn union_with_string_unwraps_to_string() {
        // When we have a DynamicUnion wrapping a String (like string | null where the
        // value is string), we should be able to extract it as a String
        let holder = make_union_holder("string", "string", make_string_holder("hello"));
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();

        // Extracting as String should work by unwrapping the union
        let s: String = value.get().unwrap();
        assert_eq!(s, "hello");
    }

    #[test]
    fn union_with_string_unwraps_to_option_string() {
        // This is the real-world case: optional types come as DynamicUnion(string |
        // null) When the value is present (string), we should get Some(string)
        let holder = make_union_holder("string", "string", make_string_holder("AJ"));
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();

        let opt: Option<String> = value.get().unwrap();
        assert_eq!(opt, Some("AJ".to_string()));
    }

    #[test]
    fn union_with_null_unwraps_to_option_none() {
        // When the union contains null, Option<T> should return None
        let holder = make_union_holder("string", "null", make_null_holder());
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();

        let opt: Option<String> = value.get().unwrap();
        assert_eq!(opt, None);
    }

    #[test]
    fn union_with_int_unwraps_to_int() {
        let holder = make_union_holder("int", "int", make_int_holder(42));
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();

        let i: i64 = value.get().unwrap();
        assert_eq!(i, 42);
    }

    #[test]
    fn union_with_bool_unwraps_to_bool() {
        let holder = make_union_holder("bool", "bool", make_bool_holder(true));
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();

        let b: bool = value.get().unwrap();
        assert!(b);
    }

    #[test]
    fn union_with_list_unwraps_to_vec() {
        let holder = make_union_holder(
            "string[]",
            "string[]",
            make_list_holder(vec![
                make_string_holder("a"),
                make_string_holder("b"),
                make_string_holder("c"),
            ]),
        );
        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();

        let vec: Vec<String> = value.get().unwrap();
        assert_eq!(vec, vec!["a", "b", "c"]);
    }

    #[test]
    fn nested_union_unwraps_correctly() {
        // A union wrapping another union (edge case)
        let inner_union = make_union_holder("string", "string", make_string_holder("nested"));
        let outer_union = make_union_holder("string", "string", inner_union);
        let value: TestBamlValue = BamlDecode::baml_decode(&outer_union).unwrap();

        let s: String = value.get().unwrap();
        assert_eq!(s, "nested");
    }
}

// =============================================================================
// Complex/nested structure tests
// =============================================================================

mod complex_structures {
    use super::*;

    #[test]
    fn deeply_nested_class() {
        let level3 = make_class_holder("Level3", vec![("value", make_int_holder(42))]);
        let level2 = make_class_holder("Level2", vec![("child", level3)]);
        let level1 = make_class_holder("Level1", vec![("child", level2)]);

        let value: TestBamlValue = BamlDecode::baml_decode(&level1).unwrap();
        let dc1: DynamicClass<MockTypes, MockStreamTypes> = value.get().unwrap();

        let dc2: DynamicClass<MockTypes, MockStreamTypes> = dc1.get("child").unwrap();
        let dc3: DynamicClass<MockTypes, MockStreamTypes> = dc2.get("child").unwrap();
        let final_value: i64 = dc3.get("value").unwrap();

        assert_eq!(final_value, 42);
    }

    #[test]
    fn list_of_classes() {
        let holder = make_list_holder(vec![
            make_class_holder("Item", vec![("id", make_int_holder(1))]),
            make_class_holder("Item", vec![("id", make_int_holder(2))]),
            make_class_holder("Item", vec![("id", make_int_holder(3))]),
        ]);

        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::List(items) = value {
            assert_eq!(items.len(), 3);
            for (i, item) in items.into_iter().enumerate() {
                let dc: DynamicClass<MockTypes, MockStreamTypes> = item.get().unwrap();
                let id: i64 = dc.get("id").unwrap();
                assert_eq!(id, (i + 1) as i64);
            }
        } else {
            panic!("expected List");
        }
    }

    #[test]
    fn class_with_all_field_types() {
        let holder = make_class_holder(
            "ComplexClass",
            vec![
                ("string_field", make_string_holder("hello")),
                ("int_field", make_int_holder(42)),
                ("float_field", make_float_holder(3.14)),
                ("bool_field", make_bool_holder(true)),
                ("null_field", make_null_holder()),
                (
                    "list_field",
                    make_list_holder(vec![make_int_holder(1), make_int_holder(2)]),
                ),
                (
                    "map_field",
                    make_map_holder(vec![("key", make_string_holder("value"))]),
                ),
                (
                    "nested_class",
                    make_class_holder("Inner", vec![("x", make_int_holder(100))]),
                ),
                ("enum_field", make_enum_holder("Status", "Active")),
            ],
        );

        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        let dc: DynamicClass<MockTypes, MockStreamTypes> = value.get().unwrap();

        assert_eq!(dc.get::<String>("string_field").unwrap(), "hello");
        assert_eq!(dc.get::<i64>("int_field").unwrap(), 42);
        assert!((dc.get::<f64>("float_field").unwrap() - 3.14).abs() < f64::EPSILON);
        assert!(dc.get::<bool>("bool_field").unwrap());
        assert_eq!(dc.get::<Option<String>>("null_field").unwrap(), None);
        assert_eq!(dc.get::<Vec<i64>>("list_field").unwrap(), vec![1, 2]);

        let map: HashMap<String, String> = dc.get("map_field").unwrap();
        assert_eq!(map.get("key"), Some(&"value".to_string()));

        let inner: DynamicClass<MockTypes, MockStreamTypes> = dc.get("nested_class").unwrap();
        assert_eq!(inner.get::<i64>("x").unwrap(), 100);

        let status: DynamicEnum = dc.get("enum_field").unwrap();
        assert_eq!(status.value, "Active");
    }

    #[test]
    fn union_containing_checked_value() {
        let checked_inner = make_checked_holder(
            make_string_holder("validated"),
            vec![("check", "len > 0", "passed")],
        );
        let holder = make_union_holder("MaybeChecked", "Checked", checked_inner);

        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::DynamicUnion(du) = value {
            assert_eq!(du.variant_name, "Checked");
            if let BamlValue::Checked(checked) = *du.value {
                assert!(matches!(*checked.value, BamlValue::String(s) if s == "validated"));
            } else {
                panic!("expected Checked inner");
            }
        } else {
            panic!("expected DynamicUnion");
        }
    }

    #[test]
    fn stream_state_containing_class() {
        let class_holder =
            make_class_holder("StreamedData", vec![("progress", make_int_holder(50))]);
        let holder = make_stream_state_holder(class_holder, CffiStreamState::Started);

        let value: TestBamlValue = BamlDecode::baml_decode(&holder).unwrap();
        if let BamlValue::StreamState(ss) = value {
            assert_eq!(ss.state, StreamingState::Started);
            if let BamlValue::DynamicClass(dc) = *ss.value {
                let progress: i64 = dc.get("progress").unwrap();
                assert_eq!(progress, 50);
            } else {
                panic!("expected DynamicClass inner");
            }
        } else {
            panic!("expected StreamState");
        }
    }
}
