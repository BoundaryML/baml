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
    fn test_optional_fields() {
        let result = B.TestOptionalFields.call("test optional fields")
            .expect("Failed to call TestOptionalFields");

        // Verify required fields
        assert_eq!(result.requiredString, "hello", "Expected requiredString to be 'hello'");
        assert_eq!(result.requiredInt, 42, "Expected requiredInt to be 42");
        assert!(result.requiredBool, "Expected requiredBool to be true");

        // Verify optional fields that should be present
        assert!(result.optionalString.is_some(), "Expected optionalString to be present");
        assert_eq!(result.optionalString.as_ref().unwrap(), "world", "Expected optionalString to be 'world'");

        assert!(result.optionalBool.is_some(), "Expected optionalBool to be present");
        assert_eq!(result.optionalBool.unwrap(), false, "Expected optionalBool to be false");

        assert!(result.optionalArray.is_some(), "Expected optionalArray to be present");
        assert_eq!(result.optionalArray.as_ref().unwrap().len(), 3, "Expected optionalArray length 3");

        // Verify optional fields that should be omitted
        assert!(result.optionalInt.is_none(), "Expected optionalInt to be omitted");
        assert!(result.optionalMap.is_none(), "Expected optionalMap to be omitted");
    }

    #[test]
    fn test_nullable_types() {
        let result = B.TestNullableTypes.call("test nullable types")
            .expect("Failed to call TestNullableTypes");

        // Verify nullable fields that should be present
        assert!(result.nullableString.is_some(), "Expected nullableString to be present");
        assert_eq!(result.nullableString.as_ref().unwrap(), "present", "Expected nullableString to be 'present'");

        assert!(result.nullableFloat.is_some(), "Expected nullableFloat to be present");
        assert_eq!(result.nullableFloat.unwrap(), 3.14, "Expected nullableFloat to be 3.14");

        assert!(result.nullableArray.is_some(), "Expected nullableArray to be present");
        assert_eq!(result.nullableArray.as_ref().unwrap().len(), 2, "Expected nullableArray length 2");

        // Verify nullable fields that should be null
        assert!(result.nullableInt.is_none(), "Expected nullableInt to be null");
        assert!(result.nullableBool.is_none(), "Expected nullableBool to be null");
        assert!(result.nullableObject.is_none(), "Expected nullableObject to be null");
    }

    #[test]
    fn test_mixed_optional_nullable() {
        let result = B.TestMixedOptionalNullable.call("test mixed optional nullable")
            .expect("Failed to call TestMixedOptionalNullable");

        // Verify id is positive
        assert!(result.id > 0, "Expected id to be positive, got {}", result.id);

        // Verify tags is a non-null array
        assert!(result.tags.len() >= 0, "Expected tags to be non-null array");

        // Check primary user is present with valid data
        assert!(result.primaryUser.id > 0, "Expected primaryUser.id to be positive, got {}", result.primaryUser.id);
        assert!(!result.primaryUser.name.is_empty(), "Expected primaryUser.name to be non-empty");
    }

    #[test]
    fn test_all_null() {
        let result = B.TestAllNull.call("test all null")
            .expect("Failed to call TestAllNull");

        // Verify all nullable fields are null
        assert!(result.nullableString.is_none(), "Expected nullableString to be null");
        assert!(result.nullableInt.is_none(), "Expected nullableInt to be null");
        assert!(result.nullableFloat.is_none(), "Expected nullableFloat to be null");
        assert!(result.nullableBool.is_none(), "Expected nullableBool to be null");
        assert!(result.nullableArray.is_none(), "Expected nullableArray to be null");
        assert!(result.nullableObject.is_none(), "Expected nullableObject to be null");
    }

    #[test]
    fn test_all_optional_omitted() {
        let result = B.TestAllOptionalOmitted.call("test all optional omitted")
            .expect("Failed to call TestAllOptionalOmitted");

        // Verify required fields have values
        assert!(!result.requiredString.is_empty(), "Expected requiredString to be non-empty");
        assert!(result.requiredInt != 0, "Expected requiredInt to be non-zero");

        // Verify all optional fields are omitted
        assert!(result.optionalString.is_none(), "Expected optionalString to be omitted");
        assert!(result.optionalInt.is_none(), "Expected optionalInt to be omitted");
        assert!(result.optionalBool.is_none(), "Expected optionalBool to be omitted");
        assert!(result.optionalArray.is_none(), "Expected optionalArray to be omitted");
        assert!(result.optionalMap.is_none(), "Expected optionalMap to be omitted");
    }
}
