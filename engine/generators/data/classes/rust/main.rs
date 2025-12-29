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
    fn test_consume_simple_class() {
        let cls = SimpleClass {
            digits: 10,
            words: "hello".to_string(),
        };

        let result = B.ConsumeSimpleClass(&cls).expect("Failed to call ConsumeSimpleClass");

        // Basic validation that we got a result
        assert!(
            result.digits != 0 || !result.words.is_empty(),
            "Expected non-empty result from ConsumeSimpleClass"
        );
    }

    #[test]
    #[ignore] // Streaming not yet implemented for Rust
    fn test_make_simple_class_stream() {
        // TODO: Implement when streaming is available
        // This should mirror the Go test:
        // - Start a stream for MakeSimpleClass
        // - Iterate through stream results
        // - Validate we get a final result with non-zero values
    }
}
