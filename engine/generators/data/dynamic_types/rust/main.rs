// Test file for TypeBuilder and dynamic types codegen
// This tests the generated TypeBuilder wrappers for @@dynamic classes and enums

mod baml_client;

// TypeBuilder import commented out until TypeBuilder methods are implemented
// use baml_client::TypeBuilder;
use baml_client::types::*;

fn main() {
    println!("Test - dynamic_types baml_client module loaded successfully!");
}

// NOTE: type_builder_tests are commented out because TypeBuilder methods
// (string(), Person(), etc.) are not yet implemented in the generated code.
// These tests demonstrate the intended API once TypeBuilder is fully implemented.
//
// #[cfg(test)]
// mod type_builder_tests {
//     use super::*;
//
//     #[test]
//     fn test_type_builder_creation() {
//         let tb = TypeBuilder::new();
//         let string_type = tb.string();
//         let int_type = tb.int();
//         let float_type = tb.float();
//         let bool_type = tb.bool();
//         assert!(!string_type.print().is_empty());
//         assert!(!int_type.print().is_empty());
//         assert!(!float_type.print().is_empty());
//         assert!(!bool_type.print().is_empty());
//     }
//
//     // ... (additional tests omitted for brevity)
// }

#[cfg(test)]
mod dynamic_struct_field_tests {
    use super::*;
    use baml_client::BamlValue;

    #[test]
    fn test_dynamic_class_has_dynamic_field() {
        // Person is @@dynamic, so it should have __dynamic field
        let person = Person {
            name: "Alice".to_string(),
            age: 30,
            __dynamic: std::collections::HashMap::new(),
        };

        assert!(!person.has("email"));
    }

    #[test]
    fn test_dynamic_class_get_methods() {
        let mut dynamic = std::collections::HashMap::new();
        dynamic.insert(
            "occupation".to_string(),
            BamlValue::String("Engineer".to_string()),
        );
        dynamic.insert("years_experience".to_string(), BamlValue::Int(5));

        let person = Person {
            name: "Bob".to_string(),
            age: 25,
            __dynamic: dynamic,
        };

        // has() should work
        assert!(person.has("occupation"));
        assert!(person.has("years_experience"));
        assert!(!person.has("nonexistent"));

        // get() should work
        let occupation: String = person.get("occupation").expect("Should get occupation");
        assert_eq!(occupation, "Engineer");

        let years: i64 = person.get("years_experience").expect("Should get years");
        assert_eq!(years, 5);
    }

    #[test]
    fn test_dynamic_class_get_ref() {
        let mut dynamic = std::collections::HashMap::new();
        dynamic.insert("tag".to_string(), BamlValue::String("rust".to_string()));

        let person = Person {
            name: "Charlie".to_string(),
            age: 35,
            __dynamic: dynamic,
        };

        // get_ref should return a reference
        let tag_ref = person.get_ref("tag");
        assert!(tag_ref.is_some());

        let nonexistent = person.get_ref("nonexistent");
        assert!(nonexistent.is_none());
    }

    #[test]
    fn test_dynamic_class_iterate_fields() {
        let mut dynamic = std::collections::HashMap::new();
        dynamic.insert("field1".to_string(), BamlValue::Int(1));
        dynamic.insert("field2".to_string(), BamlValue::Int(2));

        let person = Person {
            name: "Dave".to_string(),
            age: 40,
            __dynamic: dynamic,
        };

        let field_count = person.dynamic_fields().count();
        assert_eq!(field_count, 2);
    }

    #[test]
    fn test_non_dynamic_class_has_no_dynamic_field() {
        // Address is NOT @@dynamic, so it should NOT have __dynamic field
        // This test verifies the struct compiles without __dynamic
        let address = Address {
            street: "123 Main St".to_string(),
            city: "Springfield".to_string(),
            country: "USA".to_string(),
        };

        assert_eq!(address.street, "123 Main St");
        // No __dynamic field - if this compiles, the test passes
    }
}

#[cfg(test)]
mod dynamic_enum_tests {
    use super::*;

    #[test]
    fn test_dynamic_enum_has_dynamic_variant() {
        // Category is @@dynamic, so it should have _Dynamic variant
        let dynamic_category = Category::_Dynamic("Sports".to_string());
        assert_eq!(format!("{}", dynamic_category), "Sports");
    }

    #[test]
    fn test_dynamic_enum_from_str() {
        // Known variants should parse
        let tech: Category = "Technology".parse().expect("Should parse Technology");
        assert_eq!(tech, Category::Technology);

        // Unknown variants should become _Dynamic for dynamic enums
        let sports: Category = "Sports".parse().expect("Should parse Sports");
        assert_eq!(sports, Category::_Dynamic("Sports".to_string()));
    }

    #[test]
    fn test_non_dynamic_enum_from_str() {
        // Status is NOT @@dynamic, so unknown variants should error
        let active: Status = "Active".parse().expect("Should parse Active");
        assert_eq!(active, Status::Active);

        // Unknown variants should fail
        let unknown: Result<Status, ()> = "Unknown".parse();
        assert!(unknown.is_err());
    }

    #[test]
    fn test_dynamic_enum_default() {
        // Dynamic enums should have a Default impl (first variant)
        let default_category = Category::default();
        assert_eq!(default_category, Category::Technology);
    }

    #[test]
    fn test_non_dynamic_enum_default() {
        // Non-dynamic enums should also have a Default impl (first variant)
        let default_status = Status::default();
        assert_eq!(default_status, Status::Active);
    }

    #[test]
    fn test_dynamic_enum_to_string() {
        assert_eq!(Category::Technology.to_string(), "Technology");
        assert_eq!(Category::Science.to_string(), "Science");
        assert_eq!(Category::Arts.to_string(), "Arts");
        assert_eq!(
            Category::_Dynamic("Custom".to_string()).to_string(),
            "Custom"
        );
    }
}

// NOTE: function_with_type_builder_tests are commented out because TypeBuilder methods
// and function calling infrastructure are not yet fully implemented.
// These tests demonstrate the intended API once everything is in place.
//
// #[cfg(test)]
// mod function_with_type_builder_tests {
//     use super::*;
//     use baml_client::B;
//     use baml_client::FunctionOptions;
//     use baml::BamlValue;
//
//     #[tokio::test]
//     async fn test_dynamic_property_access_from_response() {
//         let tb = TypeBuilder::new();
//         tb.Person().add_property("occupation", &tb.string()).expect("Add occupation");
//         // ... (test implementation omitted for brevity)
//     }
//
//     // ... (additional tests omitted for brevity)
// }
