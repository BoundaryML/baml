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

    /// Test parsing partial JSON streams.
    /// Mirrors Go's TestParseStream - parses incrementally larger portions of JSON.
    /// Currently ignored as ParseStream is not yet implemented (Phase 7).
    #[test]
    #[ignore] // ParseStream not yet implemented for Rust (Phase 7)
    fn test_parse_stream() {
        // This test will parse partial JSON to validate streaming parse behavior.
        // The Go equivalent iterates through every substring of the raw JSON,
        // calling ParseStream.MakeSemanticContainer and validating error messages.
        //
        // Expected behavior:
        // - Partial parses should either succeed with partial data
        // - Or fail with specific "Missing required field" errors for class_done_needed or class_needed
        //
        // TODO: Implement when ParseStream is available:
        // let raw_text = r#"
        // {
        //     "sixteen_digit_number": 1234567890,
        //     "string_with_twenty_words": "Hello, world!",
        //     "class_1": {
        //         "i_16_digits": 1234567890,
        //         "s_20_words": "Hello, world!"
        //     },
        //     "class_2": {
        //         "i_16_digits": 1234567890,
        //         "s_20_words": "Hello, world!"
        //     },
        //     "class_done_needed": {
        //         "i_16_digits": 1234567890,
        //         "s_20_words": "Hello, world!"
        //     },
        //     "class_needed": {
        //         "i_16_digits": 1234567890,
        //         "s_20_words": "Hello, world!"
        //     }
        // "#;
        //
        // for i in 0..raw_text.len() {
        //     let partial = &raw_text[..i];
        //     match ParseStream.MakeSemanticContainer(partial) {
        //         Ok(result) => println!("-> result: {:?}", result),
        //         Err(e) => {
        //             let msg = e.to_string();
        //             assert!(
        //                 msg.contains("Missing required field: class_done_needed")
        //                     || msg.contains("Missing required field: class_needed"),
        //                 "Unexpected error in ParseStream: {}",
        //                 msg
        //             );
        //         }
        //     }
        // }

        println!("ParseStream test skipped - not yet implemented");
    }

    /// Test streaming version of MakeSemanticContainer.
    /// Mirrors Go's TestMakeSemanticContainerStream.
    #[test]
    fn test_make_semantic_container_stream() {
        let mut stream = B.MakeSemanticContainer
            .stream()
            .expect("Failed to start MakeSemanticContainer stream");

        let mut partial_count = 0;
        let mut reference_int: Option<i64> = None;

        for partial in stream.partials() {
            let partial = partial.expect("Error receiving partial");
            partial_count += 1;

            // Stability check: numeric fields should remain stable once set
            if let Some(num) = partial.sixteen_digit_number {
                if let Some(ref_int) = reference_int {
                    assert_eq!(
                        ref_int, num,
                        "sixteen_digit_number changed unexpectedly: {} != {}",
                        ref_int, num
                    );
                } else {
                    reference_int = Some(num);
                }
            }
        }

        let final_result = stream
            .get_final_response()
            .expect("Failed to get final response");

        // Validate the final result has valid data
        assert!(
            final_result.class_1.i_16_digits != 0 || !final_result.class_1.s_20_words.is_empty(),
            "Expected class_1 to have valid data"
        );

        println!(
            "MakeSemanticContainer stream completed with {} partials",
            partial_count
        );
        println!("Final result: {:?}", final_result);
    }

    /// Test synchronous MakeSemanticContainer function.
    /// Mirrors Go's TestMakeSemanticContainer.
    #[test]
    fn test_make_semantic_container() {
        let result = B.MakeSemanticContainer
            .call()
            .expect("Failed to call MakeSemanticContainer");

        // Basic validation - check that result contains valid data
        // The Go test checks result.BamlTypeName() != "", but Rust types
        // don't have this method directly. Instead, we validate the result
        // contains expected fields with reasonable values.

        // Validate that required nested classes are present with data
        assert!(
            result.class_1.i_16_digits != 0 || !result.class_1.s_20_words.is_empty(),
            "Expected class_1 to have valid data"
        );

        assert!(
            result.class_2.i_16_digits != 0 || !result.class_2.s_20_words.is_empty(),
            "Expected class_2 to have valid data"
        );

        assert!(
            result.class_done_needed.i_16_digits != 0
                || !result.class_done_needed.s_20_words.is_empty(),
            "Expected class_done_needed to have valid data"
        );

        assert!(
            result.class_needed.i_16_digits != 0 || !result.class_needed.s_20_words.is_empty(),
            "Expected class_needed to have valid data"
        );

        // Validate the result structure is complete
        // The result type SemanticContainer should have all fields populated
        println!("MakeSemanticContainer result: {:?}", result);
    }

    /// Test MakeClassWithBlockDone function.
    /// Additional test for semantic streaming class with block-level done annotation.
    #[test]
    fn test_make_class_with_block_done() {
        let result = B.MakeClassWithBlockDone
            .call()
            .expect("Failed to call MakeClassWithBlockDone");

        // Validate the result has expected fields
        assert!(
            result.i_16_digits != 0 || !result.s_20_words.is_empty(),
            "Expected ClassWithBlockDone to have valid data"
        );

        println!("MakeClassWithBlockDone result: {:?}", result);
    }

    /// Test MakeClassWithExternalDone function.
    /// Additional test for semantic streaming class with external done annotation.
    #[test]
    fn test_make_class_with_external_done() {
        let result = B.MakeClassWithExternalDone
            .call()
            .expect("Failed to call MakeClassWithExternalDone");

        // Validate the result has expected fields
        assert!(
            result.i_16_digits != 0 || !result.s_20_words.is_empty(),
            "Expected ClassWithoutDone to have valid data"
        );

        println!("MakeClassWithExternalDone result: {:?}", result);
    }
}
