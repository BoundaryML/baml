//! Streaming function tests - ported from test_functions_streaming_test.go
//!
//! Tests for streaming functionality including:
//! - Basic streaming
//! - Uniterated streaming
//! - Provider-specific streaming
//! - Concurrent streaming
//! - Nested class streaming
//! - Big numbers streaming

use rust::baml_client::sync_client::B;
use std::time::Instant;

/// Test basic streaming - Go: TestBasicStreaming
#[test]
fn test_basic_streaming() {
    let start = Instant::now();
    let stream = B
        .PromptTestStreaming
        .stream("Programming languages are fun to create");
    assert!(stream.is_ok(), "Expected successful stream creation");

    let mut stream = stream.unwrap();
    let mut msgs: Vec<String> = Vec::new();
    let mut first_msg_time = None;
    let mut last_msg_time = None;

    // Iterate through partial results
    for partial in stream.partials() {
        match partial {
            Ok(partial_value) => {
                msgs.push(partial_value);
                if first_msg_time.is_none() {
                    first_msg_time = Some(Instant::now());
                }
                last_msg_time = Some(Instant::now());
            }
            Err(e) => {
                panic!("Unexpected error during streaming: {:?}", e);
            }
        }
    }

    // Get final result
    let final_result = stream.get_final_response();
    assert!(
        final_result.is_ok(),
        "Expected successful final result, got {:?}",
        final_result
    );
    let final_value = final_result.unwrap();

    // Go: Verify timing constraints
    if let Some(first_time) = first_msg_time {
        let to_first = first_time.duration_since(start);
        assert!(
            to_first.as_millis() <= 1500,
            "Expected first message within 1.5 seconds, took {:?}",
            to_first
        );
    }

    if let Some(last_time) = last_msg_time {
        let to_last = last_time.duration_since(start);
        assert!(
            to_last.as_secs() >= 1,
            "Expected last message after 1 second, took {:?}",
            to_last
        );
    }

    // Go: Verify we got streaming responses
    assert!(!final_value.is_empty(), "Expected non-empty final response");
    assert!(!msgs.is_empty(), "Expected at least one streamed response");

    // Go: Verify message continuity (each message should be >= previous length)
    for i in 1..msgs.len() {
        assert!(
            msgs[i].len() >= msgs[i - 1].len(),
            "Expected messages to be continuous and growing"
        );
    }

    // Go: Final message should match last stream message
    if !msgs.is_empty() {
        assert_eq!(
            msgs[msgs.len() - 1],
            final_value,
            "Expected last stream message to match final response"
        );
    }
}

/// Test uniterated streaming - Go: TestStreamingUniterated
#[test]
fn test_uniterated_streaming() {
    let stream = B.PromptTestStreaming.stream("The color blue makes me sad");
    assert!(stream.is_ok(), "Expected successful stream creation");

    let stream = stream.unwrap();
    // Skip directly to final result without iterating
    let final_result = stream.get_final_response();
    assert!(
        final_result.is_ok(),
        "Expected successful final result, got {:?}",
        final_result
    );
    let output = final_result.unwrap();
    assert!(!output.is_empty(), "Expected non-empty output");
}

/// Test streaming with Claude provider - Go: TestStreamingClaude
#[test]
fn test_streaming_claude() {
    let stream = B.PromptTestClaude.stream("Mt Rainier is tall");
    assert!(stream.is_ok(), "Expected successful stream creation");

    let mut stream = stream.unwrap();
    let mut msgs: Vec<String> = Vec::new();

    // Collect partial results
    for partial in stream.partials() {
        if let Ok(msg) = partial {
            msgs.push(msg);
        }
    }

    let final_result = stream.get_final_response();
    assert!(
        final_result.is_ok(),
        "Expected successful final result, got {:?}",
        final_result
    );
    let final_value = final_result.unwrap();

    assert!(!final_value.is_empty(), "Expected non-empty final response");
    assert!(!msgs.is_empty(), "Expected at least one streamed response");

    // Go: Verify message continuity
    for i in 1..msgs.len() {
        assert!(
            msgs[i].len() >= msgs[i - 1].len(),
            "Expected messages to be continuous"
        );
    }

    // Go: Last stream message should match final
    if !msgs.is_empty() {
        assert_eq!(
            msgs[msgs.len() - 1],
            final_value,
            "Expected last stream message to match final response"
        );
    }
}

/// Test streaming with Gemini provider - Go: TestStreamingGemini
#[test]
fn test_streaming_gemini() {
    let stream = B.TestGemini.stream("Dr.Pepper");
    assert!(stream.is_ok(), "Expected successful stream creation");

    let mut stream = stream.unwrap();
    let mut msgs: Vec<String> = Vec::new();

    // Go: Filter out empty messages
    for partial in stream.partials() {
        if let Ok(msg) = partial {
            if !msg.is_empty() {
                msgs.push(msg);
            }
        }
    }

    let final_result = stream.get_final_response();
    assert!(
        final_result.is_ok(),
        "Expected successful final result, got {:?}",
        final_result
    );
    let final_value = final_result.unwrap();

    assert!(!final_value.is_empty(), "Expected non-empty final response");
    assert!(!msgs.is_empty(), "Expected at least one streamed response");

    // Go: Verify message continuity
    for i in 1..msgs.len() {
        assert!(
            msgs[i].len() >= msgs[i - 1].len(),
            "Expected messages to be continuous"
        );
    }

    // Go: Last stream message should match final
    if !msgs.is_empty() {
        assert_eq!(
            msgs[msgs.len() - 1],
            final_value,
            "Expected last stream message to match final response"
        );
    }
}

/// Test concurrent streaming - Go: TestConcurrentStreaming
#[test]
fn test_concurrent_streaming() {
    use std::thread;

    let handles: Vec<_> = [
        "Tell me about Go",
        "Tell me about Python",
        "Tell me about Rust",
    ]
    .iter()
    .map(|input| {
        let input = input.to_string();
        thread::spawn(move || {
            let stream = B.PromptTestStreaming.stream(&input);
            assert!(stream.is_ok(), "Expected successful stream creation");
            let stream = stream.unwrap();
            let result = stream.get_final_response();
            assert!(result.is_ok(), "Expected successful final result");
            let output = result.unwrap();
            assert!(
                !output.is_empty(),
                "Expected non-empty result from each stream"
            );
            output
        })
    })
    .collect();

    let mut results = Vec::new();
    for handle in handles {
        let result = handle.join().expect("Thread panicked");
        results.push(result);
    }

    // Go: assert.Len(t, finalResults, numStreams, "Expected results from all streams")
    assert_eq!(results.len(), 3, "Expected results from all streams");
}

/// Test streaming with nested class output - Go: TestNestedClassStreaming
#[test]
fn test_nested_class_streaming() {
    let stream = B
        .FnOutputClassNested
        .stream("My name is Harrison. My hair is black and I'm 6 feet tall.");
    assert!(stream.is_ok(), "Expected successful stream creation");

    let mut stream = stream.unwrap();
    let mut partial_count = 0;

    for partial in stream.partials() {
        if partial.is_ok() {
            partial_count += 1;
        }
    }

    let final_result = stream.get_final_response();
    assert!(
        final_result.is_ok(),
        "Expected successful final result, got {:?}",
        final_result
    );
    let output = final_result.unwrap();

    // Go: assert.NotEmpty(t, msgs, "Expected at least one streamed response")
    assert!(partial_count > 0, "Expected at least one streamed response");
    // Go: assert.NotEmpty(t, final.Prop1, "Expected final response to have prop1")
    assert!(
        !output.prop1.is_empty(),
        "Expected final response to have prop1"
    );
}

/// Test streaming big numbers - Go: TestStreamBigNumbers
#[test]
fn test_stream_big_numbers() {
    let stream = B.StreamBigNumbers.stream(12);
    assert!(stream.is_ok(), "Expected successful stream creation");

    let mut stream = stream.unwrap();

    // Consume partials
    for _ in stream.partials() {}

    let final_result = stream.get_final_response();
    assert!(
        final_result.is_ok(),
        "Expected successful final result, got {:?}",
        final_result
    );
    let numbers = final_result.unwrap();

    // Verify the structure has the expected fields
    // Go: just verifies partial fields match final when not nil
    let _ = numbers.a;
    let _ = numbers.b;
}

/// Test streaming compound numbers - Go: TestStreamCompoundNumbers
#[test]
fn test_streaming_compound_numbers() {
    let stream = B.StreamingCompoundNumbers.stream(12, false);
    assert!(stream.is_ok(), "Expected successful stream creation");

    let mut stream = stream.unwrap();

    // Consume partials
    for _ in stream.partials() {}

    let final_result = stream.get_final_response();
    assert!(
        final_result.is_ok(),
        "Expected successful final result, got {:?}",
        final_result
    );
    let numbers = final_result.unwrap();

    // Verify nested fields exist
    let _ = numbers.big.a;
    let _ = numbers.big.b;
}

/// Test streaming with verbose output (yapping) - Go: TestStreamingWithYapping
#[test]
fn test_streaming_verbose() {
    let stream = B.StreamingCompoundNumbers.stream(12, true);
    assert!(stream.is_ok(), "Expected successful stream creation");

    let mut stream = stream.unwrap();
    let mut partial_count = 0;

    for partial in stream.partials() {
        if partial.is_ok() {
            partial_count += 1;
        }
    }

    let final_result = stream.get_final_response();
    assert!(
        final_result.is_ok(),
        "Expected successful final result, got {:?}",
        final_result
    );
    let numbers = final_result.unwrap();

    // Go: assert.NotEmpty(t, msgs, "Expected streaming messages even with yapping")
    assert!(
        partial_count > 0,
        "Expected streaming messages even with yapping"
    );
    // Go: assert.NotZero(t, final.Big.A, "Expected final result to have valid big numbers")
    assert!(
        numbers.big.a != 0,
        "Expected final result to have valid big numbers"
    );
    assert!(
        numbers.big.b != 0.0,
        "Expected final result to have valid big numbers"
    );
}
