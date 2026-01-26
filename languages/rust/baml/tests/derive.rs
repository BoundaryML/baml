//! Tests for `BamlEncode` and `BamlDecode` derive macros.

mod common;

use baml::{
    BamlClass, BamlDecode, BamlEncode, BamlEnum,
    __internal::{host_map_entry, host_value},
};

// =============================================================================
// BamlEncode derive tests
// =============================================================================

mod encode {
    use super::*;

    mod structs {
        use super::*;

        #[derive(BamlEncode)]
        struct SimpleStruct {
            name: String,
            age: i64,
        }

        #[derive(BamlEncode)]
        #[baml(name = "PersonInfo")]
        struct RenamedStruct {
            #[baml(name = "full_name")]
            name: String,
            #[baml(name = "years_old")]
            age: i64,
        }

        #[test]
        fn simple_struct_uses_rust_name() {
            let s = SimpleStruct {
                name: "Alice".to_string(),
                age: 30,
            };
            let encoded = s.baml_encode();

            if let Some(host_value::Value::ClassValue(class)) = encoded.value {
                assert_eq!(class.name, "SimpleStruct");
                assert_eq!(class.fields.len(), 2);
            } else {
                panic!("expected class value");
            }
        }

        #[test]
        fn renamed_struct_uses_baml_name() {
            let p = RenamedStruct {
                name: "Bob".to_string(),
                age: 25,
            };
            let encoded = p.baml_encode();

            if let Some(host_value::Value::ClassValue(class)) = encoded.value {
                assert_eq!(class.name, "PersonInfo");

                let field_names: Vec<_> = class
                    .fields
                    .iter()
                    .filter_map(|f| {
                        if let Some(host_map_entry::Key::StringKey(k)) = &f.key {
                            Some(k.clone())
                        } else {
                            None
                        }
                    })
                    .collect();

                assert!(field_names.contains(&"full_name".to_string()));
                assert!(field_names.contains(&"years_old".to_string()));
                assert!(!field_names.contains(&"name".to_string()));
                assert!(!field_names.contains(&"age".to_string()));
            } else {
                panic!("expected class value");
            }
        }
    }

    mod enums {
        use super::*;

        #[derive(BamlEncode, Eq, PartialEq, Hash)]
        enum SimpleEnum {
            Red,
            Green,
            Blue,
        }

        #[derive(BamlEncode, Eq, PartialEq, Hash)]
        #[baml(name = "ColorChoice")]
        enum RenamedEnum {
            #[baml(name = "RED")]
            Red,
            #[baml(name = "GREEN")]
            Green,
            #[baml(name = "BLUE")]
            Blue,
        }

        impl std::str::FromStr for SimpleEnum {
            type Err = ();

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    "Red" => Ok(SimpleEnum::Red),
                    "Green" => Ok(SimpleEnum::Green),
                    "Blue" => Ok(SimpleEnum::Blue),
                    _ => Err(()),
                }
            }
        }

        impl std::str::FromStr for RenamedEnum {
            type Err = ();

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    "RED" => Ok(RenamedEnum::Red),
                    "GREEN" => Ok(RenamedEnum::Green),
                    "BLUE" => Ok(RenamedEnum::Blue),
                    _ => Err(()),
                }
            }
        }

        #[test]
        fn simple_enum_uses_rust_names() {
            let c = SimpleEnum::Green;
            let encoded = c.baml_encode();

            if let Some(host_value::Value::EnumValue(e)) = encoded.value {
                assert_eq!(e.name, "SimpleEnum");
                assert_eq!(e.value, "Green");
            } else {
                panic!("expected enum value");
            }
        }

        #[test]
        fn renamed_enum_uses_baml_names() {
            let c = RenamedEnum::Red;
            let encoded = c.baml_encode();

            if let Some(host_value::Value::EnumValue(e)) = encoded.value {
                assert_eq!(e.name, "ColorChoice");
                assert_eq!(e.value, "RED");
            } else {
                panic!("expected enum value");
            }
        }
    }

    mod unions {
        use super::*;

        /// A simple union of primitive types
        #[derive(Debug, Clone, PartialEq, BamlEncode)]
        #[baml(union)]
        enum PrimitiveUnion {
            String(String),
            Int(i64),
            Bool(bool),
        }

        /// A union containing a nested struct
        #[derive(Debug, Clone, PartialEq, BamlEncode)]
        struct Person {
            name: String,
            age: i64,
        }

        #[derive(Debug, Clone, PartialEq, BamlEncode)]
        #[baml(union)]
        enum PersonOrString {
            Person(Person),
            String(String),
        }

        #[test]
        fn union_string_variant_encodes_inner_value() {
            let u = PrimitiveUnion::String("hello".to_string());
            let encoded = u.baml_encode();

            // Union encode should just encode the inner value directly
            if let Some(host_value::Value::StringValue(s)) = encoded.value {
                assert_eq!(s, "hello");
            } else {
                panic!("expected string value, got {:?}", encoded.value);
            }
        }

        #[test]
        fn union_int_variant_encodes_inner_value() {
            let u = PrimitiveUnion::Int(42);
            let encoded = u.baml_encode();

            if let Some(host_value::Value::IntValue(i)) = encoded.value {
                assert_eq!(i, 42);
            } else {
                panic!("expected int value, got {:?}", encoded.value);
            }
        }

        #[test]
        fn union_bool_variant_encodes_inner_value() {
            let u = PrimitiveUnion::Bool(true);
            let encoded = u.baml_encode();

            if let Some(host_value::Value::BoolValue(b)) = encoded.value {
                assert!(b);
            } else {
                panic!("expected bool value, got {:?}", encoded.value);
            }
        }

        #[test]
        fn union_with_class_variant_encodes_class() {
            let u = PersonOrString::Person(Person {
                name: "Alice".to_string(),
                age: 30,
            });
            let encoded = u.baml_encode();

            if let Some(host_value::Value::ClassValue(class)) = encoded.value {
                assert_eq!(class.name, "Person");
                assert_eq!(class.fields.len(), 2);
            } else {
                panic!("expected class value, got {:?}", encoded.value);
            }
        }

        #[test]
        fn union_with_string_variant_in_mixed_union() {
            let u = PersonOrString::String("just a string".to_string());
            let encoded = u.baml_encode();

            if let Some(host_value::Value::StringValue(s)) = encoded.value {
                assert_eq!(s, "just a string");
            } else {
                panic!("expected string value, got {:?}", encoded.value);
            }
        }
    }
}

// =============================================================================
// BamlDecode derive tests
// =============================================================================

mod decode {
    use super::*;

    mod structs {
        use super::*;

        #[derive(BamlDecode, Debug, PartialEq)]
        #[baml(name = "TestPerson")]
        struct DecodableStruct {
            name: String,
            age: i64,
            email: Option<String>,
        }

        #[test]
        fn implements_baml_class_trait() {
            assert_eq!(DecodableStruct::TYPE_NAME, "TestPerson");
        }
    }

    mod enums {
        use super::*;

        #[derive(BamlDecode, Debug, PartialEq)]
        #[baml(name = "TestColor")]
        enum DecodableEnum {
            Red,
            Green,
            Blue,
        }

        #[test]
        fn implements_baml_enum_trait() {
            assert_eq!(DecodableEnum::ENUM_NAME, "TestColor");
        }

        #[test]
        fn decodes_valid_variants() {
            let red = DecodableEnum::from_variant_name("Red").unwrap();
            assert_eq!(red, DecodableEnum::Red);

            let green = DecodableEnum::from_variant_name("Green").unwrap();
            assert_eq!(green, DecodableEnum::Green);

            let blue = DecodableEnum::from_variant_name("Blue").unwrap();
            assert_eq!(blue, DecodableEnum::Blue);
        }

        #[test]
        fn returns_error_for_unknown_variant() {
            let result = DecodableEnum::from_variant_name("Unknown");
            assert!(result.is_err());
        }
    }

    mod unions {
        use super::*;
        use crate::common::{
            make_bool_holder, make_class_holder, make_int_holder, make_string_holder,
            make_union_holder,
        };

        /// A simple union of primitive types
        #[derive(Debug, Clone, PartialEq, BamlDecode)]
        #[baml(union)]
        enum PrimitiveUnion {
            #[baml(name = "string")]
            String(String),
            #[baml(name = "int")]
            Int(i64),
            #[baml(name = "bool")]
            Bool(bool),
        }

        /// A struct to use in union tests
        #[derive(Debug, Clone, PartialEq, BamlDecode)]
        struct Person {
            name: String,
            age: i64,
        }

        #[derive(Debug, Clone, PartialEq, BamlDecode)]
        #[baml(union)]
        enum PersonOrString {
            #[baml(name = "Person")]
            Person(Person),
            #[baml(name = "string")]
            String(String),
        }

        #[test]
        fn decodes_string_variant_from_union() {
            let holder = make_union_holder("PrimitiveUnion", "string", make_string_holder("hello"));

            let result = PrimitiveUnion::baml_decode(&holder).unwrap();
            assert_eq!(result, PrimitiveUnion::String("hello".to_string()));
        }

        #[test]
        fn decodes_int_variant_from_union() {
            let holder = make_union_holder("PrimitiveUnion", "int", make_int_holder(42));

            let result = PrimitiveUnion::baml_decode(&holder).unwrap();
            assert_eq!(result, PrimitiveUnion::Int(42));
        }

        #[test]
        fn decodes_bool_variant_from_union() {
            let holder = make_union_holder("PrimitiveUnion", "bool", make_bool_holder(true));

            let result = PrimitiveUnion::baml_decode(&holder).unwrap();
            assert_eq!(result, PrimitiveUnion::Bool(true));
        }

        #[test]
        fn decodes_class_variant_from_union() {
            let holder = make_union_holder(
                "PersonOrString",
                "Person",
                make_class_holder(
                    "Person",
                    vec![
                        ("name", make_string_holder("Alice")),
                        ("age", make_int_holder(30)),
                    ],
                ),
            );

            let result = PersonOrString::baml_decode(&holder).unwrap();
            assert_eq!(
                result,
                PersonOrString::Person(Person {
                    name: "Alice".to_string(),
                    age: 30,
                })
            );
        }

        #[test]
        fn decodes_string_variant_in_mixed_union() {
            let holder = make_union_holder(
                "PersonOrString",
                "string",
                make_string_holder("just a string"),
            );

            let result = PersonOrString::baml_decode(&holder).unwrap();
            assert_eq!(result, PersonOrString::String("just a string".to_string()));
        }

        #[test]
        fn returns_error_for_non_union_holder() {
            // Try to decode a raw string (not wrapped in UnionVariantValue)
            let holder = make_string_holder("hello");

            let result = PrimitiveUnion::baml_decode(&holder);
            assert!(result.is_err());
        }
    }
}

// =============================================================================
// Combined encode + decode tests (both traits on same type)
// =============================================================================

mod combined {
    use super::*;
    use crate::common::{
        make_class_holder, make_int_holder, make_string_holder, make_union_holder,
    };

    // Note: Encode and decode are NOT symmetric for unions:
    // - Encode: Union variant -> raw inner value (e.g., StringValue)
    // - Decode: UnionVariantValue wrapper -> Union variant
    // The BAML runtime handles the wrapping/unwrapping appropriately.

    /// A union type that implements both encode and decode
    #[derive(Debug, Clone, PartialEq, BamlEncode, BamlDecode)]
    #[baml(union)]
    enum StringOrInt {
        #[baml(name = "string")]
        String(String),
        #[baml(name = "int")]
        Int(i64),
    }

    /// A struct that can be used in unions
    #[derive(Debug, Clone, PartialEq, BamlEncode, BamlDecode)]
    struct User {
        name: String,
        age: i64,
    }

    #[derive(Debug, Clone, PartialEq, BamlEncode, BamlDecode)]
    #[baml(union)]
    enum UserOrString {
        #[baml(name = "User")]
        User(User),
        #[baml(name = "string")]
        String(String),
    }

    /// A recursive union type using Box (for recursive types)
    #[derive(Debug, Clone, PartialEq, BamlEncode, BamlDecode)]
    #[baml(union)]
    enum RecursiveUnion {
        #[baml(name = "string")]
        Leaf(String),
        #[baml(name = "RecursiveUnion")]
        Node(Box<RecursiveUnion>),
    }

    #[test]
    fn both_traits_can_be_derived_on_primitive_union() {
        // Verify encode works
        let original = StringOrInt::String("test".to_string());
        let encoded = original.baml_encode();
        assert!(matches!(
            encoded.value,
            Some(host_value::Value::StringValue(_))
        ));

        // Verify decode works (with runtime-style UnionVariantValue wrapper)
        let holder = make_union_holder("StringOrInt", "string", make_string_holder("test"));
        let decoded = StringOrInt::baml_decode(&holder).unwrap();
        assert_eq!(decoded, StringOrInt::String("test".to_string()));
    }

    #[test]
    fn both_traits_can_be_derived_on_class_union() {
        // Verify encode works
        let original = UserOrString::User(User {
            name: "Bob".to_string(),
            age: 25,
        });
        let encoded = original.baml_encode();
        assert!(matches!(
            encoded.value,
            Some(host_value::Value::ClassValue(_))
        ));

        // Verify decode works
        let holder = make_union_holder(
            "UserOrString",
            "User",
            make_class_holder(
                "User",
                vec![
                    ("name", make_string_holder("Bob")),
                    ("age", make_int_holder(25)),
                ],
            ),
        );
        let decoded = UserOrString::baml_decode(&holder).unwrap();
        assert_eq!(
            decoded,
            UserOrString::User(User {
                name: "Bob".to_string(),
                age: 25,
            })
        );
    }

    #[test]
    fn box_types_work_in_recursive_unions() {
        // Test encoding a recursive structure
        let nested = RecursiveUnion::Node(Box::new(RecursiveUnion::Leaf("inner".to_string())));
        let encoded = nested.baml_encode();

        // The outer Node variant encodes the Box<RecursiveUnion>, which encodes the
        // inner Leaf Since it's a union, the inner value is encoded directly
        // (string value)
        assert!(matches!(
            encoded.value,
            Some(host_value::Value::StringValue(_))
        ));

        // Test decoding a nested structure
        let inner_holder =
            make_union_holder("RecursiveUnion", "string", make_string_holder("inner"));
        let outer_holder = make_union_holder("RecursiveUnion", "RecursiveUnion", inner_holder);

        let decoded = RecursiveUnion::baml_decode(&outer_holder).unwrap();
        assert_eq!(
            decoded,
            RecursiveUnion::Node(Box::new(RecursiveUnion::Leaf("inner".to_string())))
        );
    }
}
