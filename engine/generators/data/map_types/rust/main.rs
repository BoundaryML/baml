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
    fn test_simple_maps() {
        let result = B.TestSimpleMaps.call("test simple maps").expect("Failed to call TestSimpleMaps");

        // Verify simple map contents
        assert_eq!(result.stringToString.len(), 2, "Expected stringToString length 2, got {}", result.stringToString.len());
        assert_eq!(result.stringToString.get("key1"), Some(&"value1".to_string()), "Expected stringToString['key1'] to be 'value1'");

        assert_eq!(result.stringToInt.len(), 3, "Expected stringToInt length 3, got {}", result.stringToInt.len());
        assert_eq!(result.stringToInt.get("one"), Some(&1), "Expected stringToInt['one'] to be 1");

        assert_eq!(result.stringToFloat.len(), 2, "Expected stringToFloat length 2, got {}", result.stringToFloat.len());
        let pi = result.stringToFloat.get("pi").expect("Expected 'pi' key in stringToFloat");
        assert!((pi - 3.14159).abs() < 0.0001, "Expected stringToFloat['pi'] to be ~3.14159, got {}", pi);

        assert_eq!(result.stringToBool.len(), 2, "Expected stringToBool length 2, got {}", result.stringToBool.len());
        assert_eq!(result.stringToBool.get("isTrue"), Some(&true), "Expected stringToBool['isTrue'] to be true");

        assert_eq!(result.intToString.len(), 3, "Expected intToString length 3, got {}", result.intToString.len());
        assert_eq!(result.intToString.get("1"), Some(&"one".to_string()), "Expected intToString['1'] to be 'one'");
    }

    #[test]
    fn test_complex_maps() {
        let result = B.TestComplexMaps.call("test complex maps").expect("Failed to call TestComplexMaps");

        // Verify complex map contents
        assert!(result.userMap.len() >= 2, "Expected at least 2 users in userMap, got {}", result.userMap.len());
        for (key, user) in &result.userMap {
            assert!(!user.name.is_empty(), "User '{}' has empty name", key);
            assert!(!user.email.is_empty(), "User '{}' has empty email", key);
        }

        assert!(result.productMap.len() >= 3, "Expected at least 3 products in productMap, got {}", result.productMap.len());
        for (key, product) in &result.productMap {
            assert!(!product.name.is_empty(), "Product {} has empty name", key);
            assert!(product.price > 0.0, "Product {} has invalid price: {}", key, product.price);
        }

        assert!(result.nestedMap.len() >= 1, "Expected at least 1 entry in nestedMap, got {}", result.nestedMap.len());

        assert_eq!(result.arrayMap.len(), 2, "Expected arrayMap length 2, got {}", result.arrayMap.len());

        assert!(result.mapArray.len() >= 2, "Expected at least 2 maps in mapArray, got {}", result.mapArray.len());
    }

    #[test]
    fn test_nested_maps() {
        let result = B.TestNestedMaps.call("test nested maps").expect("Failed to call TestNestedMaps");

        // Verify nested map structure
        assert!(result.simple.len() >= 2, "Expected at least 2 entries in simple map, got {}", result.simple.len());

        assert!(result.oneLevelNested.len() >= 2, "Expected at least 2 entries in oneLevelNested, got {}", result.oneLevelNested.len());
        for (key, inner_map) in &result.oneLevelNested {
            assert!(inner_map.len() >= 2, "Expected at least 2 entries in oneLevelNested['{}'], got {}", key, inner_map.len());
        }

        assert!(result.twoLevelNested.len() >= 2, "Expected at least 2 entries in twoLevelNested, got {}", result.twoLevelNested.len());

        assert!(result.mapOfArrays.len() >= 2, "Expected at least 2 entries in mapOfArrays, got {}", result.mapOfArrays.len());

        assert!(result.mapOfMaps.len() >= 2, "Expected at least 2 entries in mapOfMaps, got {}", result.mapOfMaps.len());
    }

    #[test]
    fn test_edge_case_maps() {
        let result = B.TestEdgeCaseMaps.call("test edge case maps").expect("Failed to call TestEdgeCaseMaps");

        // Verify edge case map contents
        assert_eq!(result.emptyMap.len(), 0, "Expected emptyMap to be empty, got length {}", result.emptyMap.len());

        assert_eq!(result.nullableValues.len(), 2, "Expected nullableValues length 2, got {}", result.nullableValues.len());
        let present = result.nullableValues.get("present").expect("Expected 'present' key");
        assert_eq!(present.as_deref(), Some("value"), "Expected nullableValues['present'] to be 'value'");

        assert_eq!(result.unionValues.len(), 3, "Expected unionValues length 3, got {}", result.unionValues.len());
    }

    #[test]
    fn test_large_maps() {
        let result = B.TestLargeMaps.call("test large maps").expect("Failed to call TestLargeMaps");

        // Verify large map sizes (LLMs are fuzzy, so we may get a few less)
        assert!(result.stringToString.len() >= 15, "Expected at least 15 entries in stringToString, got {}", result.stringToString.len());
        assert!(result.stringToInt.len() >= 15, "Expected at least 15 entries in stringToInt, got {}", result.stringToInt.len());
        assert!(result.stringToFloat.len() >= 15, "Expected at least 15 entries in stringToFloat, got {}", result.stringToFloat.len());
        assert!(result.stringToBool.len() >= 15, "Expected at least 15 entries in stringToBool, got {}", result.stringToBool.len());
        assert!(result.intToString.len() >= 15, "Expected at least 15 entries in intToString, got {}", result.intToString.len());
    }

    // Test top-level map return types
    #[test]
    fn test_top_level_string_map() {
        let result = B.TestTopLevelStringMap.call("test string map").expect("Failed to call TestTopLevelStringMap");

        assert_eq!(result.len(), 3, "Expected 3 entries in string map, got {}", result.len());
        assert_eq!(result.get("first"), Some(&"Hello".to_string()), "Unexpected value for 'first'");
        assert_eq!(result.get("second"), Some(&"World".to_string()), "Unexpected value for 'second'");
        assert_eq!(result.get("third"), Some(&"BAML".to_string()), "Unexpected value for 'third'");
    }

    #[test]
    fn test_top_level_int_map() {
        let result = B.TestTopLevelIntMap.call("test int map").expect("Failed to call TestTopLevelIntMap");

        assert_eq!(result.len(), 4, "Expected 4 entries in int map, got {}", result.len());
        assert_eq!(result.get("one"), Some(&1), "Unexpected value for 'one'");
        assert_eq!(result.get("two"), Some(&2), "Unexpected value for 'two'");
        assert_eq!(result.get("ten"), Some(&10), "Unexpected value for 'ten'");
        assert_eq!(result.get("hundred"), Some(&100), "Unexpected value for 'hundred'");
    }

    #[test]
    fn test_top_level_float_map() {
        let result = B.TestTopLevelFloatMap.call("test float map").expect("Failed to call TestTopLevelFloatMap");

        assert_eq!(result.len(), 3, "Expected 3 entries in float map, got {}", result.len());
        let pi = result.get("pi").expect("Expected 'pi' key");
        let e = result.get("e").expect("Expected 'e' key");
        assert!((pi - 3.14159).abs() < 0.0001, "Unexpected value for 'pi': {}", pi);
        assert!((e - 2.71828).abs() < 0.0001, "Unexpected value for 'e': {}", e);
    }

    #[test]
    fn test_top_level_bool_map() {
        let result = B.TestTopLevelBoolMap.call("test bool map").expect("Failed to call TestTopLevelBoolMap");

        assert_eq!(result.len(), 3, "Expected 3 entries in bool map, got {}", result.len());
        assert_eq!(result.get("isActive"), Some(&true), "Unexpected value for 'isActive'");
        assert_eq!(result.get("isDisabled"), Some(&false), "Unexpected value for 'isDisabled'");
        assert_eq!(result.get("isEnabled"), Some(&true), "Unexpected value for 'isEnabled'");
    }

    #[test]
    fn test_top_level_nested_map() {
        let result = B.TestTopLevelNestedMap.call("test nested map").expect("Failed to call TestTopLevelNestedMap");

        assert_eq!(result.len(), 2, "Expected 2 entries in nested map, got {}", result.len());
        let users = result.get("users").expect("Expected 'users' key");
        let roles = result.get("roles").expect("Expected 'roles' key");
        assert_eq!(users.len(), 2, "Unexpected length for 'users': {}", users.len());
        assert_eq!(roles.len(), 2, "Unexpected length for 'roles': {}", roles.len());
    }

    #[test]
    fn test_top_level_map_of_arrays() {
        let result = B.TestTopLevelMapOfArrays.call("test map of arrays").expect("Failed to call TestTopLevelMapOfArrays");

        assert_eq!(result.len(), 3, "Expected 3 entries in map of arrays, got {}", result.len());
        let evens = result.get("evens").expect("Expected 'evens' key");
        let odds = result.get("odds").expect("Expected 'odds' key");
        let primes = result.get("primes").expect("Expected 'primes' key");
        assert_eq!(evens.len(), 4, "Unexpected length for 'evens': {}", evens.len());
        assert_eq!(odds.len(), 4, "Unexpected length for 'odds': {}", odds.len());
        assert_eq!(primes.len(), 5, "Unexpected length for 'primes': {}", primes.len());
    }

    #[test]
    fn test_top_level_empty_map() {
        let result = B.TestTopLevelEmptyMap.call("test empty map").expect("Failed to call TestTopLevelEmptyMap");

        assert_eq!(result.len(), 0, "Expected empty map, got {} entries", result.len());
    }

    #[test]
    fn test_top_level_map_with_nullable() {
        let result = B.TestTopLevelMapWithNullable.call("use just a json map").expect("Failed to call TestTopLevelMapWithNullable");

        assert_eq!(result.len(), 3, "Expected 3 entries in nullable map, got {}", result.len());
        let present = result.get("present").expect("Expected 'present' key");
        assert_eq!(present.as_deref(), Some("value"), "Expected 'present' to have value 'value'");
        let absent = result.get("absent").expect("Expected 'absent' key");
        assert!(absent.is_none(), "Expected 'absent' to be None");
    }

    #[test]
    fn test_top_level_map_of_objects() {
        let result = B.TestTopLevelMapOfObjects.call("test object map").expect("Failed to call TestTopLevelMapOfObjects");

        assert_eq!(result.len(), 2, "Expected 2 entries in object map, got {}", result.len());
        for (key, user) in &result {
            assert!(!user.name.is_empty(), "User {} has empty name", key);
            assert!(!user.email.is_empty(), "User {} has empty email", key);
        }
    }
}
