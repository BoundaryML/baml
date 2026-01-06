// Test file for generated BAML client
// This will be compiled against the generated baml_client module

mod baml_client;

use baml_client::sync_client::B;

fn main() {
    println!("Test - baml_client module loaded successfully!");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_top_level_string() {
        let result = B.TestTopLevelString.call("test string").expect("Failed to call TestTopLevelString");
        assert_eq!(result, "Hello from BAML!", "Expected 'Hello from BAML!', got '{}'", result);
    }

    #[test]
    fn test_top_level_int() {
        let result = B.TestTopLevelInt.call("test int").expect("Failed to call TestTopLevelInt");
        assert_eq!(result, 42, "Expected 42, got {}", result);
    }

    #[test]
    fn test_top_level_float() {
        let result = B.TestTopLevelFloat.call("test float").expect("Failed to call TestTopLevelFloat");
        assert!(result >= 3.14 && result <= 3.15, "Expected ~3.14159, got {}", result);
    }

    #[test]
    fn test_top_level_bool() {
        let result = B.TestTopLevelBool.call("test bool").expect("Failed to call TestTopLevelBool");
        assert!(result, "Expected true, got false");
    }

    #[test]
    fn test_primitive_types() {
        let result = B.TestPrimitiveTypes.call("test input").expect("Failed to call TestPrimitiveTypes");

        assert_eq!(result.stringField, "Hello, BAML!", "Expected stringField to be 'Hello, BAML!', got '{}'", result.stringField);
        assert_eq!(result.intField, 42, "Expected intField to be 42, got {}", result.intField);
        assert!(result.floatField >= 3.14 && result.floatField <= 3.15, "Expected floatField to be ~3.14159, got {}", result.floatField);
        assert!(result.boolField, "Expected boolField to be true, got false");
        // nullField is () in Rust, so we just verify it exists
        assert_eq!(result.nullField, (), "Expected nullField to be ()");
    }

    #[test]
    fn test_primitive_arrays() {
        let result = B.TestPrimitiveArrays.call("test arrays").expect("Failed to call TestPrimitiveArrays");

        assert_eq!(result.stringArray.len(), 3, "Expected stringArray length 3, got {}", result.stringArray.len());
        assert_eq!(result.intArray.len(), 5, "Expected intArray length 5, got {}", result.intArray.len());
        assert_eq!(result.floatArray.len(), 4, "Expected floatArray length 4, got {}", result.floatArray.len());
        assert_eq!(result.boolArray.len(), 4, "Expected boolArray length 4, got {}", result.boolArray.len());
    }

    #[test]
    fn test_primitive_maps() {
        let result = B.TestPrimitiveMaps.call("test maps").expect("Failed to call TestPrimitiveMaps");

        assert_eq!(result.stringMap.len(), 2, "Expected stringMap length 2, got {}", result.stringMap.len());
        assert_eq!(result.intMap.len(), 3, "Expected intMap length 3, got {}", result.intMap.len());
        assert_eq!(result.floatMap.len(), 2, "Expected floatMap length 2, got {}", result.floatMap.len());
        assert_eq!(result.boolMap.len(), 2, "Expected boolMap length 2, got {}", result.boolMap.len());
    }

    #[test]
    fn test_mixed_primitives() {
        let result = B.TestMixedPrimitives.call("test mixed").expect("Failed to call TestMixedPrimitives");

        assert!(!result.name.is_empty(), "Expected name to be non-empty");
        assert!(result.age > 0, "Expected age to be positive, got {}", result.age);
    }

    #[test]
    fn test_empty_collections() {
        let result = B.TestEmptyCollections.call("test empty").expect("Failed to call TestEmptyCollections");

        assert_eq!(result.stringArray.len(), 0, "Expected empty stringArray, got length {}", result.stringArray.len());
        assert_eq!(result.intArray.len(), 0, "Expected empty intArray, got length {}", result.intArray.len());
        assert_eq!(result.floatArray.len(), 0, "Expected empty floatArray, got length {}", result.floatArray.len());
        assert_eq!(result.boolArray.len(), 0, "Expected empty boolArray, got length {}", result.boolArray.len());
    }
}
