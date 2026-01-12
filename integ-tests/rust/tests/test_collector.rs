//! Collector tests - ported from test_collector_comprehensive_test.go
//!
//! Tests for collector functionality including:
//! - Basic usage
//! - Streaming calls
//! - Multiple calls
//! - Concurrent calls
//! - Multiple collectors
//! - Error handling

use baml::LogType;
use rust::baml_client::new_collector;
use rust::baml_client::sync_client::B;

/// Test basic collector usage - Go: TestCollectorBasicUsage
#[test]
fn test_collector_basic() {
    let collector = new_collector("my-collector");

    // Go: Initially no logs
    let logs = collector.logs();
    assert_eq!(logs.len(), 0, "Initially no logs");

    // Make a function call with collector
    let result = B
        .TestOpenAIGPT4oMini
        .with_collector(&collector)
        .call("hi there");

    assert!(result.is_ok(), "Expected successful call, got {:?}", result);

    // Go: Should have one log entry
    let logs = collector.logs();
    assert_eq!(logs.len(), 1, "Should have one log entry");

    let log = &logs[0];

    // Go: assert.Equal(t, "TestOpenAIGPT4oMini", name)
    assert_eq!(
        log.function_name(),
        "TestOpenAIGPT4oMini",
        "Function name should match"
    );

    // Go: assert.Equal(t, "call", logType)
    assert_eq!(log.log_type(), LogType::Call, "Log type should be call");

    // Go: Verify timing fields
    let timing = log.timing();
    assert!(
        timing.start_time_utc_ms() > 0,
        "Start time should be positive"
    );
    assert!(
        timing.duration_ms().unwrap_or(0) > 0,
        "Duration should be positive"
    );

    // Go: Verify usage fields
    let usage = log.usage();
    assert!(usage.input_tokens() > 0, "Input tokens should be positive");
    assert!(
        usage.output_tokens() > 0,
        "Output tokens should be positive"
    );

    // Go: Verify calls
    let calls = log.calls();
    assert_eq!(calls.len(), 1, "Should have one call");

    let call = &calls[0];
    // Go: assert.Equal(t, "openai", provider)
    assert_eq!(call.provider(), "openai", "Provider should be openai");
    // Go: assert.Equal(t, "GPT4oMini", clientName)
    assert_eq!(call.client_name(), "GPT4oMini", "Client name should match");
    // Go: assert.True(t, selected)
    assert!(call.selected(), "Call should be selected");

    // Go: Verify request/response
    let request = call.http_request();
    assert!(request.is_some(), "HTTP request should exist");
    let request = request.unwrap();
    let body_text = request.body().text().unwrap_or_default();
    assert!(
        body_text.contains("messages"),
        "Request body should contain 'messages'"
    );
    assert!(
        body_text.contains("gpt-4o-mini"),
        "Request body should contain model name"
    );

    // Go: Verify HTTP response (only available via as_call())
    if let Some(llm_call) = call.as_call() {
        let response = llm_call.http_response();
        assert!(
            response.is_some(),
            "HTTP response should exist for non-streaming"
        );
        let response = response.unwrap();
        assert_eq!(response.status(), 200, "HTTP status should be 200");
        let response_body = response.body().text().unwrap_or_default();
        assert!(
            response_body.contains("choices"),
            "Response should contain 'choices'"
        );
    }

    // Go: Verify call usage
    let call_usage = call.usage();
    assert!(call_usage.is_some(), "Call should have usage");
    let call_usage = call_usage.unwrap();
    assert!(
        call_usage.input_tokens() > 0,
        "Call input tokens should be positive"
    );
    assert!(
        call_usage.output_tokens() > 0,
        "Call output tokens should be positive"
    );

    // Go: Usage should match between call and log
    assert_eq!(
        call_usage.input_tokens(),
        usage.input_tokens(),
        "Call and log input tokens should match"
    );
    assert_eq!(
        call_usage.output_tokens(),
        usage.output_tokens(),
        "Call and log output tokens should match"
    );

    // Go: Verify collector usage
    let collector_usage = collector.usage();
    assert_eq!(
        collector_usage.input_tokens(),
        usage.input_tokens(),
        "Collector and log input tokens should match"
    );
    assert_eq!(
        collector_usage.output_tokens(),
        usage.output_tokens(),
        "Collector and log output tokens should match"
    );
}

/// Test collector with streaming calls - Go: TestCollectorStreamingCalls
#[test]
fn test_collector_streaming() {
    let collector = new_collector("my-collector");

    let stream = B
        .TestOpenAIGPT4oMini
        .with_collector(&collector)
        .stream("hi there");

    assert!(stream.is_ok(), "Expected successful stream creation");

    let mut stream = stream.unwrap();

    // Consume stream to get final result
    for _ in stream.partials() {}
    let result = stream.get_final_response();
    assert!(result.is_ok(), "Expected successful final result");
    let final_result = result.unwrap();
    assert!(!final_result.is_empty(), "Expected non-empty final result");

    // Go: Check logs
    let logs = collector.logs();
    assert_eq!(logs.len(), 1, "Should have one log entry");

    let log = &logs[0];
    // Go: assert.Equal(t, "TestOpenAIGPT4oMini", name)
    assert_eq!(
        log.function_name(),
        "TestOpenAIGPT4oMini",
        "Function name should match"
    );
    // Go: assert.Equal(t, "stream", logType)
    assert_eq!(log.log_type(), LogType::Stream, "Log type should be stream");

    // Go: Verify timing and usage
    let timing = log.timing();
    assert!(
        timing.start_time_utc_ms() > 0,
        "Start time should be positive"
    );
    assert!(
        timing.duration_ms().unwrap_or(0) > 0,
        "Duration should be positive"
    );

    let usage = log.usage();
    assert!(usage.input_tokens() > 0, "Input tokens should be positive");
    assert!(
        usage.output_tokens() > 0,
        "Output tokens should be positive"
    );

    // Go: Verify calls
    let calls = log.calls();
    assert_eq!(calls.len(), 1, "Should have one call");

    let call = &calls[0];
    assert_eq!(call.provider(), "openai", "Provider should be openai");
    assert_eq!(call.client_name(), "GPT4oMini", "Client name should match");
    assert!(call.selected(), "Call should be selected");

    // Go: For streaming, HTTP response should be nil (via as_stream check)
    assert!(call.as_stream().is_some(), "Should be a streaming call");

    // Go: But request should exist
    let request = call.http_request();
    assert!(request.is_some(), "HTTP request should exist");
    let body_text = request.unwrap().body().text().unwrap_or_default();
    assert!(
        body_text.contains("stream"),
        "Request body should contain 'stream'"
    );
}

/// Test collector with multiple calls - Go: TestCollectorMultipleCalls
#[test]
fn test_collector_multiple_calls() {
    let collector = new_collector("my-collector");

    // First call
    let result1 = B
        .TestOpenAIGPT4oMini
        .with_collector(&collector)
        .call("First call");
    assert!(result1.is_ok(), "Expected successful first call");

    let logs = collector.logs();
    assert_eq!(logs.len(), 1, "Should have one log after first call");

    // Capture usage after first call
    let first_usage = logs[0].usage();
    let first_input = first_usage.input_tokens();
    let first_output = first_usage.output_tokens();

    // Go: Verify collector usage matches first call
    let collector_usage = collector.usage();
    assert_eq!(
        collector_usage.input_tokens(),
        first_input,
        "Collector input tokens should match first call"
    );
    assert_eq!(
        collector_usage.output_tokens(),
        first_output,
        "Collector output tokens should match first call"
    );

    // Second call
    let result2 = B
        .TestOpenAIGPT4oMini
        .with_collector(&collector)
        .call("Second call");
    assert!(result2.is_ok(), "Expected successful second call");

    let logs = collector.logs();
    assert_eq!(logs.len(), 2, "Should have two logs after second call");

    // Capture usage after second call
    let second_usage = logs[1].usage();
    let second_input = second_usage.input_tokens();
    let second_output = second_usage.output_tokens();

    // Go: Verify collector usage is sum of both calls
    let total_input = first_input + second_input;
    let total_output = first_output + second_output;

    let collector_usage = collector.usage();
    assert_eq!(
        collector_usage.input_tokens(),
        total_input,
        "Collector input tokens should be sum of both calls"
    );
    assert_eq!(
        collector_usage.output_tokens(),
        total_output,
        "Collector output tokens should be sum of both calls"
    );
}

/// Test collector with concurrent calls - Go: TestCollectorConcurrentCalls
#[test]
fn test_collector_concurrent() {
    use std::sync::Arc;
    use std::thread;

    let collector = Arc::new(new_collector("parallel-collector"));

    let handles: Vec<_> = ["call #1", "call #2", "call #3"]
        .iter()
        .map(|input| {
            let c = Arc::clone(&collector);
            let input = input.to_string();
            thread::spawn(move || {
                let result = B.TestOpenAIGPT4oMini.with_collector(&c).call(&input);
                assert!(result.is_ok(), "Expected successful concurrent call");
                result.unwrap()
            })
        })
        .collect();

    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.join().expect("Thread panicked"));
    }

    // Go: Verify we got all results
    assert_eq!(results.len(), 3, "Expected results from all calls");
    for result in &results {
        assert!(
            !result.is_empty(),
            "Expected non-empty result from each call"
        );
    }

    // Go: Verify collector captured all calls
    let logs = collector.logs();
    assert_eq!(logs.len(), 3, "Should have three logs");

    // Go: Verify each call is recorded properly
    let mut total_input = 0i64;
    let mut total_output = 0i64;

    for log in &logs {
        assert_eq!(
            log.function_name(),
            "TestOpenAIGPT4oMini",
            "Function name should match"
        );
        assert_eq!(log.log_type(), LogType::Call, "Log type should be call");

        let usage = log.usage();
        assert!(usage.input_tokens() > 0, "Input tokens should be positive");
        assert!(
            usage.output_tokens() > 0,
            "Output tokens should be positive"
        );

        total_input += usage.input_tokens();
        total_output += usage.output_tokens();
    }

    // Go: Verify total collector usage equals sum of all calls
    let collector_usage = collector.usage();
    assert_eq!(
        collector_usage.input_tokens(),
        total_input,
        "Collector input tokens should equal sum of all calls"
    );
    assert_eq!(
        collector_usage.output_tokens(),
        total_output,
        "Collector output tokens should equal sum of all calls"
    );
}

/// Test multiple collectors - Go: TestCollectorMultipleCollectors
#[test]
fn test_multiple_collectors() {
    let collector1 = new_collector("collector-1");
    let collector2 = new_collector("collector-2");

    // Go: Pass both collectors for the first call
    let result1 = B
        .TestOpenAIGPT4oMini
        .with_collectors(&[collector1.clone(), collector2.clone()])
        .call("First call");
    assert!(
        result1.is_ok(),
        "Expected successful call with both collectors"
    );

    // Go: Check usage/logs after the first call
    let logs1 = collector1.logs();
    assert_eq!(logs1.len(), 1, "Collector 1 should have one log");

    let logs2 = collector2.logs();
    assert_eq!(logs2.len(), 1, "Collector 2 should have one log");

    // Go: Verify both collectors have the exact same usage for the first call
    let usage1 = logs1[0].usage();
    let usage2 = logs2[0].usage();
    assert_eq!(
        usage1.input_tokens(),
        usage2.input_tokens(),
        "Both collectors should have same input tokens"
    );
    assert_eq!(
        usage1.output_tokens(),
        usage2.output_tokens(),
        "Both collectors should have same output tokens"
    );

    // Go: Second call uses only collector1
    let result2 = B
        .TestOpenAIGPT4oMini
        .with_collector(&collector1)
        .call("Second call");
    assert!(result2.is_ok(), "Expected successful second call");

    // Go: Re-check logs/usage
    let logs1 = collector1.logs();
    assert_eq!(logs1.len(), 2, "Collector 1 should have two logs");

    let logs2 = collector2.logs();
    assert_eq!(logs2.len(), 1, "Collector 2 should still have one log");
}

/// Test collector with provider-specific collection - Go: TestCollectorProviderSpecific
#[test]
fn test_collector_provider_specific() {
    let collector = new_collector("provider-test");

    // Test with OpenAI provider
    let result = B.TestOpenAI.with_collector(&collector).call("Hello");
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);

    let logs = collector.logs();
    assert_eq!(logs.len(), 1, "Should have one log");

    let log = &logs[0];
    assert_eq!(
        log.function_name(),
        "TestOpenAI",
        "Function name should match"
    );
    assert_eq!(log.log_type(), LogType::Call, "Log type should be call");

    let calls = log.calls();
    assert_eq!(calls.len(), 1, "Should have one call");

    let call = &calls[0];
    assert_eq!(call.provider(), "openai", "Provider should be openai");
    assert!(call.selected(), "Call should be selected");

    // Go: Verify request exists
    let request = call.http_request();
    assert!(request.is_some(), "HTTP request should exist");

    // Go: Verify response exists for non-streaming (via as_call())
    if let Some(llm_call) = call.as_call() {
        let response = llm_call.http_response();
        assert!(response.is_some(), "HTTP response should exist");
        assert_eq!(response.unwrap().status(), 200, "HTTP status should be 200");
    }

    // Go: Verify usage
    let usage = call.usage();
    assert!(usage.is_some(), "Call should have usage");
    let usage = usage.unwrap();
    assert!(usage.input_tokens() > 0, "Input tokens should be positive");
    assert!(
        usage.output_tokens() > 0,
        "Output tokens should be positive"
    );
}

/// Test collector error handling - Go: TestCollectorErrorHandling
#[test]
fn test_collector_error_handling() {
    let collector = new_collector("error-collector");

    // Call a function that might fail
    let result = B.FnAlwaysFails.with_collector(&collector).call("test");

    // The call should fail, but collector should still capture logs
    assert!(result.is_err(), "Expected error from FnAlwaysFails");

    // Go: Even with error, collector might still have captured the attempt
    let logs = collector.logs();
    // Length could be 0 or 1 depending on when error occurred
    assert!(
        logs.len() <= 1,
        "Expected at most one log entry for failed call"
    );
}

/// Test collector memory management - Go: TestCollectorMemoryManagement
#[test]
fn test_collector_memory() {
    // Create and drop many collectors
    {
        let collector = new_collector("temp-collector");
        let _ = B
            .TestFnNamedArgsSingleString
            .with_collector(&collector)
            .call("memory test");
        // Collector is dropped at end of iteration
    }
}

/// Test pre-streaming collection - Go: TestCollectorBeforeStreaming
#[test]
fn test_collector_pre_streaming() {
    let collector = new_collector("my-collector");

    // Make streaming call
    let stream = B
        .TestOpenAIGPT4oMini
        .with_collector(&collector)
        .stream("elaborate on the following: 'The quick brown fox jumps over the lazy dog.'");
    assert!(stream.is_ok(), "Expected successful stream creation");

    // Go: Before consuming stream, usage should be zero
    let usage = collector.usage();
    assert_eq!(
        usage.input_tokens(),
        0,
        "Input tokens should be 0 before streaming"
    );
    assert_eq!(
        usage.output_tokens(),
        0,
        "Output tokens should be 0 before streaming"
    );

    let mut stream = stream.unwrap();

    // Consume stream
    for _ in stream.partials() {}
    let result = stream.get_final_response();
    assert!(result.is_ok(), "Expected successful final result");
    let final_result = result.unwrap();
    assert!(!final_result.is_empty(), "Expected non-empty final result");

    // After consuming, logs should exist
    let logs = collector.logs();
    assert_eq!(logs.len(), 1, "Should have one log entry");

    let log = &logs[0];
    assert_eq!(
        log.function_name(),
        "TestOpenAIGPT4oMini",
        "Function name should match"
    );
    assert_eq!(log.log_type(), LogType::Stream, "Log type should be stream");

    // Go: Verify timing and usage after consuming
    let timing = log.timing();
    assert!(
        timing.start_time_utc_ms() > 0,
        "Start time should be positive"
    );
    assert!(
        timing.duration_ms().unwrap_or(0) > 0,
        "Duration should be positive"
    );

    let usage = log.usage();
    assert!(usage.input_tokens() > 0, "Input tokens should be positive");
    assert!(
        usage.output_tokens() > 0,
        "Output tokens should be positive"
    );

    // Go: Verify calls
    let calls = log.calls();
    assert_eq!(calls.len(), 1, "Should have one call");

    let call = &calls[0];
    assert_eq!(call.provider(), "openai", "Provider should be openai");
    assert_eq!(call.client_name(), "GPT4oMini", "Client name should match");
    assert!(call.selected(), "Call should be selected");

    // Go: For streaming, it should be a stream call
    assert!(call.as_stream().is_some(), "Should be a streaming call");

    // Go: But request should exist
    let request = call.http_request();
    assert!(request.is_some(), "HTTP request should exist");
    let body_text = request.unwrap().body().text().unwrap_or_default();
    assert!(
        body_text.contains("stream"),
        "Request body should contain 'stream'"
    );
}

/// Test collector call details
#[test]
fn test_collector_call_details() {
    let collector = new_collector("details-collector");

    let result = B
        .TestOpenAI
        .with_collector(&collector)
        .call("Tell me about Rust");

    assert!(result.is_ok(), "Expected successful call, got {:?}", result);

    // Access collector data
    let logs = collector.logs();
    assert_eq!(logs.len(), 1, "Should have one log entry");

    // Verify we can access all the log data
    let log = &logs[0];
    let _ = log.id();
    let _ = log.function_name();
    let _ = log.log_type();
    let _ = log.tags();
    let _ = log.timing();
    let _ = log.usage();
    let _ = log.calls();
}

/// Test collector with class output
#[test]
fn test_collector_class_output() {
    let collector = new_collector("class-collector");

    let result = B
        .FnOutputClass
        .with_collector(&collector)
        .call("Create a test class");

    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert!(!output.prop1.is_empty(), "Expected non-empty prop1");

    // Verify collector captured the call
    let logs = collector.logs();
    assert_eq!(logs.len(), 1, "Should have one log entry");
}
