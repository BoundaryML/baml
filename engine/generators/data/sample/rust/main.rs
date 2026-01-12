// Test file for generated BAML client
// This will be compiled against the generated baml_client module

mod baml_client;

use baml_client::sync_client::B;
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

#[cfg(test)]
mod observability_tests {
    use super::*;
    use baml::{
        Timing, Usage, LLMCall, LLMCallKind, LLMStreamCall, Collector, FunctionLog,
        HTTPRequest, HTTPResponse, HTTPBody, SSEResponse, LogType
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Helper function to test comprehensive collector API (mirrors Go's testCollectorAPI)
    fn test_collector_api(collector: &Collector, expected_function: &str) {
        // Test collector name
        let name = collector.name();
        assert!(!name.is_empty(), "Collector name should not be empty");
        println!("Collector name: {}", name);

        // Test usage collection
        let usage = collector.usage();
        assert!(usage.input_tokens() > 0, "Expected positive input tokens");
        assert!(usage.output_tokens() > 0, "Expected positive output tokens");
        println!("Usage - input: {}, output: {}", usage.input_tokens(), usage.output_tokens());

        // Test logs collection
        let logs = collector.logs();
        assert!(!logs.is_empty(), "Expected at least one log entry");
        println!("Found {} log entries", logs.len());

        // Test last log
        let last_log = collector.last().expect("Expected last log to exist");

        // Test function log details
        test_function_log_api(&last_log, expected_function);

        // Test all logs
        for (i, log) in logs.iter().enumerate() {
            println!("Testing log entry {}", i);
            test_function_log_api(log, "");
        }
    }

    /// Helper function to test FunctionLog API (mirrors Go's testFunctionLogAPI)
    fn test_function_log_api(log: &FunctionLog, expected_function: &str) {
        // Test log ID
        let id = log.id();
        assert!(!id.is_empty(), "Log ID should not be empty");
        println!("Log ID: {}", id);

        // Test function name
        let function_name = log.function_name();
        assert!(!function_name.is_empty(), "Function name should not be empty");
        println!("Function name: {}", function_name);
        if !expected_function.is_empty() && expected_function != "Multiple" {
            assert!(
                expected_function.contains(&function_name) || function_name.contains(expected_function),
                "Expected function name to contain {}, got {}",
                expected_function, function_name
            );
        }

        // Test log type
        let log_type = log.log_type();
        println!("Log type: {:?}", log_type);

        // Test timing
        let timing = log.timing();
        let start_time = timing.start_time_utc_ms();
        assert!(start_time > 0, "Start time should be positive");
        println!("Start time (UTC ms): {}", start_time);

        // Validate start time is reasonable (within last hour)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        assert!(
            start_time <= now && start_time > now - 3600000,
            "Start time seems unreasonable: {} (now: {})", start_time, now
        );

        if let Some(duration) = timing.duration_ms() {
            println!("Duration (ms): {}", duration);
            assert!(duration >= 0 && duration < 60000, "Duration seems unreasonable: {} ms", duration);
        }

        // Test usage from log
        let log_usage = log.usage();
        println!("Log input tokens: {}", log_usage.input_tokens());
        println!("Log output tokens: {}", log_usage.output_tokens());

        // Test raw LLM response
        if let Some(raw_response) = log.raw_llm_response() {
            println!("Raw LLM response length: {} characters", raw_response.len());
        }

        // Test calls
        let calls = log.calls();
        println!("Found {} LLM calls", calls.len());
        for (i, call) in calls.iter().enumerate() {
            test_llm_call_api(call, i as i32);
        }

        // Test selected call
        if let Some(selected) = log.selected_call() {
            println!("Found selected call");
            test_llm_call_api(&selected, -1); // -1 indicates selected call
        }
    }

    /// Helper function to test LLMCall API (mirrors Go's testLLMCallAPI)
    fn test_llm_call_api(call: &LLMCallKind, index: i32) {
        let prefix = if index == -1 {
            "Selected call".to_string()
        } else {
            format!("Call {}", index)
        };

        // Test client name
        let client_name = call.client_name();
        assert!(!client_name.is_empty(), "{}: Client name should not be empty", prefix);
        println!("{}: Client name: {}", prefix, client_name);

        // Test provider
        let provider = call.provider();
        assert!(!provider.is_empty(), "{}: Provider should not be empty", prefix);
        println!("{}: Provider: {}", prefix, provider);

        // Test selected status
        let selected = call.selected();
        println!("{}: Selected: {}", prefix, selected);
        if index == -1 {
            assert!(selected, "Selected call should have selected=true");
        }

        // Test usage
        if let Some(usage) = call.usage() {
            println!("{}: Input tokens: {}", prefix, usage.input_tokens());
            println!("{}: Output tokens: {}", prefix, usage.output_tokens());
        }

        // Test HTTP request
        if let Some(request) = call.http_request() {
            test_http_request_api(&request, &prefix);
        }

        // Type-specific tests based on LLMCallKind variant
        match call {
            LLMCallKind::Call(llm_call) => {
                // Test timing for regular call
                let timing = llm_call.timing();
                println!("{}: Start time: {}", prefix, timing.start_time_utc_ms());
                if let Some(duration) = timing.duration_ms() {
                    println!("{}: Duration: {} ms", prefix, duration);
                }

                // Test HTTP response (available for non-streaming)
                if let Some(response) = llm_call.http_response() {
                    test_http_response_api(&response, &prefix);
                }
            }
            LLMCallKind::Stream(stream_call) => {
                // Test timing for stream call
                let timing = stream_call.timing();
                println!("{}: Start time: {}", prefix, timing.start_time_utc_ms());
                if let Some(duration) = timing.duration_ms() {
                    println!("{}: Duration: {} ms", prefix, duration);
                }

                // Test SSE chunks
                if let Some(chunks) = stream_call.sse_chunks() {
                    println!("{}: Found {} SSE chunks", prefix, chunks.len());
                    for chunk in chunks.iter().take(3) { // Only test first few
                        test_sse_response_api(chunk, &prefix);
                    }
                }
            }
        }
    }

    /// Helper function to test HTTPRequest API (mirrors Go's testHTTPRequestAPI)
    fn test_http_request_api(request: &HTTPRequest, prefix: &str) {
        // Test request ID
        let id = request.id();
        assert!(!id.is_empty(), "{}: Request ID should not be empty", prefix);
        println!("{}: Request ID: {}", prefix, id);

        // Test URL
        let url = request.url();
        assert!(url.starts_with("http"), "{}: URL should start with http, got: {}", prefix, url);
        println!("{}: URL: {}", prefix, url);

        // Test method
        let method = request.method();
        assert!(method == "POST" || method == "GET", "{}: Unexpected HTTP method: {}", prefix, method);
        println!("{}: Method: {}", prefix, method);

        // Test headers
        let headers = request.headers();
        println!("{}: Headers count: {}", prefix, headers.len());
        if let Some(content_type) = headers.get("content-type") {
            println!("{}: Content-Type: {}", prefix, content_type);
        }

        // Test body
        let body = request.body();
        test_http_body_api(&body, &format!("{} request", prefix));
    }

    /// Helper function to test HTTPResponse API (mirrors Go's testHTTPResponseAPI)
    fn test_http_response_api(response: &HTTPResponse, prefix: &str) {
        // Test request ID
        let id = response.id();
        println!("{}: Response request ID: {}", prefix, id);

        // Test status
        let status = response.status();
        assert!(status >= 200 && status < 300, "{}: Unexpected HTTP status: {}", prefix, status);
        println!("{}: Status: {}", prefix, status);

        // Test headers
        let headers = response.headers();
        println!("{}: Response headers count: {}", prefix, headers.len());

        // Test body
        let body = response.body();
        test_http_body_api(&body, &format!("{} response", prefix));
    }

    /// Helper function to test HTTPBody API (mirrors Go's testHTTPBodyAPI)
    fn test_http_body_api(body: &HTTPBody, prefix: &str) {
        // Test text
        if let Ok(text) = body.text() {
            println!("{}: Body text length: {} characters", prefix, text.len());
            if text.len() > 0 && text.len() < 200 {
                println!("{}: Body text preview: {}", prefix, &text[..std::cmp::min(100, text.len())]);
            }
        }

        // Test JSON
        match body.json() {
            Ok(_json) => println!("{}: Body contains valid JSON", prefix),
            Err(e) => println!("{}: Body is not valid JSON: {}", prefix, e),
        }
    }

    /// Helper function to test SSEResponse API (mirrors Go's testSSEResponseAPI)
    fn test_sse_response_api(sse: &SSEResponse, prefix: &str) {
        // Test text
        let text = sse.text();
        println!("{}: SSE text length: {} characters", prefix, text.len());

        // Test JSON
        if let Some(json) = sse.json() {
            println!("{}: SSE contains JSON: {:?}", prefix, json);
        } else {
            println!("{}: SSE JSON is null or invalid", prefix);
        }
    }

    // ==========================================================================
    // Actual Tests
    // ==========================================================================

    #[test]
    fn test_timing_after_call() {
        let collector = new_collector("timing-test");
        let result = B.Foo.with_collector(&collector).call(8192);
        assert!(result.is_ok());

        let log = collector.last().expect("Should have a log");
        let timing = log.timing();

        assert!(timing.start_time_utc_ms() > 0, "start_time should be positive");
        assert!(timing.duration_ms().is_some(), "duration should be set");
        assert!(timing.duration_ms().unwrap() > 0, "duration should be positive");
    }

    #[test]
    fn test_calls_returns_llm_calls() {
        let collector = new_collector("calls-test");
        let result = B.Foo.with_collector(&collector).call(8192);
        assert!(result.is_ok());

        let log = collector.last().expect("Should have a log");
        let calls = log.calls();

        assert!(!calls.is_empty(), "Should have at least 1 LLM call");

        let call = &calls[0];
        assert!(!call.provider().is_empty());
        assert!(call.selected() || calls.len() > 1, "At least one call should be selected");
    }

    #[test]
    fn test_llm_call_has_http_request() {
        let collector = new_collector("http-request-test");
        let result = B.Foo.with_collector(&collector).call(8192);
        assert!(result.is_ok());

        let log = collector.last().expect("Should have a log");
        let calls = log.calls();
        let call = &calls[0];

        let request = call.http_request().expect("Should have HTTP request");
        assert!(!request.url().is_empty(), "URL should not be empty");
        assert_eq!(request.method(), "POST");

        // Check body is valid JSON
        let body_json = request.body().json();
        assert!(body_json.is_ok(), "Body should be valid JSON");
    }

    #[test]
    fn test_llm_call_has_usage() {
        let collector = new_collector("usage-test");
        let result = B.Foo.with_collector(&collector).call(8192);
        assert!(result.is_ok());

        let log = collector.last().expect("Should have a log");
        let calls = log.calls();
        let call = &calls[0];

        let usage = call.usage().expect("Should have usage");
        assert!(usage.input_tokens() > 0, "Should have input tokens");
        assert!(usage.output_tokens() > 0, "Should have output tokens");
    }

    #[test]
    fn test_selected_call() {
        let collector = new_collector("selected-call-test");
        let result = B.Foo.with_collector(&collector).call(8192);
        assert!(result.is_ok());

        let log = collector.last().expect("Should have a log");
        let selected = log.selected_call().expect("Should have a selected call");

        assert!(selected.selected(), "Selected call should have selected=true");
        assert!(!selected.provider().is_empty());
    }

    #[test]
    fn test_cached_input_tokens() {
        let collector = new_collector("cached-tokens-test");
        let result = B.Foo.with_collector(&collector).call(8192);
        assert!(result.is_ok());

        let usage = collector.usage();
        // cached_input_tokens may be None or Some(0) for non-cached calls
        // The important thing is that the method exists and doesn't panic
        let cached = usage.cached_input_tokens();
        println!("Cached input tokens: {:?}", cached);
    }

    #[test]
    fn test_comprehensive_collector_api() {
        let collector = new_collector("comprehensive-test");
        let result = B.Foo.with_collector(&collector).call(8192);
        assert!(result.is_ok());

        test_collector_api(&collector, "Foo");
    }

    #[test]
    fn test_streaming_observability() {
        let collector = new_collector("stream-observability-test");

        let mut stream = B.Foo
            .with_collector(&collector)
            .stream(8192)
            .expect("Failed to start stream");

        // Consume the stream
        for partial in stream.partials() {
            let _ = partial.expect("Error receiving partial");
        }
        let _ = stream.get_final_response().expect("Failed to get final response");

        let log = collector.last().expect("Should have a log");
        assert_eq!(log.log_type(), LogType::Stream);

        // Verify timing exists
        let timing = log.timing();
        assert!(timing.start_time_utc_ms() > 0);

        // Verify calls contain stream calls
        let calls = log.calls();
        assert!(!calls.is_empty());

        for call in &calls {
            if let LLMCallKind::Stream(stream_call) = call {
                // Stream calls should have SSE chunks
                if let Some(chunks) = stream_call.sse_chunks() {
                    println!("Stream call has {} SSE chunks", chunks.len());
                }
            }
        }
    }
}
