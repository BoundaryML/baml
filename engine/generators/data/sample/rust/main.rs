// Test file for generated BAML client
// This will be compiled against the generated baml_client module

mod baml_client;

use baml_client::B;
use baml_client::new_collector;
use baml_client::types::*;
use baml_client::ClientRegistry;

fn main() {
    println!("Test - baml_client module loaded successfully!");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_variant_name(result: &Union2ExampleOrExample2) -> &'static str {
        match result {
            Union2ExampleOrExample2::Example(_) => "Example",
            Union2ExampleOrExample2::Example2(_) => "Example2",
        }
    }

    #[test]
    fn test_baml_client_compiles() {
        println!("baml_client module compiles successfully");
    }

    #[test]
    fn test_foo() {
        let collector = new_collector("test-foo-collector");

        // New pattern: B.Function.with_options().call()
        let result = B.Foo
            .with_collector(&collector)
            .call(8192)
            .expect("Failed to call Foo");

        let variant_name = get_variant_name(&result);
        assert!(!variant_name.is_empty());
        println!("Foo returned variant: {}", variant_name);

        // Verify collector captured the call
        let logs = collector.logs();
        assert!(!logs.is_empty());
        println!("Collector captured {} log(s)", logs.len());
    }

    #[test]
    fn test_bar() {
        let collector = new_collector("test-bar-collector");

        let result = B.Bar
            .with_collector(&collector)
            .call(42)
            .expect("Failed to call Bar");

        let variant_name = get_variant_name(&result);
        assert!(!variant_name.is_empty());
        println!("Bar returned variant: {}", variant_name);

        let logs = collector.logs();
        assert!(!logs.is_empty());
    }

    #[test]
    fn test_foo_stream() {
        // New pattern: B.Function.stream()
        let mut stream = B.Foo.stream(8192).expect("Failed to start Foo stream");

        let mut partial_count = 0;
        for partial in stream.partials() {
            let _partial = partial.expect("Error receiving partial");
            partial_count += 1;
        }

        let final_result = stream
            .get_final_response()
            .expect("Failed to get final response");

        let variant_name = get_variant_name(&final_result);
        assert!(!variant_name.is_empty());
        println!("Foo stream completed with {} partials", partial_count);
    }

    #[test]
    fn test_bar_stream() {
        let mut stream = B.Bar.stream(42).expect("Failed to start Bar stream");

        let mut partial_count = 0;
        for partial in stream.partials() {
            let _partial = partial.expect("Error receiving partial");
            partial_count += 1;
        }

        let final_result = stream
            .get_final_response()
            .expect("Failed to get final response");

        assert!(!get_variant_name(&final_result).is_empty());
    }

    #[test]
    fn test_multiple_functions_with_collector() {
        let collector = new_collector("test-multiple-functions-collector");

        // Client-level options pattern
        let client = B.with_collector(&collector);

        let result1 = client.Foo.call(123).expect("Failed to call Foo first time");
        assert!(!get_variant_name(&result1).is_empty());

        let result2 = client.Bar.call(456).expect("Failed to call Bar");
        assert!(!get_variant_name(&result2).is_empty());

        let result3 = client.Foo.call(789).expect("Failed to call Foo second time");
        assert!(!get_variant_name(&result3).is_empty());

        let logs = collector.logs();
        assert_eq!(logs.len(), 3);

        let expected_functions = ["Foo", "Bar", "Foo"];
        for (i, log) in logs.iter().enumerate() {
            assert_eq!(log.function_name(), expected_functions[i]);
        }
    }

    #[test]
    fn test_collector_clear() {
        let collector = new_collector("test-clear-collector");

        B.Foo.with_collector(&collector).call(111).expect("Failed to call Foo");
        B.Bar.with_collector(&collector).call(222).expect("Failed to call Bar");

        let logs = collector.logs();
        assert!(!logs.is_empty());

        let count = collector.clear();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_collector_usage() {
        let collector = new_collector("test-usage-collector");

        B.Foo.with_collector(&collector).call(8192).expect("Failed to call Foo");

        let usage = collector.usage();
        assert!(usage.input_tokens() > 0);
        assert!(usage.output_tokens() > 0);
    }

    #[test]
    fn test_collector_function_log_details() {
        let collector = new_collector("test-log-details-collector");

        B.Foo.with_collector(&collector).call(8192).expect("Failed to call Foo");

        let last_log = collector.last().expect("Expected at least one log");
        assert!(!last_log.id().is_empty());
        assert_eq!(last_log.function_name(), "Foo");
    }

    #[test]
    fn test_collector_name() {
        let collector_name = "my-named-collector";
        let collector = new_collector(collector_name);
        assert_eq!(collector.name(), collector_name);
    }

    #[test]
    fn test_foo_with_different_values() {
        let test_values = [0i64, 1, 100, 1000, 8192];

        for value in test_values {
            let result = B.Foo.call(value).expect(&format!("Failed to call Foo with value {}", value));
            assert!(!get_variant_name(&result).is_empty());
        }
    }

    #[test]
    fn test_bar_with_different_values() {
        let test_values = [0i64, 1, 42, 99, 1000];

        for value in test_values {
            let result = B.Bar.call(value).expect(&format!("Failed to call Bar with value {}", value));
            assert!(!get_variant_name(&result).is_empty());
        }
    }

    #[test]
    fn test_result_debug_format() {
        let result = B.Foo.call(8192).expect("Failed to call Foo");
        let debug_str = format!("{:?}", result);
        assert!(!debug_str.is_empty());
    }

    #[test]
    fn test_result_clone() {
        let result = B.Foo.call(8192).expect("Failed to call Foo");
        let cloned = result.clone();
        assert_eq!(get_variant_name(&result), get_variant_name(&cloned));
    }
}

#[cfg(test)]
mod async_tests {
    use crate::baml_client::async_client::B;
    use crate::baml_client::new_collector;
    use crate::baml_client::types::*;
    use baml::LogType;

    fn get_variant_name(result: &Union2ExampleOrExample2) -> &'static str {
        match result {
            Union2ExampleOrExample2::Example(_) => "Example",
            Union2ExampleOrExample2::Example2(_) => "Example2",
        }
    }

    #[tokio::test]
    async fn test_foo_async() {
        let collector = new_collector("test-foo-async-collector");

        let result = B.Foo
            .with_collector(&collector)
            .call(8192)
            .await
            .expect("Failed to call Foo async");

        let variant_name = get_variant_name(&result);
        assert!(!variant_name.is_empty());

        // Verify collector captured the call correctly
        let logs = collector.logs();
        assert_eq!(logs.len(), 1);

        let log = &logs[0];
        assert_eq!(log.function_name(), "Foo");
        assert_eq!(log.log_type(), LogType::Call);
        assert!(!log.id().is_empty());

        // Verify tokens were used
        let usage = log.usage();
        assert!(usage.input_tokens() > 0, "Should have input tokens");
        assert!(usage.output_tokens() > 0, "Should have output tokens");
    }

    #[tokio::test]
    async fn test_foo_stream_async() {
        let collector = new_collector("test-foo-stream-async-collector");

        let mut stream = B.Foo
            .with_collector(&collector)
            .stream(8192)
            .expect("Failed to start Foo stream");

        let mut partial_count = 0;
        while let Some(partial) = stream.next().await {
            let _partial = partial.expect("Error receiving partial");
            partial_count += 1;
        }

        let final_result = stream
            .get_final_response()
            .await
            .expect("Failed to get final response");

        let variant_name = get_variant_name(&final_result);
        assert!(!variant_name.is_empty());
        assert!(partial_count > 0, "Should have received at least one partial");

        // Verify collector captured streaming call
        let logs = collector.logs();
        assert_eq!(logs.len(), 1);

        let log = &logs[0];
        assert_eq!(log.function_name(), "Foo");
        assert_eq!(log.log_type(), LogType::Stream);

        // Verify tokens were used
        let usage = log.usage();
        assert!(usage.input_tokens() > 0, "Should have input tokens");
        assert!(usage.output_tokens() > 0, "Should have output tokens");
    }

    #[tokio::test]
    async fn test_stream_cancellation_on_drop() {
        let collector = new_collector("test-cancellation-collector");

        // Start a stream but drop it before completion
        {
            let mut stream = B.Foo
                .with_collector(&collector)
                .stream(8192)
                .expect("Failed to start Foo stream");
            // Get just one partial to ensure stream started
            let first = stream.next().await;
            assert!(first.is_some(), "Should receive at least one partial");
            // Drop stream here - should trigger cancellation
        }

        // Give a moment for cancellation to propagate
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Collector should have captured the call
        let logs = collector.logs();
        assert_eq!(logs.len(), 1, "Collector should have exactly one log");

        let log = &logs[0];
        assert_eq!(log.function_name(), "Foo");
        assert_eq!(log.log_type(), LogType::Stream);

        // Usage will be 0 since it's only populated on the final stream event,
        // which we cancelled before receiving
        let usage = log.usage();
        assert_eq!(usage.input_tokens(), 0, "Cancelled stream should have 0 input tokens");
        assert_eq!(usage.output_tokens(), 0, "Cancelled stream should have 0 output tokens");
    }

    #[tokio::test]
    async fn test_async_with_timeout_success() {
        use std::time::Duration;

        let collector = new_collector("test-timeout-success-collector");

        // Test that timeout works with async call (generous timeout)
        let result = tokio::time::timeout(
            Duration::from_secs(60),
            B.Foo.with_collector(&collector).call(100)
        ).await;

        assert!(result.is_ok(), "Call should complete within timeout");
        let inner = result.unwrap();
        assert!(inner.is_ok(), "Call should succeed");

        // Verify collector captured successful call
        let logs = collector.logs();
        assert_eq!(logs.len(), 1);

        let log = &logs[0];
        assert_eq!(log.function_name(), "Foo");
        assert_eq!(log.log_type(), LogType::Call);

        let usage = log.usage();
        assert!(usage.input_tokens() > 0);
        assert!(usage.output_tokens() > 0);
    }
}

#[cfg(test)]
mod client_registry_tests {
    use super::*;

    #[test]
    fn test_undefined_client_returns_error() {
        // Using an undefined client should return an error, not panic
        let result = B.Foo
            .with_client("NonExistentClient12345")
            .call(8192);

        assert!(result.is_err(), "Expected error for undefined client");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("NonExistentClient12345") || err_msg.contains("not found") || err_msg.contains("unknown"),
            "Error message should mention the client name: {}", err_msg
        );
    }

    #[test]
    fn test_client_registry_with_invalid_provider_returns_error() {
        let mut registry = ClientRegistry::new();
        registry.add_llm_client(
            "BadClient",
            "invalid_provider_xyz",
            [("model".to_string(), serde_json::json!("test"))].into_iter().collect(),
        );
        registry.set_primary_client("BadClient");

        let result = B.Foo
            .with_client_registry(&registry)
            .call(8192);

        assert!(result.is_err(), "Expected error for invalid provider");
    }

    #[test]
    fn test_client_registry_api_compiles() {
        // Test ClientRegistry API compiles and basic methods work
        let mut registry = ClientRegistry::new();

        registry.add_llm_client(
            "TestClient",
            "openai",
            [
                ("model".to_string(), serde_json::json!("gpt-4")),
                ("temperature".to_string(), serde_json::json!(0.7)),
                ("max_tokens".to_string(), serde_json::json!(100)),
            ].into_iter().collect(),
        );
        registry.set_primary_client("TestClient");

        // Verify registry is not empty after adding client
        assert!(!registry.is_empty());

        // Verify empty registry is empty
        let empty_registry = ClientRegistry::new();
        assert!(empty_registry.is_empty());
    }

    #[test]
    fn test_with_client_and_collector_chaining() {
        let collector = new_collector("client-chain-test");

        // Test that with_client and with_collector can be chained
        // This verifies the builder pattern works correctly
        let result = B.Foo
            .with_client("NonExistentClient")
            .with_collector(&collector)
            .call(8192);

        // Should fail due to invalid client, but collector should still be set
        assert!(result.is_err());
    }

    #[test]
    fn test_with_client_registry_and_collector_chaining() {
        let collector = new_collector("registry-chain-test");
        let mut registry = ClientRegistry::new();
        registry.add_llm_client(
            "ChainTest",
            "openai",
            [("model".to_string(), serde_json::json!("gpt-4"))].into_iter().collect(),
        );
        registry.set_primary_client("ChainTest");

        // Test that with_client_registry and with_collector can be chained
        let _result = B.Foo
            .with_client_registry(&registry)
            .with_collector(&collector)
            .call(8192);
        // Result may succeed or fail depending on API key availability
    }
}
