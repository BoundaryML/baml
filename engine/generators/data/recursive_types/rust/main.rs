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
    fn test_foo() {
        let result = B.Foo(8192).expect("Failed to call Foo");

        // Basic validation that we got a result
        // JSON is Option<Union5...>, so we check it's Some
        assert!(result.is_some(), "Expected non-null result from Foo");
    }

    #[test]
    fn test_json_input() {
        // Create union input with string value
        let input: JSON = Some(Union5FloatOrIntOrListJSONOrMapStringKeyJSONValueOrString::String(
            "Hello".to_string(),
        ));

        let result = B.JsonInput(&input).expect("Failed to call JsonInput");

        // Basic validation that we got a result
        assert!(result.is_some(), "Expected non-null result from JsonInput");
    }

    #[test]
    #[ignore] // Streaming not yet implemented in Rust
    fn test_foo_stream() {
        // TODO: Implement streaming test when streaming is available
        // This should mirror the Go test:
        // - Call stream.Foo(8192)
        // - Iterate over the channel
        // - Verify we get stream results and a final result
        unimplemented!("Streaming not yet implemented");
    }
}
