// Test file for generated BAML client
// This will be compiled against the generated baml_client module

mod baml_client;

use baml_client::sync_client::B;
use baml_client::types::*;

fn main() {
    // Create a union category using the Kservice variant
    let category = Union2KresourceOrKservice::Kservice;

    let input = ExistingSystemComponent {
        id: 1,
        name: "Hello".to_string(),
        r#type: "service".to_string(),
        category,
        explanation: "Hello".to_string(),
    };

    let array = vec![input];

    let result = B.JsonInput.call(&array).expect("Failed to call JsonInput");
    println!("Result: {:?}", result);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_input_stream() {
        // Create a union category using the Kservice variant
        let category = Union2KresourceOrKservice::Kservice;

        let input = ExistingSystemComponent {
            id: 1,
            name: "Hello".to_string(),
            r#type: "service".to_string(),
            category,
            explanation: "Hello".to_string(),
        };

        let array = vec![input];

        let mut stream = B
            .JsonInput
            .stream(&array)
            .expect("Failed to start JsonInput stream");

        let mut partial_count = 0;
        for partial in stream.partials() {
            let _partial = partial.expect("Error receiving partial");
            partial_count += 1;
        }

        let final_result = stream
            .get_final_response()
            .expect("Failed to get final response");

        assert!(
            !final_result.is_empty(),
            "Expected non-empty result from JsonInput stream"
        );
        println!(
            "JsonInput stream completed with {} partials, result len: {}",
            partial_count,
            final_result.len()
        );
    }

    #[test]
    fn test_json_input() {
        // Create a union category using the Kservice variant
        let category = Union2KresourceOrKservice::Kservice;

        let input = ExistingSystemComponent {
            id: 1,
            name: "Hello".to_string(),
            r#type: "service".to_string(),
            category,
            explanation: "Hello".to_string(),
        };

        let array = vec![input];

        let result = B.JsonInput.call(&array).expect("Failed to call JsonInput");

        // Basic validation - ensure we get a non-empty result
        assert!(
            !result.is_empty(),
            "Expected non-empty result from JsonInput"
        );
    }
}
