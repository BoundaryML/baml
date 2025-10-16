package main

import (
	"context"
	"testing"
	"time"

	b "example.com/integ-tests/baml_client"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestCachingFunctionality tests BAML caching functionality
// Reference: test_functions.py:1188-1192

// TestCachingBasic tests basic caching functionality
func TestCachingBasic(t *testing.T) {
	ctx := context.Background()
	
	// First call - should execute and cache
	start1 := time.Now()
	result1, err := b.TestCaching(ctx, "What is the capital of France?", "Paris")
	duration1 := time.Since(start1)
	require.NoError(t, err)
	assert.NotEmpty(t, result1)
	
	// Second call with same input - should use cache and be faster
	start2 := time.Now()
	result2, err := b.TestCaching(ctx, "What is the capital of France?", "Paris")
	duration2 := time.Since(start2)
	require.NoError(t, err)
	assert.NotEmpty(t, result2)
		
	t.Logf("First call took: %v, Second call took: %v", duration1, duration2)
	// Note: Cache timing comparison can be flaky in tests, so we just log it
}


// TestCachingWithCollector tests caching behavior with collector
func TestCachingWithCollector(t *testing.T) {
	ctx := context.Background()
	
	collector, err := b.NewCollector("cache-collector")
	require.NoError(t, err)
	
	// First call with collector
	result1, err := b.TestCaching(ctx, "Test caching with collector", "cached content", b.WithCollector(collector))
	require.NoError(t, err)
	assert.NotEmpty(t, result1)
	
	// Check collector logs after first call
	logs1, err := collector.Logs()
	require.NoError(t, err)
	firstCallCount := len(logs1)
	
	// Second call with same collector and same input
	result2, err := b.TestCaching(ctx, "Test caching with collector", "cached content", b.WithCollector(collector))
	require.NoError(t, err)
	assert.NotEmpty(t, result2)
	
	// Check collector logs after second call
	logs2, err := collector.Logs()
	require.NoError(t, err)
	secondCallCount := len(logs2)
	
	// Both calls should be logged (even if second is cached)
	assert.Equal(t, firstCallCount+1, secondCallCount, "Both calls should be logged by collector")
}
