// Test file for generated BAML client
// This will be compiled against the generated baml_client module

mod baml_client;

use baml_client::B;
use baml_client::types::*;

fn main() {
    // Create a union category using the Kservice variant
    let category = Union2KresourceOrKservice::Kservice("service".to_string());

    let input = ExistingSystemComponent {
        id: 1,
        name: "Hello".to_string(),
        r#type: "service".to_string(),
        category,
        explanation: "Hello".to_string(),
    };

    let array = vec![input];

    let result = B.JsonInput(&array).expect("Failed to call JsonInput");
    println!("Result: {:?}", result);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Streaming not yet implemented for Rust
    fn test_json_input_stream() {
        // Create a union category using the Kservice variant
        let category = Union2KresourceOrKservice::Kservice("service".to_string());

        let input = ExistingSystemComponent {
            id: 1,
            name: "Hello".to_string(),
            r#type: "service".to_string(),
            category,
            explanation: "Hello".to_string(),
        };

        let array = vec![input];

        // TODO: Implement when streaming is available for Rust
        // let stream = B.JsonInputStream(&array).expect("Failed to start JsonInput stream");
        // ... process stream ...

        println!("Streaming test skipped - not yet implemented");
    }

    #[test]
    fn test_json_input() {
        // Create a union category using the Kservice variant
        let category = Union2KresourceOrKservice::Kservice("service".to_string());

        let input = ExistingSystemComponent {
            id: 1,
            name: "Hello".to_string(),
            r#type: "service".to_string(),
            category,
            explanation: "Hello".to_string(),
        };

        let array = vec![input];

        let result = B.JsonInput(&array).expect("Failed to call JsonInput");

        // Basic validation - ensure we get a non-empty result
        assert!(!result.is_empty(), "Expected non-empty result from JsonInput");
    }
}
