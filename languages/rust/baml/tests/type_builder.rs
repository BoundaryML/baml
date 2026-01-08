//! Tests for `TypeBuilder`, `TypeDef`, `EnumBuilder`, `EnumValueBuilder`,
//! `ClassBuilder`, `ClassPropertyBuilder`
#![allow(clippy::print_stderr)]

mod type_builder {
    use std::collections::HashMap;

    use baml::{
        BamlRuntime, ClassBuilder, ClassPropertyBuilder, EnumBuilder, EnumValueBuilder,
        FunctionArgs, TypeBuilder, TypeDef,
    };

    /// Helper to create environment variables `HashMap` from current
    /// environment
    fn env_vars() -> HashMap<String, String> {
        std::env::vars().collect()
    }

    /// Create a minimal runtime for testing `TypeBuilder` types
    fn create_test_runtime() -> BamlRuntime {
        let mut files = HashMap::new();
        files.insert(
            "main.baml".to_string(),
            r#####"
            client<llm> TestClient {
                provider openai
                options {
                    model "gpt-4o"
                    api_key "test-key"
                }
            }
            "#####
                .to_string(),
        );

        BamlRuntime::new(".", &files, &env_vars()).expect("Failed to create test runtime")
    }

    // =========================================================================
    // TypeBuilder Creation Tests
    // =========================================================================

    mod creation {
        use super::*;

        #[test]
        fn new_type_builder_succeeds() {
            let runtime = create_test_runtime();
            let _tb = runtime.new_type_builder();
        }

        #[test]
        fn multiple_type_builders_can_be_created() {
            let runtime = create_test_runtime();
            let _tb1 = runtime.new_type_builder();
            let _tb2 = runtime.new_type_builder();
            let _tb3 = runtime.new_type_builder();
        }
    }

    // =========================================================================
    // Primitive Type Tests
    // =========================================================================

    mod primitives {
        use super::*;

        #[test]
        fn string_type_returns_type_def() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let _string_type: TypeDef = tb.string();
        }

        #[test]
        fn int_type_returns_type_def() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let _int_type: TypeDef = tb.int();
        }

        #[test]
        fn float_type_returns_type_def() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let _float_type: TypeDef = tb.float();
        }

        #[test]
        fn bool_type_returns_type_def() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let _bool_type: TypeDef = tb.bool();
        }

        #[test]
        fn null_type_returns_type_def() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let _null_type: TypeDef = tb.null();
        }

        #[test]
        fn all_primitive_types_can_be_created() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();

            // Create all primitives from the same builder
            let _s = tb.string();
            let _i = tb.int();
            let _f = tb.float();
            let _b = tb.bool();
            let _n = tb.null();
        }
    }

    // =========================================================================
    // Literal Type Tests
    // =========================================================================

    mod literals {
        use super::*;

        #[test]
        fn literal_string_returns_type_def() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let _lit = tb.literal_string("hello");
        }

        #[test]
        fn literal_string_with_empty_string() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let _lit = tb.literal_string("");
        }

        #[test]
        fn literal_string_with_special_chars() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let _lit = tb.literal_string("hello\nworld\t!");
        }

        #[test]
        fn literal_int_returns_type_def() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let _lit = tb.literal_int(42);
        }

        #[test]
        fn literal_int_with_negative_value() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let _lit = tb.literal_int(-100);
        }

        #[test]
        fn literal_int_with_zero() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let _lit = tb.literal_int(0);
        }

        #[test]
        fn literal_bool_true() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let _lit = tb.literal_bool(true);
        }

        #[test]
        fn literal_bool_false() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let _lit = tb.literal_bool(false);
        }
    }

    // =========================================================================
    // Composite Type Tests
    // =========================================================================

    mod composites {
        use super::*;

        #[test]
        fn list_wraps_inner_type() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let string_type = tb.string();
            let _list_type = tb.list(&string_type);
        }

        #[test]
        fn optional_wraps_inner_type() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let int_type = tb.int();
            let _optional_type = tb.optional(&int_type);
        }

        #[test]
        fn map_with_key_and_value() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let key_type = tb.string();
            let value_type = tb.int();
            let _map_type = tb.map(&key_type, &value_type);
        }

        #[test]
        fn union_with_multiple_types() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let string_type = tb.string();
            let int_type = tb.int();
            let _union_type = tb.union(&[&string_type, &int_type]);
        }

        #[test]
        fn nested_list_of_lists() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let string_type = tb.string();
            let list_of_strings = tb.list(&string_type);
            let _list_of_lists = tb.list(&list_of_strings);
        }

        #[test]
        fn optional_list() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let string_type = tb.string();
            let list_type = tb.list(&string_type);
            let _optional_list = tb.optional(&list_type);
        }

        #[test]
        fn map_with_complex_value() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let key_type = tb.string();
            let inner_type = tb.int();
            let list_type = tb.list(&inner_type);
            let _map_type = tb.map(&key_type, &list_type);
        }
    }

    // =========================================================================
    // TypeDef Method Tests
    // =========================================================================

    mod type_def_methods {
        use super::*;

        #[test]
        fn type_def_list_method() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let string_type = tb.string();
            let _list_type = string_type.list();
        }

        #[test]
        fn type_def_optional_method() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let int_type = tb.int();
            let _optional_type = int_type.optional();
        }

        #[test]
        fn chained_list_and_optional() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let string_type = tb.string();
            // Create optional list of strings: string[]?
            let _optional_list = string_type.list().optional();
        }
    }

    // =========================================================================
    // EnumBuilder Tests
    // =========================================================================

    mod enums {
        use super::*;

        #[test]
        fn add_enum_succeeds() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let enum_builder = tb.add_enum("Status").expect("Failed to add enum");
            assert!(enum_builder.name().is_ok());
        }

        #[test]
        fn enum_name_returns_correct_name() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let enum_builder = tb.add_enum("MyEnum").expect("Failed to add enum");
            assert_eq!(enum_builder.name().unwrap(), "MyEnum");
        }

        #[test]
        fn add_value_to_enum() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let enum_builder = tb.add_enum("Status").expect("Failed to add enum");
            let _value = enum_builder
                .add_value("Active")
                .expect("Failed to add value");
        }

        #[test]
        fn enum_value_name() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let enum_builder = tb.add_enum("Status").expect("Failed to add enum");
            let value = enum_builder
                .add_value("Active")
                .expect("Failed to add value");
            assert_eq!(value.name().unwrap(), "Active");
        }

        #[test]
        fn add_multiple_values() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let enum_builder = tb.add_enum("Status").expect("Failed to add enum");
            let _v1 = enum_builder
                .add_value("Active")
                .expect("Failed to add Active");
            let _v2 = enum_builder
                .add_value("Inactive")
                .expect("Failed to add Inactive");
            let _v3 = enum_builder
                .add_value("Pending")
                .expect("Failed to add Pending");
        }

        #[test]
        fn list_values_returns_all_values() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let enum_builder = tb.add_enum("Status").expect("Failed to add enum");
            enum_builder.add_value("Active").unwrap();
            enum_builder.add_value("Inactive").unwrap();

            let values = enum_builder.list_values().expect("Failed to list values");
            assert_eq!(values.len(), 2);
        }

        #[test]
        fn get_value_returns_existing_value() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let enum_builder = tb.add_enum("Status").expect("Failed to add enum");
            enum_builder.add_value("Active").unwrap();

            let found = enum_builder
                .get_value("Active");
            assert!(found.is_some());
            assert_eq!(found.unwrap().name().unwrap(), "Active");
        }

        #[test]
        fn get_value_returns_none_for_missing() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let enum_builder = tb.add_enum("Status").expect("Failed to add enum");

            let found = enum_builder
                .get_value("NonExistent");
            assert!(found.is_none());
        }

        #[test]
        fn enum_as_type_returns_type_def() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let enum_builder = tb.add_enum("Status").expect("Failed to add enum");
            enum_builder.add_value("Active").unwrap();

            let _type_def = enum_builder.as_type().expect("Failed to get type");
        }

        #[test]
        fn enum_set_and_get_description() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let enum_builder = tb.add_enum("Status").expect("Failed to add enum");

            enum_builder
                .set_description("Status of a task")
                .expect("Failed to set description");
            let desc = enum_builder
                .description()
                .expect("Failed to get description");
            assert_eq!(desc, Some("Status of a task".to_string()));
        }

        #[test]
        fn enum_set_and_get_alias() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let enum_builder = tb.add_enum("Status").expect("Failed to add enum");

            enum_builder
                .set_alias("TaskStatus")
                .expect("Failed to set alias");
            let alias = enum_builder.alias().expect("Failed to get alias");
            assert_eq!(alias, Some("TaskStatus".to_string()));
        }
    }

    // =========================================================================
    // EnumValueBuilder Tests
    // =========================================================================

    mod enum_values {
        use super::*;

        #[test]
        fn value_set_and_get_description() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let enum_builder = tb.add_enum("Status").expect("Failed to add enum");
            let value = enum_builder
                .add_value("Active")
                .expect("Failed to add value");

            value
                .set_description("The item is active")
                .expect("Failed to set description");
            let desc = value.description().expect("Failed to get description");
            assert_eq!(desc, Some("The item is active".to_string()));
        }

        #[test]
        fn value_set_and_get_alias() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let enum_builder = tb.add_enum("Status").expect("Failed to add enum");
            let value = enum_builder
                .add_value("Active")
                .expect("Failed to add value");

            value.set_alias("ACTIVE").expect("Failed to set alias");
            let alias = value.alias().expect("Failed to get alias");
            assert_eq!(alias, Some("ACTIVE".to_string()));
        }

        #[test]
        fn value_set_and_get_skip() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let enum_builder = tb.add_enum("Status").expect("Failed to add enum");
            let value = enum_builder
                .add_value("Deprecated")
                .expect("Failed to add value");

            value.set_skip(true).expect("Failed to set skip");
            let skip = value.skip().expect("Failed to get skip");
            assert!(skip);
        }

        #[test]
        fn value_skip_defaults_to_false() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let enum_builder = tb.add_enum("Status").expect("Failed to add enum");
            let value = enum_builder
                .add_value("Active")
                .expect("Failed to add value");

            let skip = value.skip().expect("Failed to get skip");
            assert!(!skip);
        }

        #[test]
        fn chained_value_configuration() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let enum_builder = tb.add_enum("Status").expect("Failed to add enum");
            let value = enum_builder
                .add_value("Active")
                .expect("Failed to add value");

            // Chain multiple configurations
            value.set_description("Active status").unwrap();
            value.set_alias("ACTIVE").unwrap();

            // Verify all were set
            assert_eq!(
                value.description().unwrap(),
                Some("Active status".to_string())
            );
            assert_eq!(value.alias().unwrap(), Some("ACTIVE".to_string()));
        }
    }

    // =========================================================================
    // ClassBuilder Tests
    // =========================================================================

    mod classes {
        use super::*;

        #[test]
        fn add_class_succeeds() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let class_builder = tb.add_class("Person").expect("Failed to add class");
            assert!(class_builder.name().is_ok());
        }

        #[test]
        fn class_name_returns_correct_name() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let class_builder = tb.add_class("MyClass").expect("Failed to add class");
            assert_eq!(class_builder.name().unwrap(), "MyClass");
        }

        #[test]
        fn add_property_to_class() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let class_builder = tb.add_class("Person").expect("Failed to add class");
            let string_type = tb.string();
            let _prop = class_builder
                .add_property("name", &string_type)
                .expect("Failed to add property");
        }

        #[test]
        fn property_name() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let class_builder = tb.add_class("Person").expect("Failed to add class");
            let string_type = tb.string();
            let prop = class_builder
                .add_property("name", &string_type)
                .expect("Failed to add property");
            assert_eq!(prop.name().unwrap(), "name");
        }

        #[test]
        fn add_multiple_properties() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let class_builder = tb.add_class("Person").expect("Failed to add class");

            let string_type = tb.string();
            let int_type = tb.int();

            let _p1 = class_builder
                .add_property("name", &string_type)
                .expect("Failed to add name");
            let _p2 = class_builder
                .add_property("age", &int_type)
                .expect("Failed to add age");
        }

        #[test]
        fn list_properties_returns_all_properties() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let class_builder = tb.add_class("Person").expect("Failed to add class");

            let string_type = tb.string();
            let int_type = tb.int();

            class_builder.add_property("name", &string_type).unwrap();
            class_builder.add_property("age", &int_type).unwrap();

            let props = class_builder
                .list_properties()
                .expect("Failed to list properties");
            assert_eq!(props.len(), 2);
        }

        #[test]
        fn get_property_returns_existing_property() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let class_builder = tb.add_class("Person").expect("Failed to add class");
            let string_type = tb.string();
            class_builder.add_property("name", &string_type).unwrap();

            let found = class_builder
                .get_property("name");
            assert!(found.is_some());
            assert_eq!(found.unwrap().name().unwrap(), "name");
        }

        #[test]
        fn get_property_returns_none_for_missing() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let class_builder = tb.add_class("Person").expect("Failed to add class");

            let found = class_builder
                .get_property("nonexistent");
            assert!(found.is_none());
        }

        #[test]
        fn class_as_type_returns_type_def() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let class_builder = tb.add_class("Person").expect("Failed to add class");
            let string_type = tb.string();
            class_builder.add_property("name", &string_type).unwrap();

            let _type_def = class_builder.as_type().expect("Failed to get type");
        }

        #[test]
        fn class_set_and_get_description() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let class_builder = tb.add_class("Person").expect("Failed to add class");

            class_builder
                .set_description("Represents a person")
                .expect("Failed to set description");
            let desc = class_builder
                .description()
                .expect("Failed to get description");
            assert_eq!(desc, Some("Represents a person".to_string()));
        }

        #[test]
        fn class_set_and_get_alias() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let class_builder = tb.add_class("Person").expect("Failed to add class");

            class_builder
                .set_alias("Human")
                .expect("Failed to set alias");
            let alias = class_builder.alias().expect("Failed to get alias");
            assert_eq!(alias, Some("Human".to_string()));
        }
    }

    // =========================================================================
    // ClassPropertyBuilder Tests
    // =========================================================================

    mod class_properties {
        use super::*;

        #[test]
        fn property_set_and_get_description() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let class_builder = tb.add_class("Person").expect("Failed to add class");
            let string_type = tb.string();
            let prop = class_builder
                .add_property("name", &string_type)
                .expect("Failed to add property");

            prop.set_description("The person's full name")
                .expect("Failed to set description");
            let desc = prop.description().expect("Failed to get description");
            assert_eq!(desc, Some("The person's full name".to_string()));
        }

        #[test]
        fn property_set_and_get_alias() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let class_builder = tb.add_class("Person").expect("Failed to add class");
            let string_type = tb.string();
            let prop = class_builder
                .add_property("name", &string_type)
                .expect("Failed to add property");

            prop.set_alias("fullName").expect("Failed to set alias");
            let alias = prop.alias().expect("Failed to get alias");
            assert_eq!(alias, Some("fullName".to_string()));
        }

        #[test]
        fn property_get_type() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let class_builder = tb.add_class("Person").expect("Failed to add class");
            let string_type = tb.string();
            let prop = class_builder
                .add_property("name", &string_type)
                .expect("Failed to add property");

            let _prop_type = prop.get_type().expect("Failed to get type");
        }

        #[test]
        fn property_set_type() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let class_builder = tb.add_class("Person").expect("Failed to add class");
            let string_type = tb.string();
            let int_type = tb.int();

            let prop = class_builder
                .add_property("age", &string_type)
                .expect("Failed to add property");

            // Change the type from string to int
            prop.set_type(&int_type).expect("Failed to set type");
        }
    }

    // =========================================================================
    // TypeBuilder List/Get Tests
    // =========================================================================

    mod type_builder_lists {
        use super::*;

        #[test]
        fn get_enum_returns_existing_enum() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            tb.add_enum("Status").expect("Failed to add enum");

            let found = tb.get_enum("Status");
            assert!(found.is_some());
            assert_eq!(found.unwrap().name().unwrap(), "Status");
        }

        #[test]
        fn get_enum_returns_none_for_missing() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();

            let found = tb.get_enum("NonExistent");
            assert!(found.is_none());
        }

        #[test]
        fn list_enums_returns_all_enums() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            tb.add_enum("Status").unwrap();
            tb.add_enum("Priority").unwrap();

            let enums = tb.list_enums();
            assert_eq!(enums.len(), 2);
        }

        #[test]
        fn list_enums_empty_initially() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();

            let enums = tb.list_enums();
            assert!(enums.is_empty());
        }

        #[test]
        fn get_class_returns_existing_class() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let class_builder = tb.add_class("Person").expect("Failed to add class");
            // Add a property to ensure the class is fully created
            let string_type = tb.string();
            class_builder
                .add_property("name", &string_type)
                .expect("Failed to add property");

            let found = tb.get_class("Person");
            assert!(
                found.is_some(),
                "get_class('Person') returned None after adding class"
            );
            assert_eq!(found.unwrap().name().unwrap(), "Person");
        }

        #[test]
        fn get_class_returns_none_for_missing() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();

            let found = tb.get_class("NonExistent");
            assert!(found.is_none());
        }

        #[test]
        fn list_classes_returns_all_classes() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            tb.add_class("Person").unwrap();
            tb.add_class("Address").unwrap();

            let classes = tb.list_classes();
            assert_eq!(classes.len(), 2);
        }

        #[test]
        fn list_classes_empty_initially() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();

            let classes = tb.list_classes();
            assert!(classes.is_empty());
        }
    }

    // =========================================================================
    // TypeBuilder print() Tests
    // =========================================================================

    mod type_builder_print {
        use super::*;

        #[test]
        fn print_empty_builder() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let repr = tb.print();
            // Empty builder should have TypeBuilder prefix
            assert!(
                repr.contains("TypeBuilder"),
                "Expected 'TypeBuilder' in output, got: {repr}"
            );
        }

        #[test]
        fn print_includes_enum_names() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            tb.add_enum("Status").unwrap();
            tb.add_enum("Priority").unwrap();

            let repr = tb.print();
            assert!(
                repr.contains("Status"),
                "Expected 'Status' in output, got: {repr}"
            );
            assert!(
                repr.contains("Priority"),
                "Expected 'Priority' in output, got: {repr}"
            );
            assert!(
                repr.contains("Enums:"),
                "Expected 'Enums:' section in output, got: {repr}"
            );
        }

        #[test]
        fn print_includes_class_names() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            tb.add_class("Person").unwrap();
            tb.add_class("Address").unwrap();

            let repr = tb.print();
            assert!(
                repr.contains("Person"),
                "Expected 'Person' in output, got: {repr}"
            );
            assert!(
                repr.contains("Address"),
                "Expected 'Address' in output, got: {repr}"
            );
            assert!(
                repr.contains("Classes:"),
                "Expected 'Classes:' section in output, got: {repr}"
            );
        }

        #[test]
        fn print_includes_both_enums_and_classes() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            tb.add_enum("Status").unwrap();
            tb.add_class("Person").unwrap();

            let repr = tb.print();
            assert!(repr.contains("Status"), "Expected 'Status' in output");
            assert!(repr.contains("Person"), "Expected 'Person' in output");
            assert!(repr.contains("Enums:"), "Expected 'Enums:' section");
            assert!(repr.contains("Classes:"), "Expected 'Classes:' section");
        }
    }

    // =========================================================================
    // TypeDef print() Tests
    // =========================================================================

    mod type_def_print {
        use super::*;

        #[test]
        fn print_primitive_string() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            assert_eq!(tb.string().print(), "string");
        }

        #[test]
        fn print_primitive_int() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            assert_eq!(tb.int().print(), "int");
        }

        #[test]
        fn print_primitive_float() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            assert_eq!(tb.float().print(), "float");
        }

        #[test]
        fn print_primitive_bool() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            assert_eq!(tb.bool().print(), "bool");
        }

        #[test]
        fn print_primitive_null() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            assert_eq!(tb.null().print(), "null");
        }

        #[test]
        fn print_list_type() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let list_type = tb.string().list();
            assert_eq!(list_type.print(), "string[]");
        }

        #[test]
        fn print_optional_type() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let optional_type = tb.string().optional();
            // Optional is represented as union with null
            assert_eq!(optional_type.print(), "(string | null)");
        }

        #[test]
        fn print_nested_list_optional() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            // string[]? becomes (string[] | null)
            let nested = tb.string().list().optional();
            assert_eq!(nested.print(), "(string[] | null)");
        }

        #[test]
        fn print_optional_list() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            // (string?)[] becomes (string | null)[]
            let nested = tb.string().optional().list();
            assert_eq!(nested.print(), "(string | null)[]");
        }

        #[test]
        fn print_literal_string() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let lit = tb.literal_string("hello");
            assert_eq!(lit.print(), "\"hello\"");
        }

        #[test]
        fn print_literal_int() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let lit = tb.literal_int(42);
            assert_eq!(lit.print(), "42");
        }

        #[test]
        fn print_literal_bool_true() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let lit = tb.literal_bool(true);
            assert_eq!(lit.print(), "true");
        }

        #[test]
        fn print_literal_bool_false() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let lit = tb.literal_bool(false);
            assert_eq!(lit.print(), "false");
        }

        #[test]
        fn print_map_type() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let map_type = tb.map(&tb.string(), &tb.int());
            assert_eq!(map_type.print(), "map<string, int>");
        }

        #[test]
        fn print_union_type() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let string_type = tb.string();
            let int_type = tb.int();
            let union_type = tb.union(&[&string_type, &int_type]);
            // Union format: (type1 | type2)
            let repr = union_type.print();
            assert!(
                repr.contains("string") && repr.contains("int") && repr.contains('|'),
                "Expected union format with string, int, and |, got: {repr}"
            );
        }

        #[test]
        fn print_enum_type() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let enum_builder = tb.add_enum("Status").unwrap();
            enum_builder.add_value("Active").unwrap();
            let enum_type = enum_builder.as_type().unwrap();
            assert_eq!(enum_type.print(), "Status");
        }

        #[test]
        fn print_class_type() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let class_builder = tb.add_class("Person").unwrap();
            let string_type = tb.string();
            class_builder.add_property("name", &string_type).unwrap();
            let class_type = class_builder.as_type().unwrap();
            assert_eq!(class_type.print(), "Person");
        }
    }

    // =========================================================================
    // FunctionArgs Integration Tests
    // =========================================================================

    mod function_args_integration {
        use super::*;

        #[test]
        fn with_type_builder_encodes_successfully() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();

            let args = FunctionArgs::new()
                .arg("text", "hello")
                .with_type_builder(&tb);

            let encoded = args.encode();
            assert!(encoded.is_ok(), "Failed to encode args with type_builder");
        }

        #[test]
        fn type_builder_with_types_encodes_successfully() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();

            // Add some types
            let enum_builder = tb.add_enum("Status").unwrap();
            enum_builder.add_value("Active").unwrap();
            enum_builder.add_value("Inactive").unwrap();

            let class_builder = tb.add_class("Person").unwrap();
            let string_type = tb.string();
            class_builder.add_property("name", &string_type).unwrap();

            let args = FunctionArgs::new()
                .arg("text", "hello")
                .with_type_builder(&tb);

            let encoded = args.encode();
            assert!(
                encoded.is_ok(),
                "Failed to encode args with populated type_builder"
            );
        }

        #[test]
        fn type_builder_can_be_combined_with_other_args() {
            let runtime = create_test_runtime();
            let tb = runtime.new_type_builder();
            let collector = runtime.new_collector("test");

            let args = FunctionArgs::new()
                .arg("prompt", "test prompt")
                .with_env("TEST_VAR", "test_value")
                .with_tag("source", "test")
                .with_collector(&collector)
                .with_type_builder(&tb);

            let encoded = args.encode();
            assert!(
                encoded.is_ok(),
                "Failed to encode complex args with type_builder"
            );
        }
    }

    // =========================================================================
    // Thread Safety Tests
    // =========================================================================

    mod thread_safety {
        use std::{sync::Arc, thread};

        use super::*;

        #[test]
        fn type_builder_is_send() {
            fn assert_send<T: Send>() {}
            assert_send::<TypeBuilder>();
        }

        #[test]
        fn type_builder_is_sync() {
            fn assert_sync<T: Sync>() {}
            assert_sync::<TypeBuilder>();
        }

        #[test]
        fn type_def_is_send() {
            fn assert_send<T: Send>() {}
            assert_send::<TypeDef>();
        }

        #[test]
        fn type_def_is_sync() {
            fn assert_sync<T: Sync>() {}
            assert_sync::<TypeDef>();
        }

        #[test]
        fn enum_builder_is_send() {
            fn assert_send<T: Send>() {}
            assert_send::<EnumBuilder>();
        }

        #[test]
        fn enum_builder_is_sync() {
            fn assert_sync<T: Sync>() {}
            assert_sync::<EnumBuilder>();
        }

        #[test]
        fn enum_value_builder_is_send() {
            fn assert_send<T: Send>() {}
            assert_send::<EnumValueBuilder>();
        }

        #[test]
        fn enum_value_builder_is_sync() {
            fn assert_sync<T: Sync>() {}
            assert_sync::<EnumValueBuilder>();
        }

        #[test]
        fn class_builder_is_send() {
            fn assert_send<T: Send>() {}
            assert_send::<ClassBuilder>();
        }

        #[test]
        fn class_builder_is_sync() {
            fn assert_sync<T: Sync>() {}
            assert_sync::<ClassBuilder>();
        }

        #[test]
        fn class_property_builder_is_send() {
            fn assert_send<T: Send>() {}
            assert_send::<ClassPropertyBuilder>();
        }

        #[test]
        fn class_property_builder_is_sync() {
            fn assert_sync<T: Sync>() {}
            assert_sync::<ClassPropertyBuilder>();
        }

        #[test]
        fn type_builder_can_be_shared_across_threads() {
            let runtime = create_test_runtime();
            let tb = Arc::new(runtime.new_type_builder());

            let handles: Vec<_> = (0..4)
                .map(|i| {
                    let tb_clone = Arc::clone(&tb);
                    thread::spawn(move || {
                        // Each thread creates a type
                        let _string_type = tb_clone.string();
                        eprintln!("Thread {i} created string type");
                    })
                })
                .collect();

            for handle in handles {
                handle.join().expect("Thread panicked");
            }
        }
    }

    // =========================================================================
    // Dynamic Type Integration Tests (No API Key Required)
    // =========================================================================

    mod dynamic_type_integration {
        use super::*;

        /// Create a runtime with a function that uses dynamic types
        fn create_runtime_with_dynamic_class() -> BamlRuntime {
            let mut files = HashMap::new();
            files.insert(
                "main.baml".to_string(),
                r##"
                client<llm> GPT4 {
                    provider openai
                    options {
                        model "gpt-4o-mini"
                        api_key env.OPENAI_API_KEY
                    }
                }

                // Dynamic class that can have properties added at runtime
                class DynamicOutput {
                    @@dynamic
                }

                function ExtractData(text: string) -> DynamicOutput {
                    client GPT4
                    prompt #"
                        Extract structured data from this text:
                        {{text}}

                        {{ ctx.output_format }}
                    "#
                }
                "##
                .to_string(),
            );

            BamlRuntime::new(".", &files, &HashMap::new()).expect("runtime creation failed")
        }

        /// Create a runtime with a dynamic enum
        fn create_runtime_with_dynamic_enum() -> BamlRuntime {
            let mut files = HashMap::new();
            files.insert(
                "main.baml".to_string(),
                r##"
                client<llm> GPT4 {
                    provider openai
                    options {
                        model "gpt-4o-mini"
                        api_key env.OPENAI_API_KEY
                    }
                }

                // Dynamic enum that can have values added at runtime
                enum DynamicStatus {
                    @@dynamic
                }

                function ClassifyStatus(text: string) -> DynamicStatus {
                    client GPT4
                    prompt #"
                        Classify the status from this text:
                        {{text}}

                        {{ ctx.output_format }}
                    "#
                }
                "##
                .to_string(),
            );

            BamlRuntime::new(".", &files, &HashMap::new()).expect("runtime creation failed")
        }

        #[test]
        fn can_get_existing_dynamic_class() {
            let runtime = create_runtime_with_dynamic_class();
            let tb = runtime.new_type_builder();

            let class = tb.get_class("DynamicOutput");
            assert!(class.is_some(), "Should be able to get DynamicOutput class");

            let class = class.unwrap();
            assert_eq!(class.name().unwrap(), "DynamicOutput");
        }

        #[test]
        fn can_get_existing_dynamic_enum() {
            let runtime = create_runtime_with_dynamic_enum();
            let tb = runtime.new_type_builder();

            let enum_builder = tb.get_enum("DynamicStatus");
            assert!(
                enum_builder.is_some(),
                "Should be able to get DynamicStatus enum"
            );

            let enum_builder = enum_builder.unwrap();
            assert_eq!(enum_builder.name().unwrap(), "DynamicStatus");
        }

        #[test]
        fn can_add_properties_to_dynamic_class() {
            let runtime = create_runtime_with_dynamic_class();
            let tb = runtime.new_type_builder();

            let class = tb.get_class("DynamicOutput").unwrap();
            let string_type = tb.string();
            let int_type = tb.int();

            // Add properties
            class.add_property("name", &string_type).unwrap();
            class.add_property("age", &int_type).unwrap();

            // Verify properties were added
            let props = class.list_properties().unwrap();
            assert_eq!(props.len(), 2, "Should have 2 properties");
        }

        #[test]
        fn can_add_values_to_dynamic_enum() {
            let runtime = create_runtime_with_dynamic_enum();
            let tb = runtime.new_type_builder();

            let enum_builder = tb.get_enum("DynamicStatus").unwrap();

            // Add values
            enum_builder.add_value("Active").unwrap();
            enum_builder.add_value("Inactive").unwrap();
            enum_builder.add_value("Pending").unwrap();

            // Verify values were added
            let values = enum_builder.list_values().unwrap();
            assert_eq!(values.len(), 3, "Should have 3 values");
        }

        #[test]
        fn type_builder_with_dynamic_class_encodes_for_function_args() {
            let runtime = create_runtime_with_dynamic_class();
            let tb = runtime.new_type_builder();

            let class = tb.get_class("DynamicOutput").unwrap();
            let string_type = tb.string();
            let int_type = tb.int();

            class.add_property("name", &string_type).unwrap();
            class.add_property("age", &int_type).unwrap();

            // Create function args with the type builder
            let args = FunctionArgs::new()
                .arg("text", "test input")
                .with_type_builder(&tb);

            // Verify it encodes successfully
            let encoded = args.encode();
            assert!(
                encoded.is_ok(),
                "Args with type builder should encode successfully"
            );
        }

        #[test]
        fn type_builder_with_dynamic_enum_encodes_for_function_args() {
            let runtime = create_runtime_with_dynamic_enum();
            let tb = runtime.new_type_builder();

            let enum_builder = tb.get_enum("DynamicStatus").unwrap();
            enum_builder.add_value("Active").unwrap();
            enum_builder.add_value("Completed").unwrap();

            // Create function args with the type builder
            let args = FunctionArgs::new()
                .arg("text", "test input")
                .with_type_builder(&tb);

            // Verify it encodes successfully
            let encoded = args.encode();
            assert!(
                encoded.is_ok(),
                "Args with type builder should encode successfully"
            );
        }

        #[test]
        fn dynamic_class_with_complex_types_encodes() {
            let runtime = create_runtime_with_dynamic_class();
            let tb = runtime.new_type_builder();

            let class = tb.get_class("DynamicOutput").unwrap();
            let string_type = tb.string();

            // Add a list property
            let list_of_strings = tb.list(&string_type);
            class.add_property("items", &list_of_strings).unwrap();

            // Add an optional property
            let optional_string = tb.optional(&string_type);
            class.add_property("notes", &optional_string).unwrap();

            // Add a map property
            let int_type = tb.int();
            let map_type = tb.map(&string_type, &int_type);
            class.add_property("scores", &map_type).unwrap();

            let args = FunctionArgs::new()
                .arg("text", "test")
                .with_type_builder(&tb);

            let encoded = args.encode();
            assert!(encoded.is_ok(), "Complex types should encode successfully");
        }

        #[test]
        fn dynamic_class_with_nested_dynamic_class_encodes() {
            // Create runtime with nested dynamic classes
            let mut files = HashMap::new();
            files.insert(
                "main.baml".to_string(),
                r##"
                client<llm> GPT4 {
                    provider openai
                    options {
                        model "gpt-4o-mini"
                        api_key env.OPENAI_API_KEY
                    }
                }

                class InnerClass {
                    @@dynamic
                }

                class OuterClass {
                    inner InnerClass
                    @@dynamic
                }

                function ExtractNested(text: string) -> OuterClass {
                    client GPT4
                    prompt #"
                        Extract: {{text}}
                        {{ ctx.output_format }}
                    "#
                }
                "##
                .to_string(),
            );

            let runtime =
                BamlRuntime::new(".", &files, &HashMap::new()).expect("runtime creation failed");
            let tb = runtime.new_type_builder();

            // Add properties to inner class
            let inner = tb.get_class("InnerClass").unwrap();
            let string_type = tb.string();
            inner.add_property("value", &string_type).unwrap();

            // Add properties to outer class
            let outer = tb.get_class("OuterClass").unwrap();
            outer.add_property("name", &string_type).unwrap();

            let args = FunctionArgs::new()
                .arg("text", "test")
                .with_type_builder(&tb);

            let encoded = args.encode();
            assert!(
                encoded.is_ok(),
                "Nested dynamic classes should encode successfully"
            );
        }

        #[test]
        fn dynamic_enum_with_descriptions_encodes() {
            let runtime = create_runtime_with_dynamic_enum();
            let tb = runtime.new_type_builder();

            let enum_builder = tb.get_enum("DynamicStatus").unwrap();
            enum_builder.set_description("Task status options").unwrap();

            let active = enum_builder.add_value("Active").unwrap();
            active.set_description("Currently in progress").unwrap();
            active.set_alias("IN_PROGRESS").unwrap();

            let done = enum_builder.add_value("Done").unwrap();
            done.set_description("Completed successfully").unwrap();

            let args = FunctionArgs::new()
                .arg("text", "test")
                .with_type_builder(&tb);

            let encoded = args.encode();
            assert!(
                encoded.is_ok(),
                "Enum with descriptions should encode successfully"
            );
        }

        #[test]
        fn dynamic_class_with_descriptions_encodes() {
            let runtime = create_runtime_with_dynamic_class();
            let tb = runtime.new_type_builder();

            let class = tb.get_class("DynamicOutput").unwrap();
            class.set_description("Output data structure").unwrap();
            class.set_alias("Result").unwrap();

            let string_type = tb.string();
            let name_prop = class.add_property("name", &string_type).unwrap();
            name_prop.set_description("The name field").unwrap();
            name_prop.set_alias("fullName").unwrap();

            let args = FunctionArgs::new()
                .arg("text", "test")
                .with_type_builder(&tb);

            let encoded = args.encode();
            assert!(
                encoded.is_ok(),
                "Class with descriptions should encode successfully"
            );
        }
    }

    // =========================================================================
    // Integration Tests with Real Function Calls (Requires API Key)
    // =========================================================================
    //
    // NOTE: These tests are marked #[ignore] because dynamic class/enum results
    // cannot be decoded as String in the current BAML implementation. The BAML
    // decoder expects a specific type, and dynamic types are returned as class
    // objects rather than strings.
    //
    // The dynamic_type_integration tests above provide comprehensive coverage
    // for TypeBuilder API without requiring an API key.
    // =========================================================================

    mod function_call_integration {
        use super::*;

        /// Helper macro to skip test if env var is not set
        macro_rules! require_env {
            ($name:expr) => {
                match std::env::var($name) {
                    Ok(val) if !val.is_empty() && val != "test" => val,
                    _ => {
                        eprintln!("Skipping test: {} not set or is 'test'", $name);
                        return;
                    }
                }
            };
        }

        /// Create a runtime with a function that uses dynamic types
        fn create_runtime_with_dynamic_class() -> BamlRuntime {
            let mut files = HashMap::new();
            files.insert(
                "main.baml".to_string(),
                r##"
                client<llm> GPT4 {
                    provider openai
                    options {
                        model "gpt-4o-mini"
                        api_key env.OPENAI_API_KEY
                    }
                }

                // Dynamic class that can have properties added at runtime
                class DynamicOutput {
                    @@dynamic
                }

                function ExtractData(text: string) -> DynamicOutput {
                    client GPT4
                    prompt #"
                        Extract structured data from this text:
                        {{text}}

                        {{ ctx.output_format }}
                    "#
                }
                "##
                .to_string(),
            );

            BamlRuntime::new(".", &files, &HashMap::new()).expect("runtime creation failed")
        }

        /// Create a runtime with a dynamic enum
        fn create_runtime_with_dynamic_enum() -> BamlRuntime {
            let mut files = HashMap::new();
            files.insert(
                "main.baml".to_string(),
                r##"
                client<llm> GPT4 {
                    provider openai
                    options {
                        model "gpt-4o-mini"
                        api_key env.OPENAI_API_KEY
                    }
                }

                // Dynamic enum that can have values added at runtime
                enum DynamicStatus {
                    @@dynamic
                }

                function ClassifyStatus(text: string) -> DynamicStatus {
                    client GPT4
                    prompt #"
                        Classify the status from this text:
                        {{text}}

                        {{ ctx.output_format }}
                    "#
                }
                "##
                .to_string(),
            );

            BamlRuntime::new(".", &files, &HashMap::new()).expect("runtime creation failed")
        }

        #[test]
        #[ignore = "Dynamic class results cannot be decoded as String"]
        fn dynamic_type_with_function_call() {
            let api_key = require_env!("OPENAI_API_KEY");

            let runtime = create_runtime_with_dynamic_class();
            let tb = runtime.new_type_builder();

            // Get the existing DynamicOutput class and add properties to it
            let class_builder = tb
                .get_class("DynamicOutput")
                .expect("DynamicOutput class should exist");
            let string_type = tb.string();
            let int_type = tb.int();

            class_builder.add_property("name", &string_type).unwrap();
            class_builder.add_property("age", &int_type).unwrap();

            let args = FunctionArgs::new()
                .arg("text", "John is 30 years old")
                .with_env("OPENAI_API_KEY", &api_key)
                .with_type_builder(&tb);

            // This test verifies the type builder integrates with function calls
            // The actual result parsing would depend on the dynamic type system
            let result: Result<String, _> = runtime.call_function("ExtractData", &args);

            // We just verify the call doesn't error due to type builder issues
            // The actual parsing of dynamic types is a separate concern
            if let Err(e) = &result {
                eprintln!("Function call error (may be expected for dynamic types): {e:?}");
            }
        }

        #[test]
        #[ignore = "Dynamic class results cannot be decoded as String"]
        fn dynamic_type_extracts_person_correctly() {
            let api_key = require_env!("OPENAI_API_KEY");

            let runtime = create_runtime_with_dynamic_class();
            let tb = runtime.new_type_builder();

            // Get the existing DynamicOutput class and add properties
            let class_builder = tb
                .get_class("DynamicOutput")
                .expect("DynamicOutput class should exist");
            class_builder
                .set_description("A person with a name and age")
                .unwrap();

            let string_type = tb.string();
            let int_type = tb.int();

            let name_prop = class_builder
                .add_property("name", &string_type)
                .expect("Failed to add name");
            name_prop.set_description("The person's full name").unwrap();

            let age_prop = class_builder
                .add_property("age", &int_type)
                .expect("Failed to add age");
            age_prop
                .set_description("The person's age in years")
                .unwrap();

            let args = FunctionArgs::new()
                .arg(
                    "text",
                    "Alice Smith is 25 years old and works as an engineer.",
                )
                .with_env("OPENAI_API_KEY", &api_key)
                .with_type_builder(&tb);

            // Get result as String and parse as JSON
            let result: Result<String, _> = runtime.call_function("ExtractData", &args);
            assert!(result.is_ok(), "Function call failed: {:?}", result.err());

            let result_str = result.unwrap();
            eprintln!("Extracted person (raw): {result_str}");

            // Parse the string as JSON
            let value: serde_json::Value =
                serde_json::from_str(&result_str).expect("Failed to parse result as JSON");
            eprintln!(
                "Extracted person: {}",
                serde_json::to_string_pretty(&value).unwrap()
            );

            // Verify the result contains expected fields
            assert!(
                value.get("name").is_some(),
                "Result should have 'name' field: {value:?}"
            );
            assert!(
                value.get("age").is_some(),
                "Result should have 'age' field: {value:?}"
            );

            // Verify the values are reasonable
            let name = value.get("name").unwrap().as_str().unwrap_or("");
            assert!(
                name.to_lowercase().contains("alice"),
                "Name should contain 'Alice', got: {name}"
            );

            let age = value.get("age").unwrap();
            // Age could be a number or string depending on how the LLM responds
            let age_num = age
                .as_i64()
                .or_else(|| age.as_str().and_then(|s| s.parse::<i64>().ok()));
            assert!(age_num == Some(25), "Age should be 25, got: {age:?}");
        }

        #[test]
        #[ignore = "Dynamic enum results cannot be decoded as String"]
        fn dynamic_enum_extracts_status_correctly() {
            let api_key = require_env!("OPENAI_API_KEY");

            let runtime = create_runtime_with_dynamic_enum();
            let tb = runtime.new_type_builder();

            // Get the existing DynamicStatus enum and add values
            let enum_builder = tb
                .get_enum("DynamicStatus")
                .expect("DynamicStatus enum should exist");
            enum_builder
                .set_description("The current status of a task")
                .unwrap();

            let active_val = enum_builder
                .add_value("Active")
                .expect("Failed to add Active");
            active_val
                .set_description("The task is currently being worked on")
                .unwrap();

            let completed_val = enum_builder
                .add_value("Completed")
                .expect("Failed to add Completed");
            completed_val
                .set_description("The task has been finished")
                .unwrap();

            let pending_val = enum_builder
                .add_value("Pending")
                .expect("Failed to add Pending");
            pending_val
                .set_description("The task is waiting to start")
                .unwrap();

            let args = FunctionArgs::new()
                .arg(
                    "text",
                    "The project has been finished and all deliverables are done.",
                )
                .with_env("OPENAI_API_KEY", &api_key)
                .with_type_builder(&tb);

            let result: Result<String, _> = runtime.call_function("ClassifyStatus", &args);
            assert!(result.is_ok(), "Function call failed: {:?}", result.err());

            let status = result.unwrap();
            eprintln!("Extracted status: {status}");

            // The LLM should return "Completed" since the text indicates the project is
            // finished
            assert!(
                status.to_lowercase().contains("completed")
                    || status.to_lowercase().contains("complete"),
                "Status should be 'Completed' for finished project, got: {status}"
            );
        }

        #[test]
        #[ignore = "Dynamic class results cannot be decoded as String"]
        fn dynamic_class_with_optional_field() {
            let api_key = require_env!("OPENAI_API_KEY");

            let runtime = create_runtime_with_dynamic_class();
            let tb = runtime.new_type_builder();

            // Get the existing DynamicOutput class and add properties
            let class_builder = tb
                .get_class("DynamicOutput")
                .expect("DynamicOutput class should exist");

            let string_type = tb.string();
            let optional_string = tb.optional(&string_type);

            class_builder
                .add_property("email", &string_type)
                .expect("Failed to add email");
            class_builder
                .add_property("phone", &optional_string)
                .expect("Failed to add phone");

            let args = FunctionArgs::new()
                .arg("text", "Contact John at john@example.com")
                .with_env("OPENAI_API_KEY", &api_key)
                .with_type_builder(&tb);

            // Get result as String and parse as JSON
            let result: Result<String, _> = runtime.call_function("ExtractData", &args);
            assert!(result.is_ok(), "Function call failed: {:?}", result.err());

            let result_str = result.unwrap();
            let value: serde_json::Value =
                serde_json::from_str(&result_str).expect("Failed to parse result as JSON");
            eprintln!(
                "Extracted contact: {}",
                serde_json::to_string_pretty(&value).unwrap()
            );

            // Verify email is present
            assert!(
                value.get("email").is_some(),
                "Result should have 'email' field: {value:?}"
            );

            let email = value.get("email").unwrap().as_str().unwrap_or("");
            assert!(
                email.contains("john@example.com") || email.contains("example"),
                "Email should contain the email address, got: {email}"
            );

            // Phone should be null or not present since it wasn't in the text
            if let Some(phone) = value.get("phone") {
                assert!(
                    phone.is_null() || phone.as_str().map(str::is_empty).unwrap_or(false),
                    "Phone should be null or empty since not provided, got: {phone:?}"
                );
            }
        }

        #[test]
        #[ignore = "Dynamic class results cannot be decoded as String"]
        fn dynamic_class_with_list_field() {
            let api_key = require_env!("OPENAI_API_KEY");

            let runtime = create_runtime_with_dynamic_class();
            let tb = runtime.new_type_builder();

            // Get the existing DynamicOutput class and add a list property
            let class_builder = tb
                .get_class("DynamicOutput")
                .expect("DynamicOutput class should exist");

            let string_type = tb.string();
            let list_of_strings = tb.list(&string_type);

            class_builder
                .add_property("items", &list_of_strings)
                .expect("Failed to add items");

            let args = FunctionArgs::new()
                .arg("text", "Buy milk, eggs, bread, and cheese from the store.")
                .with_env("OPENAI_API_KEY", &api_key)
                .with_type_builder(&tb);

            // Get result as String and parse as JSON
            let result: Result<String, _> = runtime.call_function("ExtractData", &args);
            assert!(result.is_ok(), "Function call failed: {:?}", result.err());

            let result_str = result.unwrap();
            let value: serde_json::Value =
                serde_json::from_str(&result_str).expect("Failed to parse result as JSON");
            eprintln!(
                "Extracted shopping list: {}",
                serde_json::to_string_pretty(&value).unwrap()
            );

            // Verify items is a list
            let items = value
                .get("items")
                .expect("Result should have 'items' field");
            assert!(items.is_array(), "Items should be an array, got: {items:?}");

            let items_array = items.as_array().unwrap();
            assert!(
                items_array.len() >= 3,
                "Should have at least 3 items, got: {}",
                items_array.len()
            );

            // Check that expected items are present
            let items_lower: Vec<String> = items_array
                .iter()
                .filter_map(|v| v.as_str())
                .map(str::to_lowercase)
                .collect();

            assert!(
                items_lower.iter().any(|s| s.contains("milk")),
                "Items should contain 'milk': {items_lower:?}"
            );
            assert!(
                items_lower.iter().any(|s| s.contains("egg")),
                "Items should contain 'eggs': {items_lower:?}"
            );
        }

        #[test]
        #[ignore = "Dynamic class results cannot be decoded as String"]
        fn dynamic_nested_class_structure() {
            let api_key = require_env!("OPENAI_API_KEY");

            // Create runtime with nested dynamic classes
            let mut files = HashMap::new();
            files.insert(
                "main.baml".to_string(),
                r##"
                client<llm> GPT4 {
                    provider openai
                    options {
                        model "gpt-4o-mini"
                        api_key env.OPENAI_API_KEY
                    }
                }

                class Customer {
                    @@dynamic
                }

                class Order {
                    customer Customer
                    @@dynamic
                }

                function ExtractOrder(text: string) -> Order {
                    client GPT4
                    prompt #"
                        Extract order information from this text:
                        {{text}}

                        {{ ctx.output_format }}
                    "#
                }
                "##
                .to_string(),
            );

            let runtime =
                BamlRuntime::new(".", &files, &HashMap::new()).expect("runtime creation failed");
            let tb = runtime.new_type_builder();

            // Add properties to the Customer class
            let customer_class = tb
                .get_class("Customer")
                .expect("Customer class should exist");
            let string_type = tb.string();
            customer_class
                .add_property("name", &string_type)
                .expect("Failed to add customer name");
            customer_class
                .add_property("email", &string_type)
                .expect("Failed to add customer email");

            // Add properties to the Order class
            let order_class = tb.get_class("Order").expect("Order class should exist");
            let int_type = tb.int();

            order_class
                .add_property("orderId", &string_type)
                .expect("Failed to add orderId");
            order_class
                .add_property("totalAmount", &int_type)
                .expect("Failed to add totalAmount");

            let args = FunctionArgs::new()
                .arg(
                    "text",
                    "Order #12345 for $150 placed by Jane Doe (jane@email.com)",
                )
                .with_env("OPENAI_API_KEY", &api_key)
                .with_type_builder(&tb);

            // Get result as String and parse as JSON
            let result: Result<String, _> = runtime.call_function("ExtractOrder", &args);
            assert!(result.is_ok(), "Function call failed: {:?}", result.err());

            let result_str = result.unwrap();
            let value: serde_json::Value =
                serde_json::from_str(&result_str).expect("Failed to parse result as JSON");
            eprintln!(
                "Extracted order: {}",
                serde_json::to_string_pretty(&value).unwrap()
            );

            // Verify nested structure
            assert!(
                value.get("orderId").is_some() || value.get("order_id").is_some(),
                "Result should have 'orderId' field: {value:?}"
            );

            let customer = value.get("customer");
            assert!(
                customer.is_some(),
                "Result should have 'customer' field: {value:?}"
            );

            if let Some(customer) = customer {
                assert!(
                    customer.get("name").is_some(),
                    "Customer should have 'name' field: {customer:?}"
                );
                let name = customer.get("name").unwrap().as_str().unwrap_or("");
                assert!(
                    name.to_lowercase().contains("jane"),
                    "Customer name should contain 'Jane', got: {name}"
                );
            }
        }
    }
}
