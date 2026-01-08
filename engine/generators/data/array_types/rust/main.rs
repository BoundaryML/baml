// Test file for generated BAML client
// This will be compiled against the generated baml_client module

mod baml_client;

use baml_client::sync_client::B;
use baml_client::types::*;

fn main() {
    println!("Test - baml_client module loaded successfully!");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_arrays() {
        let result = B.TestSimpleArrays.call("test simple arrays")
            .expect("Failed to call TestSimpleArrays");

        // Verify simple array contents
        assert_eq!(result.strings.len(), 3, "Expected strings length 3, got {}", result.strings.len());
        assert_eq!(result.integers.len(), 5, "Expected integers length 5, got {}", result.integers.len());
        assert_eq!(result.floats.len(), 3, "Expected floats length 3, got {}", result.floats.len());
        assert_eq!(result.booleans.len(), 4, "Expected booleans length 4, got {}", result.booleans.len());
    }

    #[test]
    fn test_nested_arrays() {
        let result = B.TestNestedArrays.call("test nested arrays")
            .expect("Failed to call TestNestedArrays");

        // Verify nested array structure
        assert_eq!(result.matrix.len(), 3, "Expected matrix length 3, got {}", result.matrix.len());
        assert_eq!(result.matrix[0].len(), 3, "Expected matrix[0] length 3, got {}", result.matrix[0].len());
        assert_eq!(result.stringMatrix.len(), 2, "Expected stringMatrix length 2, got {}", result.stringMatrix.len());
        assert_eq!(result.threeDimensional.len(), 2, "Expected threeDimensional length 2, got {}", result.threeDimensional.len());
    }

    #[test]
    fn test_object_arrays() {
        let result = B.TestObjectArrays.call("test object arrays")
            .expect("Failed to call TestObjectArrays");

        // Verify object array contents
        assert!(result.users.len() >= 3, "Expected at least 3 users, got {}", result.users.len());
        assert!(result.products.len() >= 2, "Expected at least 2 products, got {}", result.products.len());
        assert!(result.tags.len() >= 4, "Expected at least 4 tags, got {}", result.tags.len());

        // Verify user objects have required fields
        for (i, user) in result.users.iter().enumerate() {
            assert!(user.id > 0, "User {} has invalid id: {}", i, user.id);
            assert!(!user.name.is_empty(), "User {} has empty name", i);
            assert!(!user.email.is_empty(), "User {} has empty email", i);
        }
    }

    #[test]
    fn test_mixed_arrays() {
        let result = B.TestMixedArrays.call("test mixed arrays")
            .expect("Failed to call TestMixedArrays");

        // Verify mixed array contents
        assert_eq!(result.primitiveArray.len(), 4, "Expected primitiveArray length 4, got {}", result.primitiveArray.len());
        assert_eq!(result.nullableArray.len(), 4, "Expected nullableArray length 4, got {}", result.nullableArray.len());
        assert!(result.optionalItems.len() >= 2, "Expected at least 2 optionalItems, got {}", result.optionalItems.len());
        assert!(result.arrayOfArrays.len() >= 2, "Expected at least 2 arrayOfArrays, got {}", result.arrayOfArrays.len());
        assert!(result.complexMixed.len() >= 2, "Expected at least 2 complexMixed items, got {}", result.complexMixed.len());
    }

    #[test]
    fn test_empty_arrays() {
        let result = B.TestEmptyArrays.call("test empty arrays")
            .expect("Failed to call TestEmptyArrays");

        // Verify all arrays are empty
        assert_eq!(result.strings.len(), 0, "Expected empty strings array, got length {}", result.strings.len());
        assert_eq!(result.integers.len(), 0, "Expected empty integers array, got length {}", result.integers.len());
        assert_eq!(result.floats.len(), 0, "Expected empty floats array, got length {}", result.floats.len());
        assert_eq!(result.booleans.len(), 0, "Expected empty booleans array, got length {}", result.booleans.len());
    }

    #[test]
    fn test_large_arrays() {
        let result = B.TestLargeArrays.call("test large arrays")
            .expect("Failed to call TestLargeArrays");

        // Verify large array sizes
        assert!(result.strings.len() >= 40, "Expected at least 40 strings, got {}", result.strings.len());
        assert!(result.integers.len() >= 50, "Expected at least 50 integers, got {}", result.integers.len());
        assert!(result.floats.len() >= 20, "Expected at least 20 floats, got {}", result.floats.len());
        assert!(result.booleans.len() >= 15, "Expected at least 15 booleans, got {}", result.booleans.len());
    }

    // Test top-level array return types

    #[test]
    fn test_top_level_string_array() {
        let result = B.TestTopLevelStringArray.call("test string array")
            .expect("Failed to call TestTopLevelStringArray");

        assert_eq!(result.len(), 4, "Expected 4 strings, got {}", result.len());
        assert_eq!(result[0], "apple", "Expected first element to be 'apple'");
        assert_eq!(result[1], "banana", "Expected second element to be 'banana'");
        assert_eq!(result[2], "cherry", "Expected third element to be 'cherry'");
        assert_eq!(result[3], "date", "Expected fourth element to be 'date'");
    }

    #[test]
    fn test_top_level_int_array() {
        let result = B.TestTopLevelIntArray.call("test int array")
            .expect("Failed to call TestTopLevelIntArray");

        assert_eq!(result.len(), 5, "Expected 5 integers, got {}", result.len());
        assert_eq!(result[0], 10, "Expected first element to be 10");
        assert_eq!(result[1], 20, "Expected second element to be 20");
        assert_eq!(result[2], 30, "Expected third element to be 30");
        assert_eq!(result[3], 40, "Expected fourth element to be 40");
        assert_eq!(result[4], 50, "Expected fifth element to be 50");
    }

    #[test]
    fn test_top_level_float_array() {
        let result = B.TestTopLevelFloatArray.call("test float array")
            .expect("Failed to call TestTopLevelFloatArray");

        assert_eq!(result.len(), 4, "Expected 4 floats, got {}", result.len());
        assert!((result[0] - 1.5).abs() < f64::EPSILON, "Expected first element to be 1.5");
        assert!((result[1] - 2.5).abs() < f64::EPSILON, "Expected second element to be 2.5");
        assert!((result[2] - 3.5).abs() < f64::EPSILON, "Expected third element to be 3.5");
        assert!((result[3] - 4.5).abs() < f64::EPSILON, "Expected fourth element to be 4.5");
    }

    #[test]
    fn test_top_level_bool_array() {
        let result = B.TestTopLevelBoolArray.call("test bool array")
            .expect("Failed to call TestTopLevelBoolArray");

        assert_eq!(result.len(), 5, "Expected 5 booleans, got {}", result.len());
        assert!(result[0], "Expected first element to be true");
        assert!(!result[1], "Expected second element to be false");
        assert!(result[2], "Expected third element to be true");
        assert!(!result[3], "Expected fourth element to be false");
        assert!(result[4], "Expected fifth element to be true");
    }

    #[test]
    fn test_top_level_nested_array() {
        let result = B.TestTopLevelNestedArray.call("test nested array")
            .expect("Failed to call TestTopLevelNestedArray");

        assert_eq!(result.len(), 3, "Expected 3 rows, got {}", result.len());
        for (i, row) in result.iter().enumerate() {
            assert_eq!(row.len(), 3, "Expected 3 columns in row {}, got {}", i, row.len());
        }
    }

    #[test]
    fn test_top_level_3d_array() {
        let result = B.TestTopLevel3DArray.call("test 3D array")
            .expect("Failed to call TestTopLevel3DArray");

        assert_eq!(result.len(), 2, "Expected 2 levels, got {}", result.len());
        for (i, level) in result.iter().enumerate() {
            assert_eq!(level.len(), 2, "Expected 2 rows in level {}, got {}", i, level.len());
            for (j, row) in level.iter().enumerate() {
                assert_eq!(row.len(), 2, "Expected 2 columns in level {} row {}, got {}", i, j, row.len());
            }
        }
    }

    #[test]
    fn test_top_level_empty_array() {
        let result = B.TestTopLevelEmptyArray.call("test empty array")
            .expect("Failed to call TestTopLevelEmptyArray");

        assert_eq!(result.len(), 0, "Expected empty array, got {} elements", result.len());
    }

    #[test]
    fn test_top_level_nullable_array() {
        let result = B.TestTopLevelNullableArray.call("test nullable array")
            .expect("Failed to call TestTopLevelNullableArray");

        assert_eq!(result.len(), 5, "Expected 5 elements in nullable array, got {}", result.len());
        assert_eq!(result[0].as_deref(), Some("hello"), "Expected first element to be 'hello'");
        assert!(result[1].is_none(), "Expected second element to be None");
    }

    #[test]
    fn test_top_level_object_array() {
        let result = B.TestTopLevelObjectArray.call("test object array")
            .expect("Failed to call TestTopLevelObjectArray");

        assert_eq!(result.len(), 3, "Expected 3 users, got {}", result.len());
        for (i, user) in result.iter().enumerate() {
            assert!(!user.name.is_empty(), "User {} has empty name", i);
            assert!(!user.email.is_empty(), "User {} has empty email", i);
        }
    }

    #[test]
    fn test_top_level_mixed_array() {
        let result = B.TestTopLevelMixedArray.call("test mixed array")
            .expect("Failed to call TestTopLevelMixedArray");

        assert_eq!(result.len(), 6, "Expected 6 elements in mixed array, got {}", result.len());
    }

    #[test]
    fn test_top_level_array_of_maps() {
        let result = B.TestTopLevelArrayOfMaps.call("test array of maps")
            .expect("Failed to call TestTopLevelArrayOfMaps");

        assert_eq!(result.len(), 3, "Expected 3 maps in array, got {}", result.len());
        assert_eq!(result[0].len(), 2, "Expected 2 entries in first map, got {}", result[0].len());
        assert_eq!(result[1].len(), 2, "Expected 2 entries in second map, got {}", result[1].len());
        assert_eq!(result[2].len(), 2, "Expected 2 entries in third map, got {}", result[2].len());
    }
}
