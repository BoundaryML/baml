// Test file for TypeBuilder and dynamic types codegen
// This tests the generated TypeBuilder wrappers for @@dynamic classes and enums

mod baml_client;

use baml_client::types::*;
use baml_client::TypeBuilder;

fn main() {
    println!("Test - dynamic_types baml_client module loaded successfully!");
}

#[cfg(test)]
mod type_builder_tests {
    use super::*;

    #[test]
    fn test_type_builder_creation() {
        let tb = TypeBuilder::new();
        // Test primitive types
        let string_type = tb.string();
        let int_type = tb.int();
        let float_type = tb.float();
        let bool_type = tb.bool();
        let null_type = tb.null();

        assert!(!string_type.print().is_empty());
        assert!(!int_type.print().is_empty());
        assert!(!float_type.print().is_empty());
        assert!(!bool_type.print().is_empty());
        assert!(!null_type.print().is_empty());
    }

    #[test]
    fn test_type_builder_composite_types() {
        let tb = TypeBuilder::new();

        // Test composite types
        let list_type = tb.list(&tb.string());
        let optional_type = tb.optional(&tb.int());
        let map_type = tb.map(&tb.string(), &tb.int());
        let union_type = tb.union(&[&tb.string(), &tb.int()]);

        assert!(!list_type.print().is_empty());
        assert!(!optional_type.print().is_empty());
        assert!(!map_type.print().is_empty());
        assert!(!union_type.print().is_empty());
    }

    #[test]
    fn test_type_builder_literal_types() {
        let tb = TypeBuilder::new();

        // Test literal types
        let literal_string = tb.literal_string("hello");
        let literal_int = tb.literal_int(42);
        let literal_bool = tb.literal_bool(true);

        assert!(!literal_string.print().is_empty());
        assert!(!literal_int.print().is_empty());
        assert!(!literal_bool.print().is_empty());
    }

    #[test]
    fn test_schema_class_access() {
        let tb = TypeBuilder::new();

        // Access schema-defined classes via methods
        let person_builder = tb.Person();
        let address_builder = tb.Address();
        let article_builder = tb.Article();

        // Get the type definitions
        let person_type = person_builder.r#type();
        let address_type = address_builder.r#type();
        let article_type = article_builder.r#type();

        assert!(!person_type.print().is_empty());
        assert!(!address_type.print().is_empty());
        assert!(!article_type.print().is_empty());
    }

    #[test]
    fn test_schema_field_access() {
        let tb = TypeBuilder::new();

        // Access schema-defined fields via methods
        let name_field = tb.Person().property_name();
        let age_field = tb.Person().property_age();
        let street_field = tb.Address().property_street();

        // Verify we can get names
        assert_eq!(name_field.name().unwrap(), "name");
        assert_eq!(age_field.name().unwrap(), "age");
        assert_eq!(street_field.name().unwrap(), "street");
    }

    #[test]
    fn test_schema_enum_access() {
        let tb = TypeBuilder::new();

        // Access schema-defined enums via methods
        let category_builder = tb.Category();
        let priority_builder = tb.Priority();
        let status_builder = tb.Status();

        // Get the type definitions
        let category_type = category_builder.r#type();
        let priority_type = priority_builder.r#type();
        let status_type = status_builder.r#type();

        assert!(!category_type.print().is_empty());
        assert!(!priority_type.print().is_empty());
        assert!(!status_type.print().is_empty());
    }

    #[test]
    fn test_schema_enum_value_access() {
        let tb = TypeBuilder::new();

        // Access schema-defined enum values via methods
        let technology = tb.Category().value_Technology();
        let science = tb.Category().value_Science();
        let high = tb.Priority().value_High();
        let active = tb.Status().value_Active();

        // Verify we can get names
        assert_eq!(technology.name().unwrap(), "Technology");
        assert_eq!(science.name().unwrap(), "Science");
        assert_eq!(high.name().unwrap(), "High");
        assert_eq!(active.name().unwrap(), "Active");
    }

    #[test]
    fn test_dynamic_class_add_property() {
        let tb = TypeBuilder::new();

        // Person is @@dynamic, so we can add properties
        let occupation_prop = tb
            .Person()
            .add_property("occupation", &tb.string())
            .expect("Should be able to add property to dynamic class");

        assert_eq!(occupation_prop.name().unwrap(), "occupation");
    }

    #[test]
    fn test_dynamic_enum_add_value() {
        let tb = TypeBuilder::new();

        // Category is @@dynamic, so we can add values
        let sports_value = tb
            .Category()
            .add_value("Sports")
            .expect("Should be able to add value to dynamic enum");

        assert_eq!(sports_value.name().unwrap(), "Sports");
    }

    #[test]
    fn test_set_description_on_property() {
        let tb = TypeBuilder::new();

        // Set description on a field
        tb.Person()
            .property_name()
            .set_description("The person's full name")
            .expect("Should be able to set description");

        let description = tb.Person().property_name().description().unwrap();
        assert_eq!(description, Some("The person's full name".to_string()));
    }

    #[test]
    fn test_set_alias_on_enum_value() {
        let tb = TypeBuilder::new();

        // Set alias on an enum value
        tb.Category()
            .value_Technology()
            .set_alias("TECH")
            .expect("Should be able to set alias");

        let alias = tb.Category().value_Technology().alias().unwrap();
        assert_eq!(alias, Some("TECH".to_string()));
    }

    #[test]
    fn test_fully_dynamic_enum_creation() {
        let tb = TypeBuilder::new();

        // Create a fully dynamic enum at runtime
        let my_enum = tb.add_enum("MyDynamicEnum").expect("Should create enum");
        my_enum.add_value("Option1").expect("Add value 1");
        my_enum.add_value("Option2").expect("Add value 2");

        // Verify the enum type is available
        let enum_type = my_enum.as_type().expect("Should get type");
        assert!(!enum_type.print().is_empty());
    }

    #[test]
    fn test_fully_dynamic_class_creation() {
        let tb = TypeBuilder::new();

        // Create a fully dynamic class at runtime
        let my_class = tb.add_class("MyDynamicClass").expect("Should create class");
        my_class
            .add_property("field1", &tb.string())
            .expect("Add field 1");
        my_class
            .add_property("field2", &tb.int())
            .expect("Add field 2");

        // Verify the class type is available
        let class_type = my_class.as_type().expect("Should get type");
        assert!(!class_type.print().is_empty());
    }

    #[test]
    fn test_non_dynamic_class_has_no_add_property() {
        // This is a compile-time check - Address is not @@dynamic
        // so AddressClassBuilder should NOT have an add_property method.
        // If this test compiles, the check passes.

        let tb = TypeBuilder::new();
        let _address_builder = tb.Address();

        // AddressClassBuilder does NOT have add_property method
        // Uncommenting this line should cause a compile error:
        // address_builder.add_property("zip", &tb.string());
    }

    #[test]
    fn test_non_dynamic_enum_has_no_add_value() {
        // This is a compile-time check - Status is not @@dynamic
        // so StatusEnumBuilder should NOT have an add_value method.
        // If this test compiles, the check passes.

        let tb = TypeBuilder::new();
        let _status_builder = tb.Status();

        // StatusEnumBuilder does NOT have add_value method
        // Uncommenting this line should cause a compile error:
        // status_builder.add_value("Custom");
    }

    #[test]
    fn test_type_builder_clone() {
        let tb1 = TypeBuilder::new();
        let tb2 = tb1.clone();

        // Both should work independently
        let _ = tb1.string();
        let _ = tb2.int();
    }

    #[test]
    fn test_type_builder_default() {
        let tb = TypeBuilder::default();
        let _ = tb.string();
    }

    #[test]
    fn test_type_builder_debug() {
        let tb = TypeBuilder::new();
        let debug_str = format!("{:?}", tb);
        assert!(debug_str.contains("TypeBuilder"));
    }
}

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

// End-to-end TypeBuilder tests that actually call LLM functions
// These tests require OPENAI_API_KEY to be set
#[cfg(test)]
mod e2e_type_builder_tests {
    use super::*;
    use baml_client::{sync_client::B, BamlValue};

    #[test]
    fn test_dynamic_class_property_e2e() {
        let tb = TypeBuilder::new();

        // Add dynamic property "occupation" to Person class
        let occupation_prop = tb
            .Person()
            .add_property("occupation", &tb.string())
            .expect("Should add occupation property");

        // Add a description to help the LLM (use the returned property builder)
        occupation_prop
            .set_description("The person's job or profession")
            .expect("Should set description");

        // Call the function with TypeBuilder
        let result = B.GetPerson.with_type_builder(&tb).call(
            "A software engineer named Alice who is 30 years old and works as a backend developer",
        );

        let person = result.expect("LLM call should succeed");

        // Verify static fields
        assert!(!person.name.is_empty(), "Name should not be empty");
        assert!(person.age > 0, "Age should be positive");

        // Verify dynamic field "occupation" was populated
        assert!(
            person.has("occupation"),
            "Person should have dynamic 'occupation' field"
        );

        let occupation: String = person
            .get("occupation")
            .expect("Should be able to get occupation as String");
        assert!(
            !occupation.is_empty(),
            "Occupation should not be empty: got '{}'",
            occupation
        );

        println!(
            "Got person: {} (age {}), occupation: {}",
            person.name, person.age, occupation
        );
    }

    #[test]
    fn test_multiple_dynamic_properties_e2e() {
        let tb = TypeBuilder::new();

        // Add multiple dynamic properties and set descriptions on each
        let email_prop = tb
            .Person()
            .add_property("email", &tb.string())
            .expect("Add email");
        email_prop
            .set_description("Email address")
            .expect("Set email desc");

        let is_employed_prop = tb
            .Person()
            .add_property("is_employed", &tb.bool())
            .expect("Add is_employed");
        is_employed_prop
            .set_description("Whether currently employed")
            .expect("Set is_employed desc");

        let years_exp_prop = tb
            .Person()
            .add_property("years_experience", &tb.int())
            .expect("Add years_experience");
        years_exp_prop
            .set_description("Years of work experience")
            .expect("Set years_experience desc");

        let result = B.GetPerson.with_type_builder(&tb).call(
            "Bob Smith, age 35, email bob@example.com, currently employed with 10 years experience",
        );

        let person = result.expect("LLM call should succeed");

        // Verify static fields
        assert!(!person.name.is_empty());
        assert!(person.age > 0);

        // Verify all dynamic fields
        assert!(person.has("email"), "Should have email");
        assert!(person.has("is_employed"), "Should have is_employed");
        assert!(
            person.has("years_experience"),
            "Should have years_experience"
        );

        let email: String = person.get("email").expect("Get email");
        let is_employed: bool = person.get("is_employed").expect("Get is_employed");
        let years_exp: i64 = person
            .get("years_experience")
            .expect("Get years_experience");

        println!(
            "Person: {}, email: {}, employed: {}, years: {}",
            person.name, email, is_employed, years_exp
        );

        assert!(!email.is_empty(), "Email should not be empty");
    }

    #[test]
    fn test_dynamic_enum_value_e2e() {
        let tb = TypeBuilder::new();

        // Add new enum values to Category and set descriptions on each
        let sports = tb.Category().add_value("Sports").expect("Add Sports value");
        sports
            .set_description("Sports and athletics news")
            .expect("Set Sports desc");

        let politics = tb
            .Category()
            .add_value("Politics")
            .expect("Add Politics value");
        politics
            .set_description("Political news and government")
            .expect("Set Politics desc");

        let entertainment = tb
            .Category()
            .add_value("Entertainment")
            .expect("Add Entertainment value");
        entertainment
            .set_description("Movies, TV, celebrities")
            .expect("Set Entertainment desc");

        // Test with sports content
        let result = B.ClassifyArticle.with_type_builder(&tb).call(
            "The Lakers won the championship last night with a stunning 3-pointer in overtime",
        );

        let category = result.expect("LLM call should succeed");

        // The category should be Sports (our dynamically added value)
        // Since it's a dynamic enum, it will be the _Dynamic variant
        let category_str = category.to_string();
        println!("Category: {}", category_str);

        // It should be one of our categories
        assert!(
            category_str == "Sports"
                || category_str == "Technology"
                || category_str == "Science"
                || category_str == "Arts"
                || category_str == "Politics"
                || category_str == "Entertainment",
            "Should be a valid category, got: {}",
            category_str
        );
    }

    #[test]
    fn test_nested_dynamic_types_e2e() {
        let tb = TypeBuilder::new();

        // Add dynamic property to Person (nested in Article)
        tb.Person()
            .add_property("bio", &tb.string())
            .expect("Add bio to Person");

        // Add dynamic property to Article
        tb.Article()
            .add_property("word_count", &tb.int())
            .expect("Add word_count to Article");
        tb.Article()
            .add_property("published", &tb.bool())
            .expect("Add published to Article");

        // Add new category value
        tb.Category()
            .add_value("Business")
            .expect("Add Business category");

        let result = B.CreateArticle.with_type_builder(&tb).call(
            "A 500-word published article about tech startups by John Doe, a tech journalist",
        );

        let article = result.expect("LLM call should succeed");

        // Verify static fields
        assert!(!article.title.is_empty(), "Title should not be empty");

        // Verify nested Person has static fields
        assert!(!article.author.name.is_empty(), "Author name should exist");

        // Verify dynamic fields on Article
        assert!(article.has("word_count"), "Article should have word_count");
        let word_count: i64 = article.get("word_count").expect("Get word_count");
        println!("Word count: {}", word_count);

        assert!(article.has("published"), "Article should have published");
        let published: bool = article.get("published").expect("Get published");
        println!("Published: {}", published);

        // Verify dynamic field on nested Person
        assert!(article.author.has("bio"), "Author should have bio");
        let bio: String = article.author.get("bio").expect("Get bio");
        println!("Author bio: {}", bio);

        println!(
            "Article: {} by {} (category: {})",
            article.title, article.author.name, article.category
        );
    }

    #[test]
    fn test_complex_dynamic_types_e2e() {
        let tb = TypeBuilder::new();

        // Add list of strings
        let string_list = tb.list(&tb.string());
        tb.Person()
            .add_property("skills", &string_list)
            .expect("Add skills list");

        // Add optional string
        let optional_string = tb.optional(&tb.string());
        tb.Person()
            .add_property("nickname", &optional_string)
            .expect("Add optional nickname")
            .set_description("The person's nickname (if explicitly provided)")
            .expect("Set nickname description");

        let result = B
            .GetPerson
            .with_type_builder(&tb)
            .call("Alice Johnson, 28, skills: Rust, Python, Go. Nickname: AJ");

        let person = result.expect("LLM call should succeed");

        println!("Person: {} (age {})", person.name, person.age);

        // Check if skills were populated (as a list)
        assert!(person.has("skills"), "Person should have skills");
        let skills: Vec<String> = person.get("skills").expect("Get skills");
        assert_eq!(skills.len(), 3, "Skills should have 3 items");

        // Check optional nickname should be empty
        assert!(person.has("nickname"), "Person should have nickname");
        let nickname: Option<String> = person.get("nickname").expect("Get nickname");
        assert!(nickname.is_some(), "Nickname should be present");
        assert_eq!(nickname.unwrap(), "AJ", "Nickname should be AJ");
    }

    #[test]
    fn test_type_builder_with_set_alias_e2e() {
        let tb = TypeBuilder::new();

        // Add a category with an alias (helps LLM map different terms)
        let ai_value = tb.Category().add_value("AI").expect("Add AI value");
        ai_value
            .set_alias("Artificial Intelligence")
            .expect("Set AI alias");
        ai_value
            .set_description("Artificial intelligence and machine learning")
            .expect("Set AI description");

        let result = B
            .ClassifyArticle
            .with_type_builder(&tb)
            .call("GPT-5 achieves human-level reasoning in new benchmarks, researchers claim");

        let category = result.expect("LLM call should succeed");
        let category_str = category.to_string();

        println!("Category for AI article: {}", category_str);

        // Should be AI or Technology
        assert_eq!(
            category_str, "AI",
            "Expected AI-related category, got: {}",
            category_str
        );
    }

    #[test]
    fn test_fully_dynamic_class_e2e() {
        let tb = TypeBuilder::new();

        // Create a completely new class at runtime
        let product_class = tb.add_class("Product").expect("Create Product class");
        product_class
            .add_property("name", &tb.string())
            .expect("Add name");
        product_class
            .add_property("price", &tb.float())
            .expect("Add price");
        product_class
            .add_property("in_stock", &tb.bool())
            .expect("Add in_stock");

        // Note: We can't directly call a function that returns Product
        // because it's not in the schema. But we can verify the type is registered.
        let product_type = product_class.as_type().expect("Get Product type");
        println!("Created dynamic Product type: {}", product_type.print());

        // The type exists in the TypeBuilder
        assert!(tb.get_class("Product").is_some());
    }

    // Non-LLM tests for compile-time verification
    #[test]
    fn test_function_options_with_type_builder() {
        use baml_client::FunctionOptions;

        let tb = TypeBuilder::new();

        // Add a dynamic property
        tb.Person()
            .add_property("occupation", &tb.string())
            .expect("Add occupation");

        // Create FunctionOptions with TypeBuilder
        let options = FunctionOptions::new().with_type_builder(&tb);

        // This is a compile-time check - if this compiles, the integration works
        let _ = options;
    }

    #[test]
    fn test_function_builder_with_type_builder() {
        let tb = TypeBuilder::new();

        // Add dynamic properties
        tb.Person()
            .add_property("occupation", &tb.string())
            .expect("Add occupation");

        // Verify the builder pattern works
        let function_with_tb = B.GetPerson.with_type_builder(&tb);

        // Can chain with other options
        let function_with_all = B
            .GetPerson
            .with_type_builder(&tb)
            .with_tag("test", "true")
            .with_env_var("DEBUG", "1");

        // These compile and are ready to call (if we had an API key)
        let _ = function_with_tb;
        let _ = function_with_all;
    }
}
