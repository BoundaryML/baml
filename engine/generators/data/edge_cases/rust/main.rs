// Test file for generated BAML client
// This will be compiled against the generated baml_client module

mod baml_client;

use baml_client::types::*;
use baml_client::sync_client::B;

fn main() {
    println!("Test - baml_client module loaded successfully!");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_collections() {
        let result = B.TestEmptyCollections
            .call("test empty collections")
            .expect("Failed to call TestEmptyCollections");

        // Verify all collections are empty
        assert!(
            result.emptyStringArray.is_empty(),
            "Expected emptyStringArray to be empty, got length {}",
            result.emptyStringArray.len()
        );
        assert!(
            result.emptyIntArray.is_empty(),
            "Expected emptyIntArray to be empty, got length {}",
            result.emptyIntArray.len()
        );
        assert!(
            result.emptyObjectArray.is_empty(),
            "Expected emptyObjectArray to be empty, got length {}",
            result.emptyObjectArray.len()
        );
        assert!(
            result.emptyMap.is_empty(),
            "Expected emptyMap to be empty, got length {}",
            result.emptyMap.len()
        );
        assert!(
            result.emptyNestedArray.is_empty(),
            "Expected emptyNestedArray to be empty, got length {}",
            result.emptyNestedArray.len()
        );
    }

    #[test]
    fn test_large_structure() {
        let result = B.TestLargeStructure
            .call("test large structure")
            .expect("Failed to call TestLargeStructure");

        // Verify large structure has all string fields populated
        let fields = [
            &result.field1,
            &result.field2,
            &result.field3,
            &result.field4,
            &result.field5,
        ];
        for (i, field) in fields.iter().enumerate() {
            assert!(!field.is_empty(), "Expected field{} to be non-empty", i + 1);
        }

        // Verify integer fields are non-zero
        let int_fields = [
            result.field6,
            result.field7,
            result.field8,
            result.field9,
            result.field10,
        ];
        for (i, field) in int_fields.iter().enumerate() {
            assert!(*field != 0, "Expected field{} to be non-zero", i + 6);
        }

        // Verify float fields are non-zero
        let float_fields = [
            result.field11,
            result.field12,
            result.field13,
            result.field14,
            result.field15,
        ];
        for (i, field) in float_fields.iter().enumerate() {
            assert!(*field != 0.0, "Expected field{} to be non-zero", i + 11);
        }

        // Verify arrays have expected sizes (3-5 items)
        assert!(
            result.array1.len() >= 3 && result.array1.len() <= 5,
            "Expected array1 length 3-5, got {}",
            result.array1.len()
        );
        assert!(
            result.array2.len() >= 3 && result.array2.len() <= 5,
            "Expected array2 length 3-5, got {}",
            result.array2.len()
        );
        assert!(
            result.array3.len() >= 3 && result.array3.len() <= 5,
            "Expected array3 length 3-5, got {}",
            result.array3.len()
        );
        assert!(
            result.array4.len() >= 3 && result.array4.len() <= 5,
            "Expected array4 length 3-5, got {}",
            result.array4.len()
        );
        assert!(
            result.array5.len() >= 3 && result.array5.len() <= 5,
            "Expected array5 length 3-5, got {}",
            result.array5.len()
        );

        // Verify maps have expected sizes (2-3 items)
        assert!(
            result.map1.len() >= 2 && result.map1.len() <= 3,
            "Expected map1 length 2-3, got {}",
            result.map1.len()
        );
        assert!(
            result.map2.len() >= 2 && result.map2.len() <= 3,
            "Expected map2 length 2-3, got {}",
            result.map2.len()
        );
        assert!(
            result.map3.len() >= 2 && result.map3.len() <= 3,
            "Expected map3 length 2-3, got {}",
            result.map3.len()
        );
        assert!(
            result.map4.len() >= 2 && result.map4.len() <= 3,
            "Expected map4 length 2-3, got {}",
            result.map4.len()
        );
        assert!(
            result.map5.len() >= 2 && result.map5.len() <= 3,
            "Expected map5 length 2-3, got {}",
            result.map5.len()
        );
    }

    #[test]
    fn test_deep_recursion() {
        let result = B.TestDeepRecursion
            .call(5)
            .expect("Failed to call TestDeepRecursion");

        // Verify recursion depth by traversing the linked list
        let mut current: Option<&DeepRecursion> = Some(&result);
        let mut depth = 0;

        while let Some(node) = current {
            depth += 1;
            assert!(
                !node.value.is_empty(),
                "Expected value at depth {} to be non-empty",
                depth
            );
            current = node.next.as_ref().map(|boxed| boxed.as_ref());
        }

        assert_eq!(depth, 5, "Expected recursion depth 5, got {}", depth);
    }

    #[test]
    fn test_special_characters() {
        let result = B.TestSpecialCharacters
            .call("test special characters")
            .expect("Failed to call TestSpecialCharacters");

        // Verify special character handling
        assert_eq!(
            result.normalText, "Hello World",
            "Expected normalText to be 'Hello World', got '{}'",
            result.normalText
        );
        assert!(
            result.withNewlines.contains('\n'),
            "Expected withNewlines to contain newlines"
        );
        assert!(
            result.withTabs.contains('\t'),
            "Expected withTabs to contain tabs"
        );
        assert!(
            result.withQuotes.contains('"'),
            "Expected withQuotes to contain quotes"
        );
        assert!(
            result.withBackslashes.contains('\\'),
            "Expected withBackslashes to contain backslashes"
        );
        assert!(
            !result.withUnicode.is_empty(),
            "Expected withUnicode to be non-empty"
        );
        assert!(
            !result.withEmoji.is_empty(),
            "Expected withEmoji to be non-empty"
        );
        assert!(
            !result.withMixedSpecial.is_empty(),
            "Expected withMixedSpecial to be non-empty"
        );
    }

    #[test]
    fn test_number_edge_cases() {
        let result = B.TestNumberEdgeCases
            .call("test number edge cases")
            .expect("Failed to call TestNumberEdgeCases");

        // Verify number edge cases
        assert_eq!(result.zero, 0, "Expected zero to be 0, got {}", result.zero);
        assert!(
            result.negativeInt < 0,
            "Expected negativeInt to be negative, got {}",
            result.negativeInt
        );
        assert!(
            result.largeInt > 1000,
            "Expected largeInt to be large (>1000), got {}",
            result.largeInt
        );
        assert!(
            result.veryLargeInt > 1000000,
            "Expected veryLargeInt to be very large (>1000000), got {}",
            result.veryLargeInt
        );
        assert!(
            result.smallFloat < 1.0,
            "Expected smallFloat to be small (<1.0), got {}",
            result.smallFloat
        );
        assert!(
            result.largeFloat > 1000.0,
            "Expected largeFloat to be large (>1000.0), got {}",
            result.largeFloat
        );
        assert!(
            result.negativeFloat < 0.0,
            "Expected negativeFloat to be negative, got {}",
            result.negativeFloat
        );
        assert!(
            result.scientificNotation.abs() != 0.0,
            "Expected scientificNotation to be non-zero, got {}",
            result.scientificNotation
        );
    }

    #[test]
    fn test_circular_reference() {
        let result = B.TestCircularReference
            .call("test circular reference")
            .expect("Failed to call TestCircularReference");

        // Verify circular reference structure
        assert_eq!(result.id, 1, "Expected root id to be 1, got {}", result.id);
        assert!(
            !result.name.is_empty(),
            "Expected root name to be non-empty"
        );
        assert_eq!(
            result.children.len(),
            2,
            "Expected 2 children, got {}",
            result.children.len()
        );

        // Verify children structure
        let child1 = &result.children[0];
        let child2 = &result.children[1];

        assert!(
            child1.id == 2 || child1.id == 3,
            "Expected child1 id to be 2 or 3, got {}",
            child1.id
        );
        assert!(
            child2.id == 2 || child2.id == 3,
            "Expected child2 id to be 2 or 3, got {}",
            child2.id
        );
        assert_ne!(
            child1.id, child2.id,
            "Expected children to have different ids"
        );

        // Verify parent references (if not causing circular serialization issues)
        if let Some(ref parent) = child1.parent {
            assert_eq!(
                parent.id, 1,
                "Expected child1 parent id to be 1, got {}",
                parent.id
            );
        }
        if let Some(ref parent) = child2.parent {
            assert_eq!(
                parent.id, 1,
                "Expected child2 parent id to be 1, got {}",
                parent.id
            );
        }

        // Verify related items exist (just check it's a valid array)
        // The length check is >= 0 which is always true for Vec, so we just verify it exists
        let _ = result.relatedItems.len();
    }
}
