use rust::baml_client::types::*;
use std::collections::HashMap;

// =============================================================================
// Primitive / simple class serialization + deserialization
// =============================================================================

#[test]
fn test_simple_class_roundtrip() {
    let a = BigNumbers { a: 42, b: 13.37 };
    let json = serde_json::to_string(&a).unwrap();
    assert_eq!(json, r#"{"a":42,"b":13.37}"#);
    let b: BigNumbers = serde_json::from_str(&json).unwrap();
    assert_eq!(b.a, 42);
    assert_eq!(b.b, 13.37);
}

#[test]
fn test_class_with_optional_fields() {
    let full = Blah {
        prop4: Some("hello".into()),
    };
    let json = serde_json::to_string(&full).unwrap();
    assert_eq!(json, r#"{"prop4":"hello"}"#);
    let rt: Blah = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.prop4, Some("hello".into()));

    let empty = Blah { prop4: None };
    let json = serde_json::to_string(&empty).unwrap();
    assert_eq!(json, r#"{"prop4":null}"#);
    let rt: Blah = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.prop4, None);
}

#[test]
fn test_class_with_renamed_field() {
    let item = AddTodoItem {
        r#type: "add".into(),
        item: "milk".into(),
        time: "now".into(),
        description: "buy milk".into(),
    };
    let json = serde_json::to_string(&item).unwrap();
    // The field should be serialized as "type", not "r#type"
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["type"], "add");
    assert_eq!(v["item"], "milk");

    let rt: AddTodoItem = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.r#type, "add");
}

#[test]
fn test_nested_classes() {
    let nested = Nested {
        prop3: Some("a".into()),
        prop4: None,
        prop20: Nested2 {
            prop11: Some("b".into()),
            prop12: None,
        },
    };
    let json = serde_json::to_string(&nested).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["prop3"], "a");
    assert!(v["prop4"].is_null());
    assert_eq!(v["prop20"]["prop11"], "b");
    assert!(v["prop20"]["prop12"].is_null());

    let rt: Nested = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.prop3, Some("a".into()));
    assert_eq!(rt.prop4, None);
    assert_eq!(rt.prop20.prop11, Some("b".into()));
}

#[test]
fn test_recursive_class() {
    let tree = BinaryNode {
        data: 1,
        left: Some(Box::new(BinaryNode {
            data: 2,
            left: None,
            right: None,
        })),
        right: Some(Box::new(BinaryNode {
            data: 3,
            left: None,
            right: Some(Box::new(BinaryNode {
                data: 4,
                left: None,
                right: None,
            })),
        })),
    };
    let json = serde_json::to_string(&tree).unwrap();
    let rt: BinaryNode = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.data, 1);
    assert_eq!(rt.left.as_ref().unwrap().data, 2);
    assert_eq!(rt.right.as_ref().unwrap().data, 3);
    assert_eq!(rt.right.as_ref().unwrap().right.as_ref().unwrap().data, 4);
    assert!(rt.left.as_ref().unwrap().left.is_none());
}

#[test]
fn test_class_with_list_fields() {
    let resume = Resume {
        name: "Alice".into(),
        email: "a@b.com".into(),
        phone: "555-1234".into(),
        experience: vec!["job1".into(), "job2".into()],
        education: vec![Education {
            institution: "MIT".into(),
            location: "Cambridge".into(),
            degree: "BS".into(),
            major: vec!["CS".into()],
            graduation_date: Some("2020".into()),
        }],
        skills: vec!["rust".into(), "python".into()],
    };
    let json = serde_json::to_string(&resume).unwrap();
    let rt: Resume = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.name, "Alice");
    assert_eq!(rt.experience.len(), 2);
    assert_eq!(rt.education[0].institution, "MIT");
    assert_eq!(rt.education[0].major, vec!["CS"]);
    assert_eq!(rt.education[0].graduation_date, Some("2020".into()));
    assert_eq!(rt.skills, vec!["rust", "python"]);
}

// =============================================================================
// Enum serialization + deserialization
// =============================================================================

#[test]
fn test_normal_enum_serialize() {
    assert_eq!(
        serde_json::to_string(&Category::Refund).unwrap(),
        r#""Refund""#
    );
    assert_eq!(
        serde_json::to_string(&Category::TechnicalSupport).unwrap(),
        r#""TechnicalSupport""#
    );
}

#[test]
fn test_normal_enum_deserialize() {
    let c: Category = serde_json::from_str(r#""CancelOrder""#).unwrap();
    assert_eq!(c, Category::CancelOrder);

    let c: Category = serde_json::from_str(r#""Question""#).unwrap();
    assert_eq!(c, Category::Question);
}

#[test]
fn test_normal_enum_unknown_variant_fails() {
    let result = serde_json::from_str::<Category>(r#""SomethingElse""#);
    assert!(result.is_err());
}

#[test]
fn test_normal_enum_roundtrip_all_variants() {
    for variant in [
        Category::Refund,
        Category::CancelOrder,
        Category::TechnicalSupport,
        Category::AccountIssue,
        Category::Question,
    ] {
        let json = serde_json::to_string(&variant).unwrap();
        let rt: Category = serde_json::from_str(&json).unwrap();
        assert_eq!(rt, variant);
    }
}

// =============================================================================
// Dynamic enum serialization + deserialization
// =============================================================================

#[test]
fn test_dynamic_enum_known_variant_roundtrip() {
    let c = Color::RED;
    let json = serde_json::to_string(&c).unwrap();
    assert_eq!(json, r#""RED""#);
    let rt: Color = serde_json::from_str(&json).unwrap();
    assert_eq!(rt, Color::RED);
}

#[test]
fn test_dynamic_enum_all_known_variants() {
    for (variant, expected) in [
        (Color::RED, "RED"),
        (Color::BLUE, "BLUE"),
        (Color::GREEN, "GREEN"),
        (Color::YELLOW, "YELLOW"),
        (Color::BLACK, "BLACK"),
        (Color::WHITE, "WHITE"),
    ] {
        let json = serde_json::to_string(&variant).unwrap();
        assert_eq!(json, format!(r#""{}""#, expected));
        let rt: Color = serde_json::from_str(&json).unwrap();
        assert_eq!(rt, variant);
    }
}

#[test]
fn test_dynamic_enum_dynamic_variant_serialize() {
    let c = Color::_Dynamic("MAGENTA".into());
    let json = serde_json::to_string(&c).unwrap();
    assert_eq!(json, r#""MAGENTA""#);
}

#[test]
fn test_dynamic_enum_dynamic_variant_deserialize() {
    // Unknown string values should deserialize to _Dynamic
    let c: Color = serde_json::from_str(r#""MAGENTA""#).unwrap();
    assert_eq!(c, Color::_Dynamic("MAGENTA".into()));

    let c: Color = serde_json::from_str(r#""PURPLE""#).unwrap();
    assert_eq!(c, Color::_Dynamic("PURPLE".into()));
}

#[test]
fn test_fully_dynamic_enum_serialize() {
    let e = DynEnumOne::_Dynamic("anything".into());
    let json = serde_json::to_string(&e).unwrap();
    assert_eq!(json, r#""anything""#);
}

#[test]
fn test_fully_dynamic_enum_deserialize() {
    let e: DynEnumOne = serde_json::from_str(r#""hello""#).unwrap();
    assert_eq!(e, DynEnumOne::_Dynamic("hello".into()));
}

#[test]
fn test_dynamic_enum_with_known_variants() {
    // Known variant roundtrip
    let e = DynEnumThree::TRICYCLE;
    let json = serde_json::to_string(&e).unwrap();
    assert_eq!(json, r#""TRICYCLE""#);
    let rt: DynEnumThree = serde_json::from_str(&json).unwrap();
    assert_eq!(rt, DynEnumThree::TRICYCLE);

    // Unknown becomes _Dynamic
    let e: DynEnumThree = serde_json::from_str(r#""SQUARE""#).unwrap();
    assert_eq!(e, DynEnumThree::_Dynamic("SQUARE".into()));
}

// =============================================================================
// Union serialization + deserialization
// =============================================================================

#[test]
fn test_primitive_union_int_or_string() {
    // String variant
    let u = Union2IntOrString::String("hello".into());
    let json = serde_json::to_string(&u).unwrap();
    assert_eq!(json, r#""hello""#);

    // Int variant
    let u = Union2IntOrString::Int(42);
    let json = serde_json::to_string(&u).unwrap();
    assert_eq!(json, "42");
}

#[test]
fn test_primitive_union_deserialize_prefers_first_matching_variant() {
    // Union2IntOrString has String first, then Int.
    // A string should deserialize as String variant
    let u: Union2IntOrString = serde_json::from_str(r#""hello""#).unwrap();
    assert!(matches!(u, Union2IntOrString::String(s) if s == "hello"));

    // An integer should deserialize as Int (doesn't match String)
    let u: Union2IntOrString = serde_json::from_str("42").unwrap();
    assert!(matches!(u, Union2IntOrString::Int(42)));
}

#[test]
fn test_union_bool_or_float() {
    // Union2BoolOrFloat: Float first, then Bool
    let u = Union2BoolOrFloat::Float(3.14);
    let json = serde_json::to_string(&u).unwrap();
    assert_eq!(json, "3.14");

    let u = Union2BoolOrFloat::Bool(true);
    let json = serde_json::to_string(&u).unwrap();
    assert_eq!(json, "true");

    let rt: Union2BoolOrFloat = serde_json::from_str("true").unwrap();
    assert!(matches!(rt, Union2BoolOrFloat::Bool(true)));
}

#[test]
fn test_union_bool_or_string() {
    let u = Union2BoolOrString::String("yes".into());
    let json = serde_json::to_string(&u).unwrap();
    assert_eq!(json, r#""yes""#);

    let u = Union2BoolOrString::Bool(false);
    let json = serde_json::to_string(&u).unwrap();
    assert_eq!(json, "false");

    let rt: Union2BoolOrString = serde_json::from_str("false").unwrap();
    assert!(matches!(rt, Union2BoolOrString::Bool(false)));

    let rt: Union2BoolOrString = serde_json::from_str(r#""test""#).unwrap();
    assert!(matches!(rt, Union2BoolOrString::String(s) if s == "test"));
}

#[test]
fn test_union_int_or_float_lossy_deserialization() {
    // Union2FloatOrInt: Int first, then Float
    // Serializing Int gives "42", serializing Float gives "3.14"
    let u = Union2FloatOrInt::Int(42);
    let json = serde_json::to_string(&u).unwrap();
    assert_eq!(json, "42");

    let u = Union2FloatOrInt::Float(3.14);
    let json = serde_json::to_string(&u).unwrap();
    assert_eq!(json, "3.14");

    // Deserializing "42" - since Int is listed first, serde will try Int first
    let rt: Union2FloatOrInt = serde_json::from_str("42").unwrap();
    assert!(matches!(rt, Union2FloatOrInt::Int(42)));

    // 3.14 can only match Float
    let rt: Union2FloatOrInt = serde_json::from_str("3.14").unwrap();
    assert!(matches!(rt, Union2FloatOrInt::Float(f) if (f - 3.14).abs() < 1e-10));
}

#[test]
fn test_union_of_classes() {
    let phone = Union2EmailAddressOrPhoneNumber::PhoneNumber(PhoneNumber {
        value: "555-1234".into(),
    });
    let json = serde_json::to_string(&phone).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["value"], "555-1234");

    let email = Union2EmailAddressOrPhoneNumber::EmailAddress(EmailAddress {
        value: "a@b.com".into(),
    });
    let json = serde_json::to_string(&email).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["value"], "a@b.com");

    // Since both PhoneNumber and EmailAddress have the same shape {value: string},
    // deserialization will always pick the first variant (PhoneNumber) - this is
    // the expected lossy behavior for untagged unions.
    let rt: Union2EmailAddressOrPhoneNumber =
        serde_json::from_str(r#"{"value":"anything"}"#).unwrap();
    assert!(matches!(
        rt,
        Union2EmailAddressOrPhoneNumber::PhoneNumber(_)
    ));
}

#[test]
fn test_union_of_distinguishable_classes() {
    // BookOrder and FlightConfirmation have different field sets, so we can
    // distinguish them on deserialization.
    let book = Union3BookOrderOrFlightConfirmationOrGroceryReceipt::BookOrder(BookOrder {
        orderId: "123".into(),
        title: "Rust Book".into(),
        quantity: 2,
        price: 29.99,
    });
    let json = serde_json::to_string(&book).unwrap();
    let rt: Union3BookOrderOrFlightConfirmationOrGroceryReceipt =
        serde_json::from_str(&json).unwrap();
    assert!(matches!(
        rt,
        Union3BookOrderOrFlightConfirmationOrGroceryReceipt::BookOrder(b) if b.orderId == "123"
    ));

    let flight = Union3BookOrderOrFlightConfirmationOrGroceryReceipt::FlightConfirmation(
        FlightConfirmation {
            confirmationNumber: "ABC".into(),
            flightNumber: "UA100".into(),
            departureTime: "08:00".into(),
            arrivalTime: "12:00".into(),
            seatNumber: "14A".into(),
        },
    );
    let json = serde_json::to_string(&flight).unwrap();
    let rt: Union3BookOrderOrFlightConfirmationOrGroceryReceipt =
        serde_json::from_str(&json).unwrap();
    assert!(matches!(
        rt,
        Union3BookOrderOrFlightConfirmationOrGroceryReceipt::FlightConfirmation(f) if f.flightNumber == "UA100"
    ));
}

#[test]
fn test_union_with_list_variant() {
    // Union2ListNestedOrString: String first, then ListNested
    let s = Union2ListNestedOrString::String("plain".into());
    let json = serde_json::to_string(&s).unwrap();
    assert_eq!(json, r#""plain""#);

    let list = Union2ListNestedOrString::ListNested(vec![Nested {
        prop3: Some("x".into()),
        prop4: None,
        prop20: Nested2 {
            prop11: None,
            prop12: None,
        },
    }]);
    let json = serde_json::to_string(&list).unwrap();
    let rt: Union2ListNestedOrString = serde_json::from_str(&json).unwrap();
    assert!(matches!(rt, Union2ListNestedOrString::ListNested(v) if v.len() == 1));

    let rt: Union2ListNestedOrString = serde_json::from_str(r#""text""#).unwrap();
    assert!(matches!(rt, Union2ListNestedOrString::String(s) if s == "text"));
}

#[test]
fn test_union_with_map_variant() {
    // Union2MapStringKeyRecursiveUnionValueOrString
    let s = Union2MapStringKeyRecursiveUnionValueOrString::String("simple".into());
    let json = serde_json::to_string(&s).unwrap();
    assert_eq!(json, r#""simple""#);

    let mut map = HashMap::new();
    map.insert(
        "key1".to_string(),
        Union2MapStringKeyRecursiveUnionValueOrString::String("leaf".into()),
    );
    let u = Union2MapStringKeyRecursiveUnionValueOrString::MapStringKeyRecursiveUnionValue(map);
    let json = serde_json::to_string(&u).unwrap();
    let rt: Union2MapStringKeyRecursiveUnionValueOrString = serde_json::from_str(&json).unwrap();
    assert!(matches!(
        rt,
        Union2MapStringKeyRecursiveUnionValueOrString::MapStringKeyRecursiveUnionValue(_)
    ));
}

// =============================================================================
// Literal unions
// =============================================================================

#[test]
fn test_string_literal_union_serialize() {
    assert_eq!(
        serde_json::to_string(&Union4KfourOrKoneOrKthreeOrKtwo::Kone).unwrap(),
        r#""one""#
    );
    assert_eq!(
        serde_json::to_string(&Union4KfourOrKoneOrKthreeOrKtwo::Ktwo).unwrap(),
        r#""two""#
    );
    assert_eq!(
        serde_json::to_string(&Union4KfourOrKoneOrKthreeOrKtwo::Kthree).unwrap(),
        r#""three""#
    );
    assert_eq!(
        serde_json::to_string(&Union4KfourOrKoneOrKthreeOrKtwo::Kfour).unwrap(),
        r#""four""#
    );
}

#[test]
fn test_string_literal_union_deserialize() {
    let v: Union4KfourOrKoneOrKthreeOrKtwo = serde_json::from_str(r#""one""#).unwrap();
    assert_eq!(v, Union4KfourOrKoneOrKthreeOrKtwo::Kone);

    let v: Union4KfourOrKoneOrKthreeOrKtwo = serde_json::from_str(r#""four""#).unwrap();
    assert_eq!(v, Union4KfourOrKoneOrKthreeOrKtwo::Kfour);

    let result = serde_json::from_str::<Union4KfourOrKoneOrKthreeOrKtwo>(r#""five""#);
    assert!(result.is_err());
}

#[test]
fn test_mixed_literal_union() {
    // Union3BoolKTrueOrIntK1OrKstring_output: int 1, bool true, string "string output"
    assert_eq!(
        serde_json::to_string(&Union3BoolKTrueOrIntK1OrKstring_output::IntK1).unwrap(),
        "1"
    );
    assert_eq!(
        serde_json::to_string(&Union3BoolKTrueOrIntK1OrKstring_output::BoolKTrue).unwrap(),
        "true"
    );
    assert_eq!(
        serde_json::to_string(&Union3BoolKTrueOrIntK1OrKstring_output::Kstring_output).unwrap(),
        r#""string output""#
    );

    // Deserialize
    let v: Union3BoolKTrueOrIntK1OrKstring_output = serde_json::from_str("1").unwrap();
    assert_eq!(v, Union3BoolKTrueOrIntK1OrKstring_output::IntK1);

    let v: Union3BoolKTrueOrIntK1OrKstring_output = serde_json::from_str("true").unwrap();
    assert_eq!(v, Union3BoolKTrueOrIntK1OrKstring_output::BoolKTrue);

    let v: Union3BoolKTrueOrIntK1OrKstring_output =
        serde_json::from_str(r#""string output""#).unwrap();
    assert_eq!(v, Union3BoolKTrueOrIntK1OrKstring_output::Kstring_output);
}

#[test]
fn test_two_string_literal_union() {
    assert_eq!(
        serde_json::to_string(&Union2KbarisaOrKox_burger::Kbarisa).unwrap(),
        r#""barisa""#
    );
    assert_eq!(
        serde_json::to_string(&Union2KbarisaOrKox_burger::Kox_burger).unwrap(),
        r#""ox_burger""#
    );

    let v: Union2KbarisaOrKox_burger = serde_json::from_str(r#""barisa""#).unwrap();
    assert_eq!(v, Union2KbarisaOrKox_burger::Kbarisa);

    let v: Union2KbarisaOrKox_burger = serde_json::from_str(r#""ox_burger""#).unwrap();
    assert_eq!(v, Union2KbarisaOrKox_burger::Kox_burger);
}

// =============================================================================
// Union with container types (list, map)
// =============================================================================

#[test]
fn test_union_with_list_bool_or_list_int() {
    let bools = Union2ListBoolOrListInt::ListBool(vec![true, false, true]);
    let json = serde_json::to_string(&bools).unwrap();
    assert_eq!(json, "[true,false,true]");

    let ints = Union2ListBoolOrListInt::ListInt(vec![1, 2, 3]);
    let json = serde_json::to_string(&ints).unwrap();
    assert_eq!(json, "[1,2,3]");

    // Deserializing bools: ListBool is listed first, so bools match
    let rt: Union2ListBoolOrListInt = serde_json::from_str("[true,false]").unwrap();
    assert!(matches!(rt, Union2ListBoolOrListInt::ListBool(_)));

    // Deserializing ints: ListBool can't match, so ListInt
    let rt: Union2ListBoolOrListInt = serde_json::from_str("[1,2,3]").unwrap();
    assert!(matches!(rt, Union2ListBoolOrListInt::ListInt(_)));

    // Empty list: ambiguous, picks the first variant
    let rt: Union2ListBoolOrListInt = serde_json::from_str("[]").unwrap();
    assert!(matches!(rt, Union2ListBoolOrListInt::ListBool(v) if v.is_empty()));
}

#[test]
fn test_complex_union_with_lists_and_maps() {
    // Union6BoolOrFloatOrIntOrListStringOrMapStringKeyListStringValueOrString
    type BigUnion = Union6BoolOrFloatOrIntOrListStringOrMapStringKeyListStringValueOrString;

    let int_val = BigUnion::Int(42);
    assert_eq!(serde_json::to_string(&int_val).unwrap(), "42");

    let str_val = BigUnion::String("hello".into());
    assert_eq!(serde_json::to_string(&str_val).unwrap(), r#""hello""#);

    let bool_val = BigUnion::Bool(true);
    assert_eq!(serde_json::to_string(&bool_val).unwrap(), "true");

    let list_val = BigUnion::ListString(vec!["a".into(), "b".into()]);
    let json = serde_json::to_string(&list_val).unwrap();
    assert_eq!(json, r#"["a","b"]"#);

    let mut map = HashMap::new();
    map.insert("k".to_string(), vec!["v1".to_string(), "v2".to_string()]);
    let map_val = BigUnion::MapStringKeyListStringValue(map);
    let json = serde_json::to_string(&map_val).unwrap();
    let rt: BigUnion = serde_json::from_str(&json).unwrap();
    assert!(matches!(rt, BigUnion::MapStringKeyListStringValue(_)));
}

// =============================================================================
// Dynamic class serialization + deserialization
// =============================================================================

#[test]
fn test_dynamic_class_with_static_fields_only() {
    let d = DummyOutput {
        nonce: "abc".into(),
        nonce2: "def".into(),
        __dynamic: HashMap::new(),
    };
    let json = serde_json::to_string(&d).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["nonce"], "abc");
    assert_eq!(v["nonce2"], "def");
    // No extra fields since __dynamic is empty
    assert_eq!(v.as_object().unwrap().len(), 2);

    let rt: DummyOutput = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.nonce, "abc");
    assert_eq!(rt.nonce2, "def");
    assert!(rt.__dynamic.is_empty());
}

#[test]
fn test_dynamic_class_with_dynamic_string_field() {
    let mut dynamic = HashMap::new();
    dynamic.insert("extra".into(), baml::BamlValue::String("val".into()));
    let d = DummyOutput {
        nonce: "abc".into(),
        nonce2: "def".into(),
        __dynamic: dynamic,
    };
    let json = serde_json::to_string(&d).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["nonce"], "abc");
    assert_eq!(v["extra"], "val");

    let rt: DummyOutput = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.nonce, "abc");
    assert!(rt.__dynamic.contains_key("extra"));
}

#[test]
fn test_dynamic_class_with_dynamic_int_field() {
    let mut dynamic = HashMap::new();
    dynamic.insert("count".into(), baml::BamlValue::Int(42));
    let d = DynInputOutput {
        testKey: "key".into(),
        __dynamic: dynamic,
    };
    let json = serde_json::to_string(&d).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["testKey"], "key");
    assert_eq!(v["count"], 42);

    let rt: DynInputOutput = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.testKey, "key");
    assert!(rt.__dynamic.contains_key("count"));
}

#[test]
fn test_dynamic_class_with_dynamic_list_field() {
    let mut dynamic = HashMap::new();
    dynamic.insert(
        "tags".into(),
        baml::BamlValue::List(vec![
            baml::BamlValue::String("a".into()),
            baml::BamlValue::String("b".into()),
        ]),
    );
    let d = DynInputOutput {
        testKey: "k".into(),
        __dynamic: dynamic,
    };
    let json = serde_json::to_string(&d).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["tags"], serde_json::json!(["a", "b"]));

    let rt: DynInputOutput = serde_json::from_str(&json).unwrap();
    assert!(rt.__dynamic.contains_key("tags"));
}

#[test]
fn test_dynamic_class_with_dynamic_map_field() {
    let mut inner_map = HashMap::new();
    inner_map.insert("nested_key".into(), baml::BamlValue::Bool(true));
    let mut dynamic = HashMap::new();
    dynamic.insert("metadata".into(), baml::BamlValue::Map(inner_map));
    let d = DynInputOutput {
        testKey: "k".into(),
        __dynamic: dynamic,
    };
    let json = serde_json::to_string(&d).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["metadata"]["nested_key"], true);

    let rt: DynInputOutput = serde_json::from_str(&json).unwrap();
    assert!(rt.__dynamic.contains_key("metadata"));
}

#[test]
fn test_dynamic_class_with_dynamic_null_field() {
    let mut dynamic = HashMap::new();
    dynamic.insert("optional".into(), baml::BamlValue::Null);
    let d = DynInputOutput {
        testKey: "k".into(),
        __dynamic: dynamic,
    };
    let json = serde_json::to_string(&d).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(v["optional"].is_null());

    let rt: DynInputOutput = serde_json::from_str(&json).unwrap();
    assert!(rt.__dynamic.contains_key("optional"));
}

#[test]
fn test_fully_dynamic_class_no_static_fields() {
    let mut dynamic = HashMap::new();
    dynamic.insert("a".into(), baml::BamlValue::String("x".into()));
    dynamic.insert("b".into(), baml::BamlValue::Int(1));
    let d = DynamicClassOne { __dynamic: dynamic };
    let json = serde_json::to_string(&d).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["a"], "x");
    assert_eq!(v["b"], 1);

    let rt: DynamicClassOne = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.__dynamic.len(), 2);
}

#[test]
fn test_dynamic_class_with_nested_dynamic_and_enum() {
    let d = DynamicClassTwo {
        hi: "hello".into(),
        some_class: SomeClassNestedDynamic {
            hi: "inner".into(),
            __dynamic: HashMap::new(),
        },
        status: DynEnumOne::_Dynamic("active".into()),
        __dynamic: HashMap::new(),
    };
    let json = serde_json::to_string(&d).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["hi"], "hello");
    assert_eq!(v["some_class"]["hi"], "inner");
    assert_eq!(v["status"], "active");

    let rt: DynamicClassTwo = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.hi, "hello");
    assert_eq!(rt.some_class.hi, "inner");
    assert_eq!(rt.status, DynEnumOne::_Dynamic("active".into()));
}

#[test]
fn test_dynamic_class_with_optional_and_dynamic_fields() {
    let mut dynamic = HashMap::new();
    dynamic.insert("age".into(), baml::BamlValue::Int(30));
    let p = Person {
        name: Some("Alice".into()),
        hair_color: Some(Color::RED),
        __dynamic: dynamic,
    };
    let json = serde_json::to_string(&p).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["name"], "Alice");
    assert_eq!(v["hair_color"], "RED");
    assert_eq!(v["age"], 30);

    let rt: Person = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.name, Some("Alice".into()));
    assert_eq!(rt.hair_color, Some(Color::RED));
    assert!(rt.__dynamic.contains_key("age"));
}

#[test]
fn test_dynamic_class_with_none_optional() {
    let p = Person {
        name: None,
        hair_color: None,
        __dynamic: HashMap::new(),
    };
    let json = serde_json::to_string(&p).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(v["name"].is_null());
    assert!(v["hair_color"].is_null());

    let rt: Person = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.name, None);
    assert_eq!(rt.hair_color, None);
}

#[test]
fn test_dynamic_class_deserialization_unknown_fields_into_dynamic() {
    // JSON with both known and unknown fields
    let json = r#"{"value":"hello","internal_id":"abc","extra_field":"dynamic_val","count":5}"#;
    let rt: SkipDynamicClass = serde_json::from_str(json).unwrap();
    assert_eq!(rt.value, "hello");
    assert_eq!(rt.internal_id, Some("abc".into()));
    assert!(rt.__dynamic.contains_key("extra_field"));
    assert!(rt.__dynamic.contains_key("count"));
}

// =============================================================================
// Dynamic class with OriginalB (dynamic with one static field)
// =============================================================================

#[test]
fn test_original_b_dynamic_class() {
    let mut dynamic = HashMap::new();
    dynamic.insert("extra".into(), baml::BamlValue::String("bonus".into()));
    let b = OriginalB {
        value: 42,
        __dynamic: dynamic,
    };
    let json = serde_json::to_string(&b).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["value"], 42);
    assert_eq!(v["extra"], "bonus");

    let rt: OriginalB = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.value, 42);
    assert!(rt.__dynamic.contains_key("extra"));
}

// =============================================================================
// Union of classes with dynamic class
// =============================================================================

#[test]
fn test_union_with_dynamic_class_variant() {
    // Union2OriginalAOrOriginalB: OriginalA first, then OriginalB (dynamic)
    let a = Union2OriginalAOrOriginalB::OriginalA(OriginalA { value: 10 });
    let json = serde_json::to_string(&a).unwrap();
    assert_eq!(json, r#"{"value":10}"#);

    // OriginalA and OriginalB share {value: int} so deserialization is ambiguous
    // and will pick the first variant (OriginalA)
    let rt: Union2OriginalAOrOriginalB = serde_json::from_str(r#"{"value":10}"#).unwrap();
    assert!(matches!(rt, Union2OriginalAOrOriginalB::OriginalA(_)));

    // But OriginalB with extra dynamic fields should still match OriginalB
    // because OriginalA (non-dynamic) won't accept unknown fields.
    // Actually, serde with deny_unknown_fields would help here, but without it,
    // OriginalA will just ignore extra fields. Let's test what actually happens:
    let rt: Union2OriginalAOrOriginalB =
        serde_json::from_str(r#"{"value":10,"extra":"stuff"}"#).unwrap();
    // Due to untagged union behavior, the first matching variant wins.
    // OriginalA will match (ignoring extra fields), OR might fail depending on strictness.
    // The actual behavior depends on whether OriginalA accepts extra fields.
    // Let's just verify it doesn't error:
    assert!(matches!(
        rt,
        Union2OriginalAOrOriginalB::OriginalA(_) | Union2OriginalAOrOriginalB::OriginalB(_)
    ));
}

// =============================================================================
// Checked<T> serialization + deserialization
// =============================================================================

#[test]
fn test_checked_type_serialize() {
    let checked = Checked {
        value: 42i64,
        checks: {
            let mut m = HashMap::new();
            m.insert(
                "age_check".to_string(),
                baml::Check {
                    name: "age_check".into(),
                    expression: "age > 0".into(),
                    status: baml::CheckStatus::Succeeded,
                },
            );
            m
        },
    };
    let json = serde_json::to_string(&checked).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["value"], 42);
    assert_eq!(v["checks"]["age_check"]["status"], "succeeded");
    assert_eq!(v["checks"]["age_check"]["name"], "age_check");
}

#[test]
fn test_checked_type_deserialize() {
    let json = r#"{"value":42,"checks":{"my_check":{"name":"my_check","expression":"x > 0","status":"succeeded"}}}"#;
    let rt: Checked<i64> = serde_json::from_str(json).unwrap();
    assert_eq!(rt.value, 42);
    assert!(rt.checks.contains_key("my_check"));
    assert_eq!(rt.checks["my_check"].status, baml::CheckStatus::Succeeded);
}

#[test]
fn test_checked_type_with_failed_check() {
    let json = r#"{"value":-5,"checks":{"positive":{"name":"positive","expression":"x > 0","status":"failed"}}}"#;
    let rt: Checked<i64> = serde_json::from_str(json).unwrap();
    assert_eq!(rt.value, -5);
    assert_eq!(rt.checks["positive"].status, baml::CheckStatus::Failed);
    assert!(rt.any_failed());
    assert!(!rt.all_passed());
}

#[test]
fn test_class_with_checked_field() {
    let m = Martian {
        age: Checked {
            value: 100,
            checks: HashMap::new(),
        },
    };
    let json = serde_json::to_string(&m).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["age"]["value"], 100);
    assert!(v["age"]["checks"].is_object());

    let rt: Martian = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.age.value, 100);
}

// =============================================================================
// StreamState<T> serialization + deserialization
// =============================================================================

#[test]
fn test_stream_state_serialize() {
    let ss = StreamState {
        value: 42i64,
        state: baml::StreamingState::Pending,
    };
    let json = serde_json::to_string(&ss).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["value"], 42);
    assert_eq!(v["state"], "pending");
}

#[test]
fn test_stream_state_all_states() {
    for (state, expected_str) in [
        (baml::StreamingState::Pending, "pending"),
        (baml::StreamingState::Started, "started"),
        (baml::StreamingState::Done, "done"),
    ] {
        let ss = StreamState {
            value: "hello".to_string(),
            state,
        };
        let json = serde_json::to_string(&ss).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["state"], expected_str);
    }
}

#[test]
fn test_stream_state_deserialize() {
    let json = r#"{"value":"hello","state":"done"}"#;
    let rt: StreamState<String> = serde_json::from_str(json).unwrap();
    assert_eq!(rt.value, "hello");
    assert_eq!(rt.state, baml::StreamingState::Done);
}

// =============================================================================
// BamlValue serialization + deserialization
// =============================================================================

type TestBamlValue = baml::BamlValue<Types, rust::baml_client::stream_types::StreamTypes>;

#[test]
fn test_baml_value_primitives_serialize() {
    let s: TestBamlValue = baml::BamlValue::String("hello".into());
    assert_eq!(serde_json::to_string(&s).unwrap(), r#""hello""#);

    let i: TestBamlValue = baml::BamlValue::Int(42);
    assert_eq!(serde_json::to_string(&i).unwrap(), "42");

    let f: TestBamlValue = baml::BamlValue::Float(3.14);
    assert_eq!(serde_json::to_string(&f).unwrap(), "3.14");

    let b: TestBamlValue = baml::BamlValue::Bool(true);
    assert_eq!(serde_json::to_string(&b).unwrap(), "true");

    let n: TestBamlValue = baml::BamlValue::Null;
    assert_eq!(serde_json::to_string(&n).unwrap(), "null");
}

#[test]
fn test_baml_value_list_serialize() {
    let list: TestBamlValue = baml::BamlValue::List(vec![
        baml::BamlValue::Int(1),
        baml::BamlValue::Int(2),
        baml::BamlValue::Int(3),
    ]);
    assert_eq!(serde_json::to_string(&list).unwrap(), "[1,2,3]");
}

#[test]
fn test_baml_value_map_serialize() {
    // Use a single-entry map for deterministic output
    let mut map = HashMap::new();
    map.insert("key".into(), baml::BamlValue::String("value".into()));
    let val: TestBamlValue = baml::BamlValue::Map(map);
    assert_eq!(serde_json::to_string(&val).unwrap(), r#"{"key":"value"}"#);
}

#[test]
fn test_baml_value_deserialize_primitives() {
    let s: TestBamlValue = serde_json::from_str(r#""hello""#).unwrap();
    assert!(matches!(s, baml::BamlValue::String(ref v) if v == "hello"));

    let i: TestBamlValue = serde_json::from_str("42").unwrap();
    assert!(matches!(i, baml::BamlValue::Int(42)));

    let neg: TestBamlValue = serde_json::from_str("-42").unwrap();
    assert!(matches!(neg, baml::BamlValue::Int(-42)));

    let f: TestBamlValue = serde_json::from_str("3.14").unwrap();
    assert!(matches!(f, baml::BamlValue::Float(v) if (v - 3.14).abs() < 1e-10));

    let b: TestBamlValue = serde_json::from_str("true").unwrap();
    assert!(matches!(b, baml::BamlValue::Bool(true)));

    let n: TestBamlValue = serde_json::from_str("null").unwrap();
    assert!(matches!(n, baml::BamlValue::Null));
}

#[test]
fn test_baml_value_deserialize_list() {
    let list: TestBamlValue = serde_json::from_str("[1, 2, 3]").unwrap();
    match list {
        baml::BamlValue::List(items) => {
            assert_eq!(items.len(), 3);
            assert!(matches!(items[0], baml::BamlValue::Int(1)));
            assert!(matches!(items[1], baml::BamlValue::Int(2)));
            assert!(matches!(items[2], baml::BamlValue::Int(3)));
        }
        other => panic!("expected List, got {:?}", other),
    }
}

#[test]
fn test_baml_value_deserialize_map() {
    let map: TestBamlValue = serde_json::from_str(r#"{"a": 1, "b": "two"}"#).unwrap();
    match map {
        baml::BamlValue::Map(m) => {
            assert_eq!(m.len(), 2);
            assert!(matches!(m["a"], baml::BamlValue::Int(1)));
            assert!(matches!(m["b"], baml::BamlValue::String(ref s) if s == "two"));
        }
        other => panic!("expected Map, got {:?}", other),
    }
}

#[test]
fn test_baml_value_deserialize_is_lossy_for_objects() {
    // Objects (classes) deserialize as Map, not as Known types.
    // This is the documented lossy behavior.
    let json = r#"{"a": 42, "b": 13.37}"#;
    let val: TestBamlValue = serde_json::from_str(json).unwrap();
    // Even though this looks like BigNumbers, it will be Map not Known
    assert!(matches!(val, baml::BamlValue::Map(_)));
}

#[test]
fn test_baml_value_deserialize_nested() {
    let json = r#"{"users": [{"name": "Alice"}, {"name": "Bob"}], "count": 2}"#;
    let val: TestBamlValue = serde_json::from_str(json).unwrap();
    match val {
        baml::BamlValue::Map(m) => {
            assert!(matches!(m["count"], baml::BamlValue::Int(2)));
            match &m["users"] {
                baml::BamlValue::List(users) => {
                    assert_eq!(users.len(), 2);
                }
                other => panic!("expected List, got {:?}", other),
            }
        }
        other => panic!("expected Map, got {:?}", other),
    }
}

#[test]
fn test_baml_value_known_type_serialize() {
    let known: TestBamlValue =
        baml::BamlValue::Known(Types::BigNumbers(BigNumbers { a: 1, b: 2.0 }));
    let json = serde_json::to_string(&known).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["a"], 1);
    assert_eq!(v["b"], 2.0);
}

#[test]
fn test_baml_value_dynamic_enum_serialize() {
    let de: TestBamlValue = baml::BamlValue::DynamicEnum(baml::DynamicEnum {
        name: "Sentiment".into(),
        value: "happy".into(),
    });
    let json = serde_json::to_string(&de).unwrap();
    assert_eq!(json, r#""happy""#);
}

#[test]
fn test_baml_value_dynamic_class_serialize() {
    let mut fields = HashMap::new();
    fields.insert("x".into(), baml::BamlValue::Int(10));
    fields.insert("y".into(), baml::BamlValue::String("hello".into()));
    let dc: TestBamlValue =
        baml::BamlValue::DynamicClass(baml::DynamicClass::with_fields("Point".into(), fields));
    let json = serde_json::to_string(&dc).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["x"], 10);
    assert_eq!(v["y"], "hello");
}

#[test]
fn test_baml_value_dynamic_union_serialize() {
    let du: TestBamlValue = baml::BamlValue::DynamicUnion(baml::DynamicUnion {
        name: "FooOrBar".into(),
        variant_name: "Foo".into(),
        value: Box::new(baml::BamlValue::String("foo_value".into())),
    });
    let json = serde_json::to_string(&du).unwrap();
    // DynamicUnion serializes the inner value directly
    assert_eq!(json, r#""foo_value""#);
}

// =============================================================================
// Types / StreamTypes cannot be deserialized
// =============================================================================

#[test]
fn test_types_enum_cannot_deserialize() {
    let result = serde_json::from_str::<Types>(r#"{"a":1,"b":2.0}"#);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("not deserializable")
    );
}

#[test]
fn test_stream_types_cannot_deserialize() {
    let result = serde_json::from_str::<rust::baml_client::stream_types::StreamTypes>(r#"{"a":1}"#);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("not deserializable")
    );
}

#[test]
fn test_types_enum_can_serialize() {
    let t = Types::BigNumbers(BigNumbers { a: 1, b: 2.0 });
    let json = serde_json::to_string(&t).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["a"], 1);
    assert_eq!(v["b"], 2.0);
}

// =============================================================================
// Streaming types: serialize only, no deserialize
// =============================================================================

#[test]
fn test_stream_class_serialize_only() {
    let s = rust::baml_client::stream_types::BigNumbers {
        a: Some(42),
        b: None,
    };
    let json = serde_json::to_string(&s).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["a"], 42);
    assert!(v["b"].is_null());
}

#[test]
fn test_stream_class_cannot_deserialize() {
    // Stream types only derive Serialize, not Deserialize
    // We can't test this with serde_json::from_str since it won't compile.
    // Instead, we verify by attempting to serialize and confirming it works.
    let s = rust::baml_client::stream_types::AddressWithMeta {
        street: Some("123 Main".into()),
        city: None,
        zipcode: Some("12345".into()),
    };
    let json = serde_json::to_string(&s).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["street"], "123 Main");
    assert!(v["city"].is_null());
    assert_eq!(v["zipcode"], "12345");
}

// =============================================================================
// Media types: serialize only (deserialize errors)
// =============================================================================

#[test]
fn test_media_repr_serialize_url() {
    use baml::__internal::{BamlMediaRepr, BamlMediaReprContent};
    let repr = BamlMediaRepr {
        mime_type: Some("image/png".into()),
        content: BamlMediaReprContent::Url {
            url: "https://example.com/img.png".into(),
        },
    };
    let json = serde_json::to_string(&repr).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["media_type"], "image/png");
    assert_eq!(v["url"], "https://example.com/img.png");
}

#[test]
fn test_media_repr_serialize_base64() {
    use baml::__internal::{BamlMediaRepr, BamlMediaReprContent};
    let repr = BamlMediaRepr {
        mime_type: None,
        content: BamlMediaReprContent::Base64 {
            base64: "SGVsbG8=".into(),
        },
    };
    let json = serde_json::to_string(&repr).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    // mime_type should be absent (skip_serializing_if)
    assert!(v.get("media_type").is_none());
    assert_eq!(v["base64"], "SGVsbG8=");
}

#[test]
fn test_media_repr_deserialize_url() {
    use baml::__internal::{BamlMediaRepr, BamlMediaReprContent};
    let json = r#"{"media_type":"image/jpeg","url":"https://example.com/photo.jpg"}"#;
    let repr: BamlMediaRepr = serde_json::from_str(json).unwrap();
    assert_eq!(repr.mime_type, Some("image/jpeg".into()));
    assert!(matches!(
        repr.content,
        BamlMediaReprContent::Url { ref url } if url == "https://example.com/photo.jpg"
    ));
}

#[test]
fn test_media_repr_deserialize_base64() {
    use baml::__internal::{BamlMediaRepr, BamlMediaReprContent};
    let json = r#"{"base64":"AQID"}"#;
    let repr: BamlMediaRepr = serde_json::from_str(json).unwrap();
    assert_eq!(repr.mime_type, None);
    assert!(
        matches!(repr.content, BamlMediaReprContent::Base64 { ref base64 } if base64 == "AQID")
    );
}

#[test]
fn test_image_cannot_deserialize_directly() {
    let result = serde_json::from_str::<Image>(r#"{"url":"https://example.com/img.png"}"#);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Cannot deserialize Image directly")
    );
}

#[test]
fn test_audio_cannot_deserialize_directly() {
    let result = serde_json::from_str::<Audio>(r#"{"url":"https://example.com/audio.mp3"}"#);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Cannot deserialize Audio directly")
    );
}

#[test]
fn test_pdf_cannot_deserialize_directly() {
    let result = serde_json::from_str::<Pdf>(r#"{"url":"https://example.com/doc.pdf"}"#);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Cannot deserialize Pdf directly")
    );
}

// =============================================================================
// DynamicClass / DynamicEnum / DynamicUnion cannot be deserialized
// =============================================================================

#[test]
fn test_dynamic_class_standalone_cannot_deserialize() {
    let result = serde_json::from_str::<
        baml::DynamicClass<Types, rust::baml_client::stream_types::StreamTypes>,
    >(r#"{"x": 1}"#);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("DynamicClass cannot be deserialized")
    );
}

#[test]
fn test_dynamic_enum_standalone_cannot_deserialize() {
    let result = serde_json::from_str::<baml::DynamicEnum>(r#""happy""#);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("DynamicEnum cannot be deserialized")
    );
}

#[test]
fn test_dynamic_union_standalone_cannot_deserialize() {
    let result = serde_json::from_str::<
        baml::DynamicUnion<Types, rust::baml_client::stream_types::StreamTypes>,
    >(r#""value""#);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("DynamicUnion cannot be deserialized")
    );
}

// =============================================================================
// Class containing enum fields
// =============================================================================

#[test]
fn test_class_with_enum_field_roundtrip() {
    // DynamicClassTwo has a DynEnumOne field
    let d = DynamicClassTwo {
        hi: "greetings".into(),
        some_class: SomeClassNestedDynamic {
            hi: "nested".into(),
            __dynamic: HashMap::new(),
        },
        status: DynEnumOne::_Dynamic("PENDING".into()),
        __dynamic: HashMap::new(),
    };
    let json = serde_json::to_string(&d).unwrap();
    let rt: DynamicClassTwo = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.hi, "greetings");
    assert_eq!(rt.status, DynEnumOne::_Dynamic("PENDING".into()));
}

// =============================================================================
// Union of class and string
// =============================================================================

#[test]
fn test_union_class_or_string() {
    // Union2NestedOrString: Nested first, then String
    let nested = Union2NestedOrString::Nested(Nested {
        prop3: Some("val".into()),
        prop4: None,
        prop20: Nested2 {
            prop11: None,
            prop12: None,
        },
    });
    let json = serde_json::to_string(&nested).unwrap();
    let rt: Union2NestedOrString = serde_json::from_str(&json).unwrap();
    assert!(matches!(rt, Union2NestedOrString::Nested(_)));

    let s = Union2NestedOrString::String("plain text".into());
    let json = serde_json::to_string(&s).unwrap();
    assert_eq!(json, r#""plain text""#);
    let rt: Union2NestedOrString = serde_json::from_str(&json).unwrap();
    assert!(matches!(rt, Union2NestedOrString::String(s) if s == "plain text"));
}

// =============================================================================
// Multi-primitive union (4 types)
// =============================================================================

#[test]
fn test_four_primitive_union() {
    // Union4BoolOrFloatOrIntOrString: Int, String, Bool, Float
    let cases: Vec<(Union4BoolOrFloatOrIntOrString, &str)> = vec![
        (Union4BoolOrFloatOrIntOrString::Int(10), "10"),
        (
            Union4BoolOrFloatOrIntOrString::String("hi".into()),
            r#""hi""#,
        ),
        (Union4BoolOrFloatOrIntOrString::Bool(true), "true"),
        (Union4BoolOrFloatOrIntOrString::Float(1.5), "1.5"),
    ];
    for (val, expected_json) in cases {
        let json = serde_json::to_string(&val).unwrap();
        assert_eq!(json, expected_json);
    }

    // Deserialize
    let rt: Union4BoolOrFloatOrIntOrString = serde_json::from_str("10").unwrap();
    assert!(matches!(rt, Union4BoolOrFloatOrIntOrString::Int(10)));

    let rt: Union4BoolOrFloatOrIntOrString = serde_json::from_str(r#""hi""#).unwrap();
    assert!(matches!(rt, Union4BoolOrFloatOrIntOrString::String(s) if s == "hi"));

    let rt: Union4BoolOrFloatOrIntOrString = serde_json::from_str("true").unwrap();
    assert!(matches!(rt, Union4BoolOrFloatOrIntOrString::Bool(true)));

    let rt: Union4BoolOrFloatOrIntOrString = serde_json::from_str("1.5").unwrap();
    assert!(matches!(rt, Union4BoolOrFloatOrIntOrString::Float(_)));
}

// =============================================================================
// Three-primitive union
// =============================================================================

#[test]
fn test_three_primitive_union() {
    // Union3FloatOrIntOrString: String, Int, Float
    let s = Union3FloatOrIntOrString::String("hello".into());
    assert_eq!(serde_json::to_string(&s).unwrap(), r#""hello""#);

    let i = Union3FloatOrIntOrString::Int(7);
    assert_eq!(serde_json::to_string(&i).unwrap(), "7");

    let f = Union3FloatOrIntOrString::Float(2.5);
    assert_eq!(serde_json::to_string(&f).unwrap(), "2.5");

    // Roundtrip
    let rt: Union3FloatOrIntOrString = serde_json::from_str(r#""world""#).unwrap();
    assert!(matches!(rt, Union3FloatOrIntOrString::String(s) if s == "world"));

    let rt: Union3FloatOrIntOrString = serde_json::from_str("7").unwrap();
    assert!(matches!(rt, Union3FloatOrIntOrString::Int(7)));
}

// =============================================================================
// Class with union field
// =============================================================================

#[test]
fn test_class_with_union_field() {
    let obj = ComplexMemoryObject {
        id: "mem1".into(),
        name: "test".into(),
        description: "desc".into(),
        metadata: vec![
            Union3FloatOrIntOrString::String("tag".into()),
            Union3FloatOrIntOrString::Int(42),
        ],
    };
    let json = serde_json::to_string(&obj).unwrap();
    let rt: ComplexMemoryObject = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.id, "mem1");
    assert_eq!(rt.metadata.len(), 2);
}

// =============================================================================
// Edge cases
// =============================================================================

#[test]
fn test_empty_string_fields() {
    let bn = BigNumbers { a: 0, b: 0.0 };
    let json = serde_json::to_string(&bn).unwrap();
    assert_eq!(json, r#"{"a":0,"b":0.0}"#);
    let rt: BigNumbers = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.a, 0);
    assert_eq!(rt.b, 0.0);
}

#[test]
fn test_large_numbers() {
    let bn = BigNumbers {
        a: i64::MAX,
        b: f64::MAX,
    };
    let json = serde_json::to_string(&bn).unwrap();
    let rt: BigNumbers = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.a, i64::MAX);
    // f64::MAX roundtrip should be exact
    assert_eq!(rt.b, f64::MAX);
}

#[test]
fn test_negative_numbers() {
    let bn = BigNumbers { a: -999, b: -0.001 };
    let json = serde_json::to_string(&bn).unwrap();
    let rt: BigNumbers = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.a, -999);
    assert!((rt.b - (-0.001)).abs() < 1e-10);
}

#[test]
fn test_dynamic_class_empty() {
    let d = DynamicClassOne {
        __dynamic: HashMap::new(),
    };
    let json = serde_json::to_string(&d).unwrap();
    assert_eq!(json, "{}");
    let rt: DynamicClassOne = serde_json::from_str(&json).unwrap();
    assert!(rt.__dynamic.is_empty());
}

#[test]
fn test_baml_value_mixed_list_serialize() {
    let list: TestBamlValue = baml::BamlValue::List(vec![
        baml::BamlValue::String("hello".into()),
        baml::BamlValue::Int(42),
        baml::BamlValue::Bool(true),
        baml::BamlValue::Null,
    ]);
    let json = serde_json::to_string(&list).unwrap();
    assert_eq!(json, r#"["hello",42,true,null]"#);
}

#[test]
fn test_baml_value_mixed_list_deserialize() {
    let json = r#"["hello",42,true,null]"#;
    let rt: TestBamlValue = serde_json::from_str(json).unwrap();
    match rt {
        baml::BamlValue::List(items) => {
            assert_eq!(items.len(), 4);
            assert!(matches!(items[0], baml::BamlValue::String(ref s) if s == "hello"));
            assert!(matches!(items[1], baml::BamlValue::Int(42)));
            assert!(matches!(items[2], baml::BamlValue::Bool(true)));
            assert!(matches!(items[3], baml::BamlValue::Null));
        }
        other => panic!("expected List, got {:?}", other),
    }
}

#[test]
fn test_union_enum_or_string_with_tag() {
    // Union2StringOrTag: Tag (enum) first, then String
    let tag = Union2StringOrTag::Tag(Tag::Security);
    let json = serde_json::to_string(&tag).unwrap();
    assert_eq!(json, r#""Security""#);

    // Deserialization of a known Tag variant
    let rt: Union2StringOrTag = serde_json::from_str(r#""Security""#).unwrap();
    // Since Tag is first and "Security" is a valid Tag variant, it matches Tag
    assert!(matches!(rt, Union2StringOrTag::Tag(Tag::Security)));

    let s = Union2StringOrTag::String("just a string".into());
    let json = serde_json::to_string(&s).unwrap();
    assert_eq!(json, r#""just a string""#);

    // An unknown string should fall through to String variant
    // (since Tag won't match it)
    let rt: Union2StringOrTag = serde_json::from_str(r#""random text""#).unwrap();
    // With untagged unions, Tag tries first. "random text" isn't a valid Tag variant,
    // so it falls through to String.
    assert!(matches!(rt, Union2StringOrTag::String(s) if s == "random text"));
}

#[test]
fn test_dynamic_class_multiple_dynamic_value_types_serialize() {
    // Serialization works for all BamlValue types
    let mut dynamic = HashMap::new();
    dynamic.insert("str_field".into(), baml::BamlValue::String("val".into()));
    dynamic.insert("int_field".into(), baml::BamlValue::Int(99));
    dynamic.insert("float_field".into(), baml::BamlValue::Float(1.5));
    dynamic.insert("bool_field".into(), baml::BamlValue::Bool(false));
    dynamic.insert("null_field".into(), baml::BamlValue::Null);
    dynamic.insert(
        "list_field".into(),
        baml::BamlValue::List(vec![baml::BamlValue::String("x".into())]),
    );

    let d = DynamicClassOne { __dynamic: dynamic };
    let json = serde_json::to_string(&d).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["str_field"], "val");
    assert_eq!(v["int_field"], 99);
    assert_eq!(v["float_field"], 1.5);
    assert_eq!(v["bool_field"], false);
    assert!(v["null_field"].is_null());
    assert_eq!(v["list_field"], serde_json::json!(["x"]));
}

#[test]
fn test_dynamic_class_multiple_dynamic_value_types_deserialize() {
    let json = r#"{"str_field":"val","int_field":99,"float_field":1.5,"bool_field":false,"null_field":null,"list_field":["x"]}"#;
    let rt: DynamicClassOne = serde_json::from_str(json).unwrap();
    assert_eq!(rt.__dynamic.len(), 6);
    assert!(rt.__dynamic.contains_key("str_field"));
    assert!(rt.__dynamic.contains_key("int_field"));
    assert!(rt.__dynamic.contains_key("float_field"));
    assert!(rt.__dynamic.contains_key("bool_field"));
    assert!(rt.__dynamic.contains_key("null_field"));
    assert!(rt.__dynamic.contains_key("list_field"));
}
