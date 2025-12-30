// Test file for generated BAML client
// This will be compiled against the generated baml_client module

mod baml_client;

use baml_client::B;
use baml_client::new_collector;
use baml_client::types::*;

fn main() {
    println!("Test - baml_client module loaded successfully!");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper function to validate that a union result is meaningful.
    /// Returns the variant name as a string for validation.
    fn get_variant_name(result: &Union2ExampleOrExample2) -> &'static str {
        match result {
            Union2ExampleOrExample2::Example(_) => "Example",
            Union2ExampleOrExample2::Example2(_) => "Example2",
        }
    }

    #[test]
    fn test_baml_client_compiles() {
        // This test verifies the generated code compiles
        println!("baml_client module compiles successfully");
    }

    #[test]
    fn test_foo() {
        // Create a collector for this test
        let collector = new_collector("test-foo-collector");

        // Call Foo with value 8192 and collector
        let result = B
            .with_options(|opts| opts.with_collector(&collector))
            .Foo(8192)
            .expect("Failed to call Foo");

        // Validate result has a valid variant
        let variant_name = get_variant_name(&result);
        assert!(
            !variant_name.is_empty(),
            "Expected valid result from Foo, got empty variant name"
        );
        println!("Foo returned variant: {}", variant_name);

        // Validate the result structure based on variant
        match &result {
            Union2ExampleOrExample2::Example(example) => {
                println!(
                    "Got Example: type={}, a={:?}, b={}",
                    example.r#type, example.a, example.b
                );
                assert_eq!(
                    example.r#type, "example_1",
                    "Expected type to be 'example_1'"
                );
            }
            Union2ExampleOrExample2::Example2(example2) => {
                println!(
                    "Got Example2: type={}, element={}, element2={}",
                    example2.r#type, example2.element, example2.element2
                );
                assert_eq!(
                    example2.r#type, "example_2",
                    "Expected type to be 'example_2'"
                );
                // Validate nested item
                assert_eq!(
                    example2.item.r#type, "example_1",
                    "Expected nested item type to be 'example_1'"
                );
            }
        }

        // Verify collector captured the call
        let logs = collector.logs();
        assert!(
            !logs.is_empty(),
            "Collector should have at least one log entry"
        );
        println!("Collector captured {} log(s)", logs.len());
    }

    #[test]
    fn test_bar() {
        // Create a collector for this test
        let collector = new_collector("test-bar-collector");

        // Call Bar with value 42 and collector
        let result = B
            .with_options(|opts| opts.with_collector(&collector))
            .Bar(42)
            .expect("Failed to call Bar");

        // Validate result has a valid variant
        let variant_name = get_variant_name(&result);
        assert!(
            !variant_name.is_empty(),
            "Expected valid result from Bar, got empty variant name"
        );
        println!("Bar returned variant: {}", variant_name);

        // Validate the result structure based on variant
        match &result {
            Union2ExampleOrExample2::Example(example) => {
                println!(
                    "Got Example: type={}, a={:?}, b={}",
                    example.r#type, example.a, example.b
                );
            }
            Union2ExampleOrExample2::Example2(example2) => {
                println!(
                    "Got Example2: type={}, element={}, element2={}",
                    example2.r#type, example2.element, example2.element2
                );
            }
        }

        // Verify collector captured the call
        let logs = collector.logs();
        assert!(
            !logs.is_empty(),
            "Collector should have at least one log entry"
        );
        println!("Collector captured {} log(s)", logs.len());
    }

    #[test]
    fn test_foo_stream() {
        let mut stream = B.stream.Foo(8192).expect("Failed to start Foo stream");

        let mut partial_count = 0;
        for partial in stream.partials() {
            let _partial = partial.expect("Error receiving partial");
            partial_count += 1;
        }

        let final_result = stream
            .get_final_response()
            .expect("Failed to get final response");

        let variant_name = get_variant_name(&final_result);
        assert!(
            !variant_name.is_empty(),
            "Expected valid variant from Foo stream"
        );
        println!(
            "Foo stream completed with {} partials, final variant: {}",
            partial_count, variant_name
        );
    }

    #[test]
    fn test_bar_stream() {
        let mut stream = B.stream.Bar(42).expect("Failed to start Bar stream");

        let mut partial_count = 0;
        for partial in stream.partials() {
            let _partial = partial.expect("Error receiving partial");
            partial_count += 1;
        }

        let final_result = stream
            .get_final_response()
            .expect("Failed to get final response");

        let variant_name = get_variant_name(&final_result);
        assert!(
            !variant_name.is_empty(),
            "Expected valid variant from Bar stream"
        );
        println!(
            "Bar stream completed with {} partials, final variant: {}",
            partial_count, variant_name
        );
    }

    #[test]
    fn test_multiple_functions_with_collector() {
        // Create a shared collector for all calls
        let collector = new_collector("test-multiple-functions-collector");

        // Call Foo
        let result1 = B
            .with_options(|opts| opts.with_collector(&collector))
            .Foo(123)
            .expect("Failed to call Foo first time");
        assert!(
            !get_variant_name(&result1).is_empty(),
            "First Foo call should return valid result"
        );

        // Call Bar
        let result2 = B
            .with_options(|opts| opts.with_collector(&collector))
            .Bar(456)
            .expect("Failed to call Bar");
        assert!(
            !get_variant_name(&result2).is_empty(),
            "Bar call should return valid result"
        );

        // Call Foo again
        let result3 = B
            .with_options(|opts| opts.with_collector(&collector))
            .Foo(789)
            .expect("Failed to call Foo second time");
        assert!(
            !get_variant_name(&result3).is_empty(),
            "Second Foo call should return valid result"
        );

        // Verify collector has 3 logs
        let logs = collector.logs();
        assert_eq!(logs.len(), 3, "Expected 3 logs, got {}", logs.len());

        // Verify function names in logs
        let expected_functions = ["Foo", "Bar", "Foo"];
        for (i, log) in logs.iter().enumerate() {
            let function_name = log.function_name();
            assert_eq!(
                function_name, expected_functions[i],
                "Expected function name {} for log {}, got {}",
                expected_functions[i], i, function_name
            );
        }

        println!("All sequential calls completed successfully");
        println!("  Foo(123) -> {}", get_variant_name(&result1));
        println!("  Bar(456) -> {}", get_variant_name(&result2));
        println!("  Foo(789) -> {}", get_variant_name(&result3));
    }

    #[test]
    fn test_collector_clear() {
        // Create a collector
        let collector = new_collector("test-clear-collector");

        // Make some calls
        B.with_options(|opts| opts.with_collector(&collector))
            .Foo(111)
            .expect("Failed to call Foo");

        B.with_options(|opts| opts.with_collector(&collector))
            .Bar(222)
            .expect("Failed to call Bar");

        // Verify we have logs
        let logs = collector.logs();
        assert!(!logs.is_empty(), "Expected logs before clear, got none");
        let log_count = logs.len();
        println!("Collector has {} logs before clear", log_count);

        // Clear the collector
        let count = collector.clear();
        assert_eq!(
            count, 2,
            "Expected 2 logs to be cleared, got {}",
            count
        );

        println!("Collector cleared successfully, removed {} logs", count);
    }

    #[test]
    fn test_collector_usage() {
        // Create a collector
        let collector = new_collector("test-usage-collector");

        // Make a call
        B.with_options(|opts| opts.with_collector(&collector))
            .Foo(8192)
            .expect("Failed to call Foo");

        // Get usage statistics
        let usage = collector.usage();
        let input_tokens = usage.input_tokens();
        let output_tokens = usage.output_tokens();

        println!(
            "Usage - Input tokens: {}, Output tokens: {}",
            input_tokens, output_tokens
        );

        assert!(input_tokens > 0, "Expected positive input tokens");
        assert!(output_tokens > 0, "Expected positive output tokens");
    }

    #[test]
    fn test_collector_function_log_details() {
        // Create a collector
        let collector = new_collector("test-log-details-collector");

        // Make a call
        B.with_options(|opts| opts.with_collector(&collector))
            .Foo(8192)
            .expect("Failed to call Foo");

        // Get the last log
        let last_log = collector.last().expect("Expected at least one log");

        // Test log ID
        let id = last_log.id();
        assert!(!id.is_empty(), "Log ID should not be empty");
        println!("Log ID: {}", id);

        // Test function name
        let function_name = last_log.function_name();
        assert_eq!(function_name, "Foo", "Expected function name 'Foo'");
        println!("Function name: {}", function_name);

        // Test log type
        let log_type = last_log.log_type();
        println!("Log type: {:?}", log_type);

        // Test raw LLM response
        if let Some(raw_response) = last_log.raw_llm_response() {
            println!("Raw LLM response length: {} characters", raw_response.len());
            assert!(!raw_response.is_empty(), "Raw response should not be empty");
        }

        // Test usage from log
        let log_usage = last_log.usage();
        let input_tokens = log_usage.input_tokens();
        let output_tokens = log_usage.output_tokens();
        println!(
            "Log usage - Input tokens: {}, Output tokens: {}",
            input_tokens, output_tokens
        );
    }

    #[test]
    fn test_collector_name() {
        let collector_name = "my-named-collector";
        let collector = new_collector(collector_name);

        // Verify the collector name
        let name = collector.name();
        assert_eq!(
            name, collector_name,
            "Expected collector name '{}', got '{}'",
            collector_name, name
        );
        println!("Collector name verified: {}", name);
    }

    #[test]
    fn test_foo_with_different_values() {
        // Test with different input values to ensure robustness
        let test_values = [0i64, 1, 100, 1000, 8192];

        for value in test_values {
            let result = B.Foo(value).expect(&format!("Failed to call Foo with value {}", value));
            let variant_name = get_variant_name(&result);
            assert!(
                !variant_name.is_empty(),
                "Expected valid result from Foo({})",
                value
            );
            println!("Foo({}) returned variant: {}", value, variant_name);
        }
    }

    #[test]
    fn test_bar_with_different_values() {
        // Test with different input values to ensure robustness
        let test_values = [0i64, 1, 42, 99, 1000];

        for value in test_values {
            let result = B.Bar(value).expect(&format!("Failed to call Bar with value {}", value));
            let variant_name = get_variant_name(&result);
            assert!(
                !variant_name.is_empty(),
                "Expected valid result from Bar({})",
                value
            );
            println!("Bar({}) returned variant: {}", value, variant_name);
        }
    }

    #[test]
    fn test_result_debug_format() {
        // Test that results implement Debug properly
        let result = B.Foo(8192).expect("Failed to call Foo");
        let debug_str = format!("{:?}", result);
        assert!(!debug_str.is_empty(), "Debug format should not be empty");
        println!("Debug format: {}", debug_str);
    }

    #[test]
    fn test_result_clone() {
        // Test that results implement Clone properly
        let result = B.Foo(8192).expect("Failed to call Foo");
        let cloned = result.clone();

        // Verify both have same variant
        assert_eq!(
            get_variant_name(&result),
            get_variant_name(&cloned),
            "Cloned result should have same variant"
        );
    }
}
