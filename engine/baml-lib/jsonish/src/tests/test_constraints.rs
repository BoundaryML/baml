use super::*;

const CLASS_FOO_INT_STRING: &str = r#"
class Foo {
  age int
    @check(age_lt_10, {{this < 10}})
    @check(age_lt_20, {{this < 20}})
    @assert(nonnegative, {{this >= 0}})
  name string
    @assert(nonempty_name, {{this|length > 0}})
}
"#;

test_deserializer_with_expected_score!(
    test_class_failing_one_check,
    CLASS_FOO_INT_STRING,
    r#"{"age": 11, "name": "Greg"}"#,
    FieldType::Class("Foo".to_string()),
    5
);

test_deserializer_with_expected_score!(
    test_class_failing_two_checks,
    CLASS_FOO_INT_STRING,
    r#"{"age": 21, "name": "Grog"}"#,
    FieldType::Class("Foo".to_string()),
    10
);

test_deserializer_with_expected_score!(
    test_class_failing_assert,
    CLASS_FOO_INT_STRING,
    r#"{"age": -1, "name": "Sam"}"#,
    FieldType::Class("Foo".to_string()),
    50
);

test_deserializer_with_expected_score!(
    test_class_multiple_failing_asserts,
    CLASS_FOO_INT_STRING,
    r#"{"age": -1, "name": ""}"#,
    FieldType::Class("Foo".to_string()),
    100
);

const UNION_WITH_CHECKS: &str = r#"
class Thing1 {
  bar int @check(bar_small, {{ this < 10 }})
}

class Thing2 {
  bar int @check(bar_big, {{ this > 20 }})
}

class Either {
  bar Thing1 | Thing2
  things (Thing1 | Thing2)[] @assert(list_not_too_long, {{this|length < 4}})
}
"#;

test_deserializer_with_expected_score!(
    test_union_decision_from_check,
    UNION_WITH_CHECKS,
    r#"{"bar": 5, "things":[]}"#,
    FieldType::Class("Either".to_string()),
    2
);

test_deserializer_with_expected_score!(
    test_union_decision_from_check_no_good_answer,
    UNION_WITH_CHECKS,
    r#"{"bar": 15, "things":[]}"#,
    FieldType::Class("Either".to_string()),
    7
);

test_deserializer_with_expected_score!(
    test_union_decision_in_list,
    UNION_WITH_CHECKS,
    r#"{"bar": 1, "things":[{"bar": 25}, {"bar": 35}, {"bar": 15}, {"bar": 15}]}"#,
    FieldType::Class("Either".to_string()),
    62
);

const MAP_WITH_CHECKS: &str = r#"
class Foo {
  foo map<string,int> @check(hello_is_10, {{ this["hello"] == 10 }})
}
"#;

test_deserializer_with_expected_score!(
    test_map_with_check,
    MAP_WITH_CHECKS,
    r#"{"foo": {"hello": 10, "there":13}}"#,
    FieldType::Class("Foo".to_string()),
    1
);

test_deserializer_with_expected_score!(
    test_map_with_check_fails,
    MAP_WITH_CHECKS,
    r#"{"foo": {"hello": 11, "there":13}}"#,
    FieldType::Class("Foo".to_string()),
    6
);

const NESTED_CLASS_CONSTRAINTS: &str = r#"
class Outer {
  inner Inner
}

class Inner {
  value int @check(this_le_10, {{ this < 10 }})
}
"#;

test_deserializer_with_expected_score!(
    test_nested_class_constraints,
    NESTED_CLASS_CONSTRAINTS,
    r#"{"inner": {"value": 15}}"#,
    FieldType::Class("Outer".to_string()),
    5
);

const MISSPELLED_CONSTRAINT: &str = r#"
class Foo {
  foo int @description("hi") @check(hi, {{this == 1}})
}
"#;
