//! Memory and performance tests - ported from test_memory_performance_test.go
//!
//! Tests for memory and performance including:
//! - Basic memory usage
//! - Memory with collector
//! - Leak detection
//! - Performance baseline
//! - Concurrent performance

use rust::baml_client::new_collector;
use rust::baml_client::sync_client::B;
use rust::baml_client::type_builder::TypeBuilder;
use std::time::Instant;

/// Test basic memory usage
#[test]
fn test_basic_memory_usage() {
    // Make several calls and ensure no obvious memory issues
    for _ in 0..10 {
        let result = B.TestFnNamedArgsSingleString.call("memory test");
        assert!(result.is_ok(), "Expected successful call");
    }
}

/// Test memory with collector
#[test]
fn test_memory_with_collector() {
    for i in 0..10 {
        let collector = new_collector(&format!("mem-collector-{}", i));
        let result = B
            .TestFnNamedArgsSingleString
            .with_collector(&collector)
            .call("collector memory test");
        assert!(result.is_ok(), "Expected successful call");
        // Collector is dropped here
    }
}

/// Test leak detection - many iterations
#[test]
fn test_leak_detection() {
    for i in 0..50 {
        let result = B
            .TestFnNamedArgsSingleString
            .call(format!("leak test {}", i));
        assert!(
            result.is_ok(),
            "Expected successful call at iteration {}",
            i
        );
    }
}

/// Test performance baseline
#[test]
fn test_performance_baseline() {
    let start = Instant::now();

    let result = B.TestFnNamedArgsSingleString.call("performance test");

    let elapsed = start.elapsed();
    assert!(result.is_ok(), "Expected successful call");

    // Log timing (no strict assertions on time)
    eprintln!("Single call took: {:?}", elapsed);
}

/// Test concurrent performance
#[test]
fn test_concurrent_performance() {
    use std::thread;

    let start = Instant::now();

    let handles: Vec<_> = (0..5)
        .map(|i| {
            thread::spawn(move || {
                B.TestFnNamedArgsSingleString
                    .call(format!("concurrent {}", i))
            })
        })
        .collect();

    for handle in handles {
        let result = handle.join().expect("Thread panicked");
        assert!(result.is_ok(), "Expected successful concurrent call");
    }

    let elapsed = start.elapsed();
    eprintln!("5 concurrent calls took: {:?}", elapsed);
}

/// Test streaming performance
#[test]
fn test_streaming_performance() {
    let start = Instant::now();

    let stream = B.PromptTestStreaming.stream("Short story");
    assert!(stream.is_ok(), "Expected successful stream");

    let mut stream = stream.unwrap();
    // Consume partials
    for _ in stream.partials() {}
    let result = stream.get_final_response();
    assert!(result.is_ok(), "Expected successful result");

    let elapsed = start.elapsed();
    eprintln!("Streaming call took: {:?}", elapsed);
}

/// Test large input performance
#[test]
fn test_large_input_performance() {
    let large_input = "x".repeat(1000);

    let start = Instant::now();
    let result = B.TestFnNamedArgsSingleString.call(&large_input);
    let elapsed = start.elapsed();

    assert!(result.is_ok(), "Expected successful call with large input");
    eprintln!("Large input call took: {:?}", elapsed);
}

/// Test memory with type builder
#[test]
fn test_memory_with_type_builder() {
    for _ in 0..10 {
        let tb = TypeBuilder::new();
        // Use the existing schema class accessor, then add property via inner
        let class_builder = tb.DynamicClassOne();
        let _ = class_builder.inner().add_property("test", &tb.string());

        let result = B
            .TestFnNamedArgsSingleString
            .with_type_builder(&tb)
            .call("type builder memory test");

        assert!(result.is_ok(), "Expected successful call");
        // TypeBuilder is dropped here
    }
}

/// Test resource cleanup
#[test]
fn test_resource_cleanup() {
    // Create many collectors and ensure cleanup
    let collectors: Vec<_> = (0..20)
        .map(|i| new_collector(&format!("cleanup-collector-{}", i)))
        .collect();

    for collector in &collectors {
        let result = B
            .TestFnNamedArgsSingleString
            .with_collector(collector)
            .call("cleanup test");
        assert!(result.is_ok(), "Expected successful call");
    }

    // All collectors dropped at end of scope
}
