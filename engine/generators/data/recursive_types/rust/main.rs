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
    fn test_foo() {
        let result = B.Foo.call(8192).expect("Failed to call Foo");

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

        let result = B.JsonInput.call(&input).expect("Failed to call JsonInput");

        // Basic validation that we got a result
        assert!(result.is_some(), "Expected non-null result from JsonInput");
    }

    #[test]
    fn test_foo_stream() {
        let mut stream = B.Foo.stream(8192).expect("Failed to start Foo stream");

        let mut partial_count = 0;
        for partial in stream.partials() {
            let _partial = partial.expect("Error receiving partial");
            partial_count += 1;
        }

        let final_result = stream
            .get_final_response()
            .expect("Failed to get final response");

        assert!(
            final_result.is_some(),
            "Expected non-null result from Foo stream"
        );
        println!("Foo stream completed with {} partials", partial_count);
    }
}
