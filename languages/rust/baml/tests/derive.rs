//! Tests for BamlEncode and BamlDecode derive macros.

use baml::__internal::{host_map_entry, host_value};
use baml::{BamlClass, BamlDecode, BamlEncode, BamlEnum};

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

        #[derive(BamlEncode)]
        enum SimpleEnum {
            Red,
            Green,
            Blue,
        }

        #[derive(BamlEncode)]
        #[baml(name = "ColorChoice")]
        enum RenamedEnum {
            #[baml(name = "RED")]
            Red,
            #[baml(name = "GREEN")]
            Green,
            #[baml(name = "BLUE")]
            Blue,
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
}
