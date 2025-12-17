/// Macros for testing Go type serialization.
///
/// These macros make it easy to write tests that verify BAML types
/// convert correctly to Go streaming and non-streaming type strings.
///
/// Test Go type serialization with auto-generated test names.
///
/// # Examples
///
/// ```ignore
/// // Test a class field
/// test_go_type!(
///     r#"class Foo { bar string }"#,
///     "Foo.bar",
///     "string",
///     "*string"
/// );
/// ```
#[macro_export]
macro_rules! test_go_type {
    // Class field: "Class.field" with non-streaming and streaming expectations
    // With line number from type_serialization_tests.md
    (
        $baml:expr,
        $class_dot_field:expr,
        $line_number:expr,
        $expected_non_streaming:expr,
        $expected_streaming:expr
    ) => {{
        use internal_baml_core::ir::{repr::make_test_ir, IRHelper};
        use $crate::ir_to_go::classes::{ir_class_to_go, ir_class_to_go_stream};
        use $crate::package::CurrentRenderPackage;
        use $crate::r#type::SerializeType;

        let path = $class_dot_field;
        let line_num: usize = $line_number;
        let parts: Vec<&str> = path.split('.').collect();

        let ir = make_test_ir($baml).expect("Valid BAML");
        let ir = std::sync::Arc::new(ir);
        let pkg = CurrentRenderPackage::new("baml_client", ir.clone());

        if parts.len() == 2 {
            // Class.field case
            let class_name = parts[0];
            let field_name = parts[1];

            let class = ir
                .find_class(class_name)
                .expect(&format!("Class '{}' not found", class_name))
                .item;

            // Test non-streaming
            pkg.set("baml_client.types");
            let class_go = ir_class_to_go(class, &pkg);
            let field = class_go
                .fields
                .iter()
                .find(|f| f.name == field_name)
                .expect(&format!(
                    "Field '{}' not found in class '{}'",
                    field_name, class_name
                ));
            assert_eq!(
                field.r#type.serialize_type(&pkg),
                $expected_non_streaming,
                "Non-streaming type mismatch for {} (type_serialization_tests.md:{})",
                path,
                line_num
            );

            // Test streaming
            pkg.set("baml_client.stream_types");
            let class_go_stream = ir_class_to_go_stream(class, &pkg);
            let field = class_go_stream
                .fields
                .iter()
                .find(|f| f.name == field_name)
                .expect(&format!(
                    "Field '{}' not found in streaming class '{}'",
                    field_name, class_name
                ));
            assert_eq!(
                field.r#type.serialize_type(&pkg),
                $expected_streaming,
                "Streaming type mismatch for {} (type_serialization_tests.md:{})",
                path,
                line_num
            );
        } else if parts.len() == 1 {
            // Type alias case (no dot)
            use $crate::ir_to_go::type_aliases::{ir_type_alias_to_go, ir_type_alias_to_go_stream};

            let alias_name = parts[0];
            let type_alias = ir
                .find_type_alias(alias_name)
                .expect(&format!("Type alias '{}' not found", alias_name))
                .item;

            // Non-streaming
            pkg.set("baml_client.types");
            let alias_go = ir_type_alias_to_go(type_alias, &pkg, None);
            assert_eq!(
                alias_go.type_.serialize_type(&pkg),
                $expected_non_streaming,
                "Non-streaming type alias mismatch for {} (type_serialization_tests.md:{})",
                alias_name,
                line_num
            );

            // Streaming
            pkg.set("baml_client.stream_types");
            let alias_go_stream = ir_type_alias_to_go_stream(type_alias, &pkg, None);
            assert_eq!(
                alias_go_stream.type_.serialize_type(&pkg),
                $expected_streaming,
                "Streaming type alias mismatch for {} (type_serialization_tests.md:{})",
                alias_name,
                line_num
            );
        } else {
            panic!(
                "Invalid path format: {}. Use 'Class.field' or 'TypeAlias' (type_serialization_tests.md:{})",
                path,
                line_num
            );
        }
    }};

    // Enum case: just enum name and list of values
    // With line number from type_serialization_tests.md
    (
        $baml:expr,
        $enum_name:expr,
        $line_number:expr,
        [$( $value:expr ),* $(,)?]
    ) => {{
        use internal_baml_core::ir::{repr::make_test_ir, IRHelper};
        use $crate::ir_to_go::enums::ir_enum_to_go;
        use $crate::package::CurrentRenderPackage;

        let ir = make_test_ir($baml).expect("Valid BAML");
        let ir = std::sync::Arc::new(ir);
        let pkg = CurrentRenderPackage::new("baml_client", ir.clone());
        let line_num: usize = $line_number;

        let enm = ir
            .find_enum($enum_name)
            .expect(&format!("Enum '{}' not found", $enum_name))
            .item;

        let enum_go = ir_enum_to_go(enm, &pkg);
        assert_eq!(enum_go.name, $enum_name);

        let expected_values: Vec<&str> = vec![$( $value ),*];
        let actual_values: Vec<&str> = enum_go.values.iter().map(|(v, _)| v.as_str()).collect();
        assert_eq!(
            actual_values,
            expected_values,
            "Enum values mismatch for {} (type_serialization_tests.md:{})",
            $enum_name,
            line_num
        );
    }};
}

/// Run multiple type tests in a single test function.
#[macro_export]
macro_rules! test_go_types {
    // Multiple class/alias tests
    ( $( ( $baml:expr, $path:expr, $non_streaming:expr, $streaming:expr ) ),* $(,)? ) => {
        $(
            $crate::test_go_type!($baml, $path, $non_streaming, $streaming);
        )*
    };
}

// Include auto-generated tests from type_serialization_tests.md
// Each test case becomes its own test function under `type_gen` module
include!(concat!(env!("OUT_DIR"), "/generated_type_tests.rs"));
