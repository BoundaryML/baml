//! Modular API tests - ported from test_modular_api_test.go
//!
//! Tests for modular API patterns including:
//! - Configuration options
//! - Request/response patterns
//! - Parse API patterns
//! - Stream API patterns

use rust::baml_client::new_collector;
use rust::baml_client::sync_client::B;

/// Test configuration options - Go: TestConfigurationOptions
#[test]
fn test_configuration_options() {
    // Sub-test 1: with_collector
    let collector = new_collector("config-test");
    let result = B
        .TestFnNamedArgsSingleString
        .with_collector(&collector)
        .call("test");
    assert!(result.is_ok(), "Expected successful call with collector");
    let logs = collector.logs();
    assert_eq!(logs.len(), 1, "Expected one log entry");
    assert_eq!(
        logs[0].function_name(),
        "TestFnNamedArgsSingleString",
        "Expected correct function name"
    );

    // Sub-test 2: with_env_var
    let result = B
        .TestFnNamedArgsSingleString
        .with_env_var("SOME_VAR", "value")
        .call("test");
    assert!(result.is_ok(), "Expected successful call with env var");

    // Sub-test 3: with_tag
    let collector2 = new_collector("tag-test");
    let result = B
        .TestFnNamedArgsSingleString
        .with_collector(&collector2)
        .with_tag("key", "value")
        .call("test");
    assert!(result.is_ok(), "Expected successful call with tag");
}

/// Test request/response patterns - Go: TestRequestResponsePatterns
#[test]
fn test_request_response_patterns() {
    // Sub-test 1: Basic call
    let call_result = B.TestFnNamedArgsSingleString.call("test");
    assert!(call_result.is_ok(), "Expected successful call");
    let output = call_result.unwrap();
    assert!(!output.is_empty(), "Expected non-empty output");

    // Sub-test 2: Parse API
    let parse_result = B.FnOutputBool.parse("true");
    assert!(parse_result.is_ok(), "Expected successful parse");
    assert_eq!(parse_result.unwrap(), true);

    // Sub-test 3: Parse with linked list (deep verification)
    let json = r#"{"len": 5, "head": {"data": 1, "next": {"data": 2, "next": null}}}"#;
    let result = B.BuildLinkedList.parse(json);
    assert!(result.is_ok(), "Expected successful linked list parse");
    let list = result.unwrap();
    assert_eq!(list.len, 5, "Expected len to be 5");
    assert!(list.head.is_some(), "Expected head to be present");
    let head = list.head.as_ref().unwrap();
    assert_eq!(head.data, 1, "Expected head.data to be 1");

    // Sub-test 4: Error case
    let bad_result = B.FnOutputBool.parse("not a bool");
    assert!(
        bad_result.is_err(),
        "Expected parse error for invalid input"
    );
}

/// Test parse API patterns
#[test]
fn test_parse_api_patterns() {
    // Parse final
    let result = B.FnOutputClass.parse(r#"{"prop1": "value", "prop2": 42}"#);
    assert!(result.is_ok(), "Expected successful parse");

    // Parse stream
    let stream_result = B.FnOutputClass.parse_stream(r#"{"prop1": "partial"}"#);
    assert!(stream_result.is_ok(), "Expected successful stream parse");
}

/// Test stream API patterns - Go: TestStreamAPI
#[test]
fn test_stream_api_patterns() {
    let collector = new_collector("stream-test");

    let stream = B
        .PromptTestStreaming
        .with_collector(&collector)
        .stream("test");
    // StreamingCall doesn't implement Debug, so check is_ok without unwrap in message
    assert!(stream.is_ok(), "Expected successful stream creation");

    let mut stream = stream.unwrap();
    // Consume partials
    for _ in stream.partials() {}

    let result = stream.get_final_response();
    assert!(result.is_ok(), "Expected successful final result");

    // Go: Verify log type = "stream"
    let logs = collector.logs();
    assert_eq!(logs.len(), 1, "Expected one log entry");
    assert_eq!(
        logs[0].log_type(),
        baml::LogType::Stream,
        "Expected 'stream' log type"
    );
}

/// Test API consistency - Go: TestAPIConsistency
#[test]
fn test_api_consistency() {
    // Test sync and stream return same value
    let sync_result = B.ExtractNames.call("My name is Charlie");
    assert!(sync_result.is_ok(), "Expected successful sync call");
    let sync_names = sync_result.unwrap();
    assert!(
        sync_names.iter().any(|n| n.contains("Charlie")),
        "Expected Charlie in sync result"
    );

    let stream = B.ExtractNames.stream("My name is Charlie");
    assert!(stream.is_ok(), "Expected successful stream creation");
    let mut stream = stream.unwrap();
    // Consume partials
    for _ in stream.partials() {}
    let stream_result = stream.get_final_response();
    assert!(
        stream_result.is_ok(),
        "Expected successful stream final result"
    );
    let stream_names = stream_result.unwrap();
    assert!(
        stream_names.iter().any(|n| n.contains("Charlie")),
        "Expected Charlie in stream result"
    );
}

/// Test error handling patterns - Go: TestErrorHandling
#[test]
fn test_error_handling_patterns() {
    // Sub-test 1: Invalid input
    let result = B.FnOutputBool.parse("invalid");
    assert!(result.is_err(), "Expected parse error");
    let err = format!("{:?}", result.unwrap_err());
    assert!(
        err.contains("parse") || err.contains("coerce"),
        "Expected parse error, got: {}",
        err
    );

    // Sub-test 2: Empty input
    let result = B.FnOutputBool.parse("");
    assert!(result.is_err(), "Expected error for empty input");

    // Sub-test 3: Function that always fails
    let result = B.FnAlwaysFails.call("test");
    assert!(result.is_err(), "Expected error from FnAlwaysFails");

    // Error should be inspectable
    if let Err(e) = result {
        let _ = format!("{}", e);
        let _ = format!("{:?}", e);
    }
}

/// Test configuration patterns
#[test]
fn test_configuration_patterns() {
    // Chain multiple options
    let collector = new_collector("multi-config-test");

    let result = B
        .TestFnNamedArgsSingleString
        .with_collector(&collector)
        .with_tag("key1", "value1")
        .with_tag("key2", "value2")
        .with_env_var("ENV_KEY", "env_value")
        .call("multi-config test");

    assert!(
        result.is_ok(),
        "Expected successful call with multiple configs"
    );
}

/// Test async patterns (simulated with threads)
#[test]
fn test_async_patterns() {
    use std::thread;

    let handles: Vec<_> = (0..3)
        .map(|i| thread::spawn(move || B.TestFnNamedArgsSingleString.call(format!("async {}", i))))
        .collect();

    for handle in handles {
        let result = handle.join().expect("Thread panicked");
        assert!(result.is_ok(), "Expected successful async call");
    }
}
