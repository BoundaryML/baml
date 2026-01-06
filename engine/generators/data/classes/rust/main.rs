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
    fn test_consume_simple_class() {
        let cls = SimpleClass {
            digits: 10,
            words: "hello".to_string(),
        };

        let result = B.ConsumeSimpleClass.call(&cls).expect("Failed to call ConsumeSimpleClass");

        // Basic validation that we got a result
        assert!(
            result.digits != 0 || !result.words.is_empty(),
            "Expected non-empty result from ConsumeSimpleClass"
        );
    }

    #[test]
    fn test_make_simple_class_stream() {
        let mut stream = B.MakeSimpleClass
            .stream()
            .expect("Failed to start MakeSimpleClass stream");

        let mut partial_count = 0;
        for partial in stream.partials() {
            let _partial = partial.expect("Error receiving partial");
            partial_count += 1;
        }

        assert!(partial_count > 0, "Expected at least one partial but got {partial_count}");
        let final_result = stream
            .get_final_response()
            .expect("Failed to get final response");

        assert!(
            final_result.digits != 0 || !final_result.words.is_empty(),
            "Expected SimpleClass with valid data"
        );
        println!(
            "MakeSimpleClass stream completed with {} partials: {:?}",
            partial_count, final_result
        );
    }
}
