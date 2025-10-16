use minijinja::value::Value;

use crate::baml_value_to_jinja_value::MinijinjaBamlEnumValue;

#[test]
fn test_enum_string_comparison_value_name() {
    // Test that enum compares to value name, not alias
    let enum_val = Value::from_object(MinijinjaBamlEnumValue {
        value: "Refund".to_string(),
        alias: Some("gimmie".to_string()),
        enum_name: "PaymentType".to_string(),
    });
    let value_name = Value::from("Refund");
    let alias_name = Value::from("gimmie");

    // Should equal value name
    assert_eq!(enum_val, value_name);
    assert_eq!(value_name, enum_val); // Commutativity

    // Should NOT equal alias
    assert_ne!(enum_val, alias_name);
    assert_ne!(alias_name, enum_val);
}

#[test]
fn test_enum_string_comparison_no_alias() {
    // Test enum without alias
    let enum_val = Value::from_object(MinijinjaBamlEnumValue {
        value: "Payment".to_string(),
        alias: None,
        enum_name: "PaymentType".to_string(),
    });
    let value_name = Value::from("Payment");

    assert_eq!(enum_val, value_name);
    assert_eq!(value_name, enum_val);
}

#[test]
fn test_enum_to_enum_comparison() {
    // Test that enum-to-enum comparison still works
    let enum1 = Value::from_object(MinijinjaBamlEnumValue {
        value: "Active".to_string(),
        alias: Some("active_status".to_string()),
        enum_name: "Status".to_string(),
    });
    let enum2 = Value::from_object(MinijinjaBamlEnumValue {
        value: "Active".to_string(),
        alias: Some("active_status".to_string()),
        enum_name: "Status".to_string(),
    });
    let enum3 = Value::from_object(MinijinjaBamlEnumValue {
        value: "Inactive".to_string(),
        alias: None,
        enum_name: "Status".to_string(),
    });

    assert_eq!(enum1, enum2);
    assert_ne!(enum1, enum3);
}

#[test]
fn test_enum_ordering() {
    // Test ordering consistency
    let enum_val = Value::from_object(MinijinjaBamlEnumValue {
        value: "Beta".to_string(),
        alias: None,
        enum_name: "GreekLetter".to_string(),
    });
    let alpha = Value::from("Alpha");
    let beta = Value::from("Beta");
    let gamma = Value::from("Gamma");

    assert_eq!(enum_val.cmp(&alpha), std::cmp::Ordering::Greater);
    assert_eq!(alpha.cmp(&enum_val), std::cmp::Ordering::Less);

    assert_eq!(enum_val.cmp(&beta), std::cmp::Ordering::Equal);
    assert_eq!(beta.cmp(&enum_val), std::cmp::Ordering::Equal);

    assert_eq!(enum_val.cmp(&gamma), std::cmp::Ordering::Less);
    assert_eq!(gamma.cmp(&enum_val), std::cmp::Ordering::Greater);
}

#[test]
fn test_enum_display_formatting() {
    // Test that display/render uses alias when available
    let enum_with_alias = MinijinjaBamlEnumValue {
        value: "Refund".to_string(),
        alias: Some("gimmie".to_string()),
        enum_name: "PaymentType".to_string(),
    };
    let enum_no_alias = MinijinjaBamlEnumValue {
        value: "Payment".to_string(),
        alias: None,
        enum_name: "PaymentType".to_string(),
    };

    assert_eq!(format!("{enum_with_alias}"), "gimmie");
    assert_eq!(format!("{enum_no_alias}"), "Payment");
}

#[test]
fn test_enum_case_sensitivity() {
    // Test that comparisons are case-sensitive
    let enum_val = Value::from_object(MinijinjaBamlEnumValue {
        value: "MyValue".to_string(),
        alias: Some("my-value".to_string()),
        enum_name: "MyEnum".to_string(),
    });

    assert_eq!(enum_val, Value::from("MyValue"));
    assert_ne!(enum_val, Value::from("myvalue"));
    assert_ne!(enum_val, Value::from("MYVALUE"));
    assert_ne!(enum_val, Value::from("my-value")); // Alias not used for comparison
}

#[test]
fn test_enum_comparison_with_non_string() {
    // Test that enum doesn't equal non-string types
    let enum_val = Value::from_object(MinijinjaBamlEnumValue {
        value: "123".to_string(),
        alias: None,
        enum_name: "NumberEnum".to_string(),
    });

    assert_ne!(enum_val, Value::from(123));
    assert_ne!(enum_val, Value::from(123.0));
    assert_ne!(enum_val, Value::from(true));
    assert_ne!(enum_val, Value::from(())); // None/null
}
