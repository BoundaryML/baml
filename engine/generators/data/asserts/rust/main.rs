// Test file for generated BAML client
// This will be compiled against the generated baml_client module

mod baml_client;

use baml_client::B;
use baml_client::types::*;

fn main() {
    println!("Test - baml_client module loaded successfully!");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_person_test() {
        let result = B.PersonTest().expect("Failed to call PersonTest");
        println!("{:?}", result);

        // Validate the Person struct has expected fields
        assert!(!result.name.is_empty(), "Expected name to not be empty");
        assert!(result.age > 0, "Expected age to be greater than 0");
    }

    #[test]
    #[ignore = "Streaming not yet implemented for Rust"]
    fn test_person_test_stream() {
        // This test is skipped until streaming is implemented
        // The Go equivalent tests:
        // - Starting the stream
        // - Receiving partial results
        // - Receiving a final result
        // - Validating the final result has age > 0
        unimplemented!("Streaming not yet implemented");
    }
}
