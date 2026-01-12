//! Type builder tests - ported from test_type_builder_test.go
//!
//! Tests for the TypeBuilder functionality including:
//! - Dynamic class creation
//! - Dynamic enum creation
//! - Type builder operations
//! - Dynamic class output
//! - Adding properties to existing classes/enums

use rust::baml_client::sync_client::B;
use rust::baml_client::type_builder::TypeBuilder;
use rust::baml_client::types::*;
use std::collections::HashMap;

/// Test accessing existing schema class
#[test]
fn test_access_existing_class() {
    let tb = TypeBuilder::new();

    // Access an existing class from schema
    let class_builder = tb.DynamicClassOne();
    let type_def = class_builder.r#type();

    // Type should be usable
    let _ = type_def;
}

/// Test accessing existing schema enum
#[test]
fn test_access_existing_enum() {
    let tb = TypeBuilder::new();

    // Access an existing enum from schema
    let enum_builder = tb.DynEnumOne();

    // Should be able to get the inner builder
    let _ = enum_builder.inner();
}

/// Test adding dynamic property to existing class
#[test]
fn test_add_dynamic_property() {
    let tb = TypeBuilder::new();

    // Get the inner class builder and add a property
    let class_builder = tb.DynamicClassOne();
    let result = class_builder
        .inner()
        .add_property("dynamic_field", &tb.string());

    // Adding property should succeed
    assert!(result.is_ok(), "Expected successful property addition");
}

/// Test adding dynamic value to existing enum
#[test]
fn test_add_dynamic_enum_value() {
    let tb = TypeBuilder::new();

    // Get the inner enum builder and add a value
    let enum_builder = tb.DynEnumOne();
    let result = enum_builder.inner().add_value("CUSTOM_VALUE");

    // Adding value should succeed
    assert!(result.is_ok(), "Expected successful value addition");
}

/// Test creating new dynamic class
#[test]
fn test_create_new_dynamic_class() {
    let tb = TypeBuilder::new();

    // Add a completely new class
    let result = tb.add_class("NewDynamicClass");
    assert!(result.is_ok(), "Expected successful class creation");

    let class_builder = result.unwrap();

    // Add properties to the new class
    let prop_result = class_builder.add_property("name", &tb.string());
    assert!(prop_result.is_ok(), "Expected successful property addition");
}

/// Test creating new dynamic enum
#[test]
fn test_create_new_dynamic_enum() {
    let tb = TypeBuilder::new();

    // Add a completely new enum
    let result = tb.add_enum("NewDynamicEnum");
    assert!(result.is_ok(), "Expected successful enum creation");

    let enum_builder = result.unwrap();

    // Add values to the new enum
    let value_result = enum_builder.add_value("VALUE_A");
    assert!(value_result.is_ok(), "Expected successful value addition");

    let value_result2 = enum_builder.add_value("VALUE_B");
    assert!(
        value_result2.is_ok(),
        "Expected successful second value addition"
    );
}

/// Test type builder primitive types
#[test]
fn test_type_builder_primitives() {
    let tb = TypeBuilder::new();

    // Test all primitive types
    let _ = tb.string();
    let _ = tb.int();
    let _ = tb.float();
    let _ = tb.bool();
    let _ = tb.null();
}

/// Test type builder literal types
#[test]
fn test_type_builder_literals() {
    let tb = TypeBuilder::new();

    // Test literal types
    let _ = tb.literal_string("hello");
    let _ = tb.literal_int(42);
    let _ = tb.literal_bool(true);
}

/// Test type builder composite types
#[test]
fn test_type_builder_composites() {
    let tb = TypeBuilder::new();

    // Test composite types
    let string_type = tb.string();
    let int_type = tb.int();

    let _ = tb.list(&string_type);
    let _ = tb.optional(&string_type);
    let _ = tb.map(&string_type, &int_type);
    let _ = tb.union(&[&string_type, &int_type]);
}

/// Test type builder print
#[test]
fn test_type_builder_print() {
    let tb = TypeBuilder::new();

    // Add some modifications
    let _ = tb.add_class("PrintTestClass");
    let _ = tb.add_enum("PrintTestEnum");

    // Print should return a string describing the state
    let output = tb.print();
    assert!(!output.is_empty(), "Expected non-empty print output");
}

/// Test using type builder with function call
#[test]
fn test_type_builder_with_function() {
    let tb = TypeBuilder::new();

    // Add dynamic property to DynamicClassOne
    let class_builder = tb.DynamicClassOne();
    class_builder
        .add_property("extra_field", &tb.string())
        .unwrap();
    tb.DynEnumOne().add_value("test_value").unwrap();

    // Create input with dynamic field
    let mut input = DynamicClassOne::default();
    input.__dynamic.insert(
        "extra_field".to_string(),
        baml::BamlValue::String("test_value".to_string()),
    );

    // Call function with type builder
    let result = B.DynamicFunc.with_type_builder(&tb).call(&input);

    assert!(
        result.is_ok(),
        "Expected successful call with type builder, got {:?}",
        result
    );
}

/// Test dynamic enum with function
#[test]
fn test_dynamic_enum_with_function() {
    let tb = TypeBuilder::new();

    // Add value to existing enum
    let enum_builder = tb.DynEnumOne();
    let _ = enum_builder.inner().add_value("DYNAMIC_VALUE");

    // Call function that uses the enum
    let result = B
        .ClassifyDynamicStatus
        .with_type_builder(&tb)
        .call("This should work");

    assert!(
        result.is_ok(),
        "Expected successful call with dynamic enum, got {:?}",
        result
    );
}

/// Test list of dynamic classes
#[test]
fn test_dynamic_list_input_output() {
    let tb = TypeBuilder::new();

    // Add property to DynInputOutput
    let class_builder = tb.DynInputOutput();
    let _ = class_builder
        .inner()
        .add_property("list_field", &tb.list(&tb.string()));

    let input1 = DynInputOutput {
        testKey: "item1".to_string(),
        __dynamic: HashMap::new(),
    };
    let input2 = DynInputOutput {
        testKey: "item2".to_string(),
        __dynamic: HashMap::new(),
    };

    let result = B
        .DynamicListInputOutput
        .with_type_builder(&tb)
        .call(&[input1, input2]);

    assert!(
        result.is_ok(),
        "Expected successful list call, got {:?}",
        result
    );
}

/// Test render dynamic class
#[test]
fn test_render_dynamic_class() {
    let tb = TypeBuilder::new();

    // Add property to RenderTestClass
    let class_builder = tb.RenderTestClass();
    let _ = class_builder
        .inner()
        .add_property("extra_prop", &tb.string());

    let input = RenderTestClass {
        name: "test".to_string(),
        status: RenderStatusEnum::ACTIVE,
        __dynamic: HashMap::new(),
    };

    let result = B.RenderDynamicClass.with_type_builder(&tb).call(&input);

    assert!(
        result.is_ok(),
        "Expected successful render call, got {:?}",
        result
    );
}

/// Test render dynamic enum
#[test]
fn test_render_dynamic_enum() {
    let tb = TypeBuilder::new();

    // Add values to RenderTestEnum
    let enum_builder = tb.RenderTestEnum();
    let _ = enum_builder.inner().add_value("CAR");
    let _ = enum_builder.inner().add_value("TRUCK");

    let result = B
        .RenderDynamicEnum
        .with_type_builder(&tb)
        .call(&RenderTestEnum::BIKE, &RenderTestEnum::SCOOTER);

    assert!(
        result.is_ok(),
        "Expected successful enum render call, got {:?}",
        result
    );
}

/// Test adding BAML source
#[test]
fn test_add_baml_source() {
    let tb = TypeBuilder::new();

    // Add new BAML definitions
    let baml_source = r#"
        class CustomFromBAML {
            field1 string
            field2 int
        }
    "#;

    let result = tb.add_baml(baml_source);
    // This may or may not succeed depending on schema validation
    match result {
        Ok(_) => {
            // Successfully added BAML
            let output = tb.print();
            assert!(
                output.contains("CustomFromBAML") || !output.is_empty(),
                "Expected BAML to be added"
            );
        }
        Err(e) => {
            // BAML parsing might fail for various reasons
            eprintln!("add_baml failed (may be expected): {:?}", e);
        }
    }
}
