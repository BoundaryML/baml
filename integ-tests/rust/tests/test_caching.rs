//! Caching tests - ported from test_caching_test.go
//!
//! Tests for caching functionality including:
//! - Basic caching
//! - Caching with collector

use rust::baml_client::new_collector;
use rust::baml_client::sync_client::B;
use std::time::Instant;

/// Test basic caching - Go: TestCachingBasic
#[test]
fn test_basic_caching() {
    // First call - should execute and cache
    let start1 = Instant::now();
    let result1 = B
        .TestCaching
        .call("What is the capital of France?", "Paris");
    let duration1 = start1.elapsed();
    assert!(
        result1.is_ok(),
        "Expected successful first caching call, got {:?}",
        result1
    );
    let output1 = result1.unwrap();
    assert!(!output1.is_empty(), "Expected non-empty result");

    // Second call with same input - should use cache
    let start2 = Instant::now();
    let result2 = B
        .TestCaching
        .call("What is the capital of France?", "Paris");
    let duration2 = start2.elapsed();
    assert!(
        result2.is_ok(),
        "Expected successful second caching call, got {:?}",
        result2
    );
    let output2 = result2.unwrap();
    assert!(!output2.is_empty(), "Expected non-empty result");

    // Go: Note: Cache timing comparison can be flaky in tests, so we just log it
    eprintln!(
        "First call took: {:?}, Second call took: {:?}",
        duration1, duration2
    );
}

/// Test caching with collector - Go: TestCachingWithCollector
#[test]
fn test_caching_with_collector() {
    let collector = new_collector("cache-collector");

    // First call with collector
    let result1 = B
        .TestCaching
        .with_collector(&collector)
        .call("Test caching with collector", "cached content");
    assert!(
        result1.is_ok(),
        "Expected successful first caching with collector, got {:?}",
        result1
    );
    let output1 = result1.unwrap();
    assert!(!output1.is_empty(), "Expected non-empty result");

    // Check collector logs after first call
    let logs1 = collector.logs();
    let first_call_count = logs1.len();
    assert!(
        first_call_count >= 1,
        "Expected at least one log after first call"
    );

    // Second call with same collector and same input
    let result2 = B
        .TestCaching
        .with_collector(&collector)
        .call("Test caching with collector", "cached content");
    assert!(
        result2.is_ok(),
        "Expected successful second caching with collector, got {:?}",
        result2
    );
    let output2 = result2.unwrap();
    assert!(!output2.is_empty(), "Expected non-empty result");

    // Check collector logs after second call
    let logs2 = collector.logs();
    let second_call_count = logs2.len();

    // Go: Both calls should be logged (even if second is cached)
    assert_eq!(
        second_call_count,
        first_call_count + 1,
        "Both calls should be logged by collector"
    );
}
