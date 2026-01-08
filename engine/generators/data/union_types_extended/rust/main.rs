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
    fn test_primitive_unions() {
        let result = B.TestPrimitiveUnions.call("test primitive unions")
            .expect("Failed to call TestPrimitiveUnions");

        // Verify primitive union values using pattern matching
        match &result.stringOrInt {
            Union2IntOrString::String(_) | Union2IntOrString::Int(_) => {}
        }

        match &result.stringOrFloat {
            Union2FloatOrString::String(_) | Union2FloatOrString::Float(_) => {}
        }

        match &result.intOrFloat {
            Union2FloatOrInt::Int(_) | Union2FloatOrInt::Float(_) => {}
        }

        match &result.boolOrString {
            Union2BoolOrString::Bool(_) | Union2BoolOrString::String(_) => {}
        }

        match &result.anyPrimitive {
            Union4BoolOrFloatOrIntOrString::String(_)
            | Union4BoolOrFloatOrIntOrString::Int(_)
            | Union4BoolOrFloatOrIntOrString::Float(_)
            | Union4BoolOrFloatOrIntOrString::Bool(_) => {}
        }
    }

    #[test]
    fn test_complex_unions() {
        let result = B.TestComplexUnions.call("test complex unions")
            .expect("Failed to call TestComplexUnions");

        // Verify complex union values using pattern matching
        match &result.userOrProduct {
            Union2ProductOrUser::User(_) | Union2ProductOrUser::Product(_) => {}
        }

        match &result.userOrProductOrAdmin {
            Union3AdminOrProductOrUser::User(_)
            | Union3AdminOrProductOrUser::Product(_)
            | Union3AdminOrProductOrUser::Admin(_) => {}
        }

        match &result.dataOrError {
            Union2DataResponseOrErrorResponse::DataResponse(_)
            | Union2DataResponseOrErrorResponse::ErrorResponse(_) => {}
        }

        match &result.multiTypeResult {
            Union3ErrorOrSuccessOrWarning::Success(_)
            | Union3ErrorOrSuccessOrWarning::Warning(_)
            | Union3ErrorOrSuccessOrWarning::Error(_) => {}
        }
    }

    #[test]
    fn test_discriminated_unions() {
        let result = B.TestDiscriminatedUnions.call("test discriminated unions")
            .expect("Failed to call TestDiscriminatedUnions");

        // Verify shape is one of the valid variants
        match &result.shape {
            Union3CircleOrRectangleOrTriangle::Circle(_)
            | Union3CircleOrRectangleOrTriangle::Rectangle(_)
            | Union3CircleOrRectangleOrTriangle::Triangle(_) => {}
        }

        // Check if shape is a circle with the expected discriminator
        match &result.shape {
            Union3CircleOrRectangleOrTriangle::Circle(circle) => {
                assert_eq!(circle.shape, "circle", "Expected shape.shape to be 'circle'");
                assert_eq!(circle.radius, 5.0, "Expected circle.radius to be 5.0");
            }
            _ => panic!("Expected shape to be a Circle"),
        }

        // Verify animal is one of the valid variants
        match &result.animal {
            Union3BirdOrCatOrDog::Dog(_)
            | Union3BirdOrCatOrDog::Cat(_)
            | Union3BirdOrCatOrDog::Bird(_) => {}
        }

        // Check if animal is a dog
        match &result.animal {
            Union3BirdOrCatOrDog::Dog(dog) => {
                assert_eq!(dog.species, "dog", "Expected animal.species to be 'dog'");
                assert!(!dog.breed.is_empty(), "Expected dog.breed to be non-empty");
                assert!(dog.goodBoy, "Expected dog.goodBoy to be true");
            }
            _ => panic!("Expected animal to be a Dog"),
        }

        // Verify response is one of the valid variants
        match &result.response {
            Union3ApiErrorOrApiPendingOrApiSuccess::ApiSuccess(_)
            | Union3ApiErrorOrApiPendingOrApiSuccess::ApiError(_)
            | Union3ApiErrorOrApiPendingOrApiSuccess::ApiPending(_) => {}
        }

        // Check if response is an error
        match &result.response {
            Union3ApiErrorOrApiPendingOrApiSuccess::ApiError(api_error) => {
                assert_eq!(api_error.status, "error", "Expected response.status to be 'error'");
                assert_eq!(api_error.message, "Not found", "Expected error.message to be 'Not found'");
                assert_eq!(api_error.code, 404, "Expected error.code to be 404");
            }
            _ => panic!("Expected response to be an ApiError"),
        }
    }

    #[test]
    fn test_union_arrays() {
        let result = B.TestUnionArrays.call("test union arrays")
            .expect("Failed to call TestUnionArrays");

        // Verify union array contents
        assert_eq!(
            result.mixedArray.len(),
            4,
            "Expected mixedArray length 4, got {}",
            result.mixedArray.len()
        );

        assert_eq!(
            result.nullableItems.len(),
            4,
            "Expected nullableItems length 4, got {}",
            result.nullableItems.len()
        );

        assert!(
            result.objectArray.len() >= 2,
            "Expected at least 2 objects in objectArray, got {}",
            result.objectArray.len()
        );

        assert_eq!(
            result.nestedUnionArray.len(),
            4,
            "Expected nestedUnionArray length 4, got {}",
            result.nestedUnionArray.len()
        );
    }
}
