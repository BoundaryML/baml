package main

import (
	"context"
	"runtime"
	"testing"
	"time"

	b "example.com/integ-tests/baml_client"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestMemoryPerformance tests memory usage and performance characteristics
// Reference: General performance testing patterns

// TestMemoryUsageBasic tests basic memory usage patterns
func TestMemoryUsageBasic(t *testing.T) {
	ctx := context.Background()
	
	// Get baseline memory stats
	var m1 runtime.MemStats
	runtime.GC()
	runtime.ReadMemStats(&m1)
	
	// Perform some BAML operations
	for i := 0; i < 10; i++ {
		result, err := b.TestOpenAIGPT4oMini(ctx, "memory test")
		require.NoError(t, err)
		assert.NotEmpty(t, result)
	}
	
	// Get memory stats after operations
	var m2 runtime.MemStats
	runtime.GC()
	runtime.ReadMemStats(&m2)
	
	// Log memory usage for observation
	allocDiff := m2.TotalAlloc - m1.TotalAlloc
	t.Logf("Memory allocated during test: %d bytes", allocDiff)
	t.Logf("Heap objects: %d -> %d", m1.HeapObjects, m2.HeapObjects)
	
	// Basic sanity check - shouldn't use excessive memory for simple calls
	assert.Less(t, allocDiff, uint64(100*1024*1024), "Shouldn't allocate more than 100MB for basic operations")
}

// TestMemoryUsageWithCollector tests memory usage with collector
func TestMemoryUsageWithCollector(t *testing.T) {
	ctx := context.Background()
	
	// Get baseline memory
	var m1 runtime.MemStats
	runtime.GC()
	runtime.ReadMemStats(&m1)
	
	collector, err := b.NewCollector("memory-test")
	require.NoError(t, err)
	
	// Perform operations with collector
	for i := 0; i < 5; i++ {
		result, err := b.TestOpenAIGPT4oMini(ctx, "collector memory test", b.WithCollector(collector))
		require.NoError(t, err)
		assert.NotEmpty(t, result)
	}
	
	// Check collector captured data
	logs, err := collector.Logs()
	require.NoError(t, err)
	assert.Len(t, logs, 5)
	
	// Get memory after operations
	var m2 runtime.MemStats
	runtime.GC()
	runtime.ReadMemStats(&m2)
	
	allocDiff := m2.TotalAlloc - m1.TotalAlloc
	t.Logf("Memory allocated with collector: %d bytes", allocDiff)
	
	// Collector should not cause excessive memory usage
	assert.Less(t, allocDiff, uint64(50*1024*1024), "Collector shouldn't cause excessive memory usage")
}

// TestMemoryLeakDetection tests for potential memory leaks
func TestMemoryLeakDetection(t *testing.T) {
	ctx := context.Background()
	
	// This test runs multiple iterations to check for memory leaks
	const iterations = 20
	var memStats []uint64
	
	for i := 0; i < iterations; i++ {
		// Perform operations
		result, err := b.TestOpenAIGPT4oMini(ctx, "leak test")
		require.NoError(t, err)
		assert.NotEmpty(t, result)
		
		// Force GC and measure memory every few iterations
		if i%5 == 0 {
			runtime.GC()
			var m runtime.MemStats
			runtime.ReadMemStats(&m)
			memStats = append(memStats, m.HeapInuse)
			t.Logf("Iteration %d: Heap in use: %d bytes", i, m.HeapInuse)
		}
	}
	
	// Check that memory usage doesn't grow linearly with iterations
	// (which would indicate a memory leak)
	if len(memStats) >= 2 {
		firstMem := memStats[0]
		lastMem := memStats[len(memStats)-1]
		growth := float64(lastMem) / float64(firstMem)
		
		// Memory shouldn't grow by more than 3x during iterations
		assert.Less(t, growth, 3.0, "Memory usage grew too much: %.2fx", growth)
		t.Logf("Memory growth factor: %.2fx", growth)
	}
}

// TestPerformanceBaseline tests basic performance characteristics
func TestPerformanceBaseline(t *testing.T) {
	ctx := context.Background()
	
	// Measure performance of basic operations
	start := time.Now()
	result, err := b.TestOpenAIGPT4oMini(ctx, "performance baseline test")
	duration := time.Since(start)
	
	require.NoError(t, err)
	assert.NotEmpty(t, result)
	
	t.Logf("Basic call took: %v", duration)
	
	// Basic sanity check - shouldn't take excessively long for simple calls
	// (This depends on network conditions, so we set a generous limit)
	assert.Less(t, duration, 30*time.Second, "Basic call shouldn't take more than 30 seconds")
}

// TestConcurrentPerformance tests performance under concurrent load
func TestConcurrentPerformance(t *testing.T) {
	ctx := context.Background()
	
	const numConcurrent = 3
	results := make(chan time.Duration, numConcurrent)
	
	start := time.Now()
	
	// Start concurrent operations
	for i := 0; i < numConcurrent; i++ {
		go func(id int) {
			opStart := time.Now()
			result, err := b.TestOpenAIGPT4oMini(ctx, "concurrent performance test")
			opDuration := time.Since(opStart)
			
			if err != nil {
				t.Errorf("Concurrent operation %d failed: %v", id, err)
				return
			}
			
			assert.NotEmpty(t, result)
			results <- opDuration
		}(i)
	}
	
	// Collect results
	var durations []time.Duration
	for i := 0; i < numConcurrent; i++ {
		select {
		case duration := <-results:
			durations = append(durations, duration)
		case <-time.After(60 * time.Second):
			t.Fatalf("Timeout waiting for concurrent operation %d", i)
		}
	}
	
	totalDuration := time.Since(start)
	
	// Log performance metrics
	var totalOp time.Duration
	for i, d := range durations {
		t.Logf("Operation %d took: %v", i, d)
		totalOp += d
	}
	
	avgOpDuration := totalOp / time.Duration(len(durations))
	t.Logf("Average operation duration: %v", avgOpDuration)
	t.Logf("Total test duration: %v", totalDuration)
	
	// Concurrent operations should complete in reasonable time
	assert.Less(t, totalDuration, 60*time.Second, "Concurrent operations took too long")
}

// TestStreamingPerformance tests streaming operation performance
func TestStreamingPerformance(t *testing.T) {
	ctx := context.Background()
	
	start := time.Now()
	stream, err := b.Stream.TestOpenAIGPT4oMini(ctx, "streaming performance test")
	require.NoError(t, err)
	
	var chunkCount int
	var firstChunkTime time.Duration
	var final string
	
	for value := range stream {
		if value.IsError {
			t.Fatalf("Stream error: %v", value.Error)
		}
		
		if !value.IsFinal && value.Stream() != nil {
			if chunkCount == 0 {
				firstChunkTime = time.Since(start)
			}
			chunkCount++
		}
		
		if value.IsFinal && value.Final() != nil {
			final = *value.Final()
		}
	}
	
	totalDuration := time.Since(start)
	
	assert.NotEmpty(t, final)
	assert.Greater(t, chunkCount, 0, "Expected at least some streaming chunks")
	
	t.Logf("Streaming completed in: %v", totalDuration)
	t.Logf("First chunk received in: %v", firstChunkTime)
	t.Logf("Total chunks received: %d", chunkCount)
	
	// First chunk should arrive relatively quickly
	assert.Less(t, firstChunkTime, 10*time.Second, "First chunk should arrive quickly")
}

// TestLargeInputPerformance tests performance with larger inputs
func TestLargeInputPerformance(t *testing.T) {
	ctx := context.Background()
	
	// Test with progressively larger inputs
	inputs := []struct {
		name string
		text string
	}{
		{"Small", "Small input text"},
		{"Medium", "This is a medium sized input text that contains more content to test how the system handles different input sizes and whether performance scales appropriately with input length."},
		{"Large", "This is a large input text that contains substantially more content to test how the system handles large inputs. " +
			"It includes multiple sentences and paragraphs to simulate real-world usage scenarios where users might provide " +
			"extensive context or detailed requirements. The purpose is to understand performance characteristics as input " +
			"size increases and to ensure the system can handle various content lengths efficiently without degrading " +
			"performance significantly. This helps establish baseline performance expectations for different use cases."},
	}
	
	for _, tc := range inputs {
		t.Run(tc.name, func(t *testing.T) {
			start := time.Now()
			result, err := b.TestOpenAIGPT4oMini(ctx, tc.text)
			duration := time.Since(start)
			
			require.NoError(t, err)
			assert.NotEmpty(t, result)
			
			t.Logf("%s input (%d chars) took: %v", tc.name, len(tc.text), duration)
			
			// Performance should scale reasonably with input size
			assert.Less(t, duration, 45*time.Second, "%s input took too long", tc.name)
		})
	}
}

// TestMemoryWithTypeBuilder tests memory usage with TypeBuilder
func TestMemoryWithTypeBuilder(t *testing.T) {
	ctx := context.Background()
	
	var m1 runtime.MemStats
	runtime.GC()
	runtime.ReadMemStats(&m1)
	
	// Create and use TypeBuilder
	tb, err := b.NewTypeBuilder()
	require.NoError(t, err)
	
	// Add several properties to test memory impact
	personClass, err := tb.Person()
	require.NoError(t, err)
	
	stringType, err := tb.String()
	require.NoError(t, err)
	
	for i := 0; i < 5; i++ {
		_, err = personClass.AddProperty("dynamic_prop_"+string(rune('a'+i)), stringType)
		require.NoError(t, err)
	}
	
	// Use the TypeBuilder
	result, err := b.ExtractPeople(ctx, "My name is TypeBuilder Test", b.WithTypeBuilder(tb))
	require.NoError(t, err)
	assert.NotEmpty(t, result)
	
	var m2 runtime.MemStats
	runtime.GC()
	runtime.ReadMemStats(&m2)
	
	allocDiff := m2.TotalAlloc - m1.TotalAlloc
	t.Logf("TypeBuilder memory usage: %d bytes", allocDiff)
	
	// TypeBuilder shouldn't use excessive memory
	assert.Less(t, allocDiff, uint64(10*1024*1024), "TypeBuilder shouldn't use excessive memory")
}

// TestResourceCleanup tests that resources are properly cleaned up
func TestResourceCleanup(t *testing.T) {
	ctx := context.Background()
	
	// Create multiple collectors in succession to test cleanup
	var collectors []interface{}
	
	for i := 0; i < 5; i++ {
		collector, err := b.NewCollector("cleanup-test-" + string(rune('a'+i)))
		require.NoError(t, err)
		
		// Use the collector
		result, err := b.TestOpenAIGPT4oMini(ctx, "cleanup test", b.WithCollector(collector))
		require.NoError(t, err)
		assert.NotEmpty(t, result)
		
		collectors = append(collectors, collector)
	}
	
	// Force GC to clean up any resources
	collectors = nil // Remove references
	runtime.GC()
	runtime.GC() // Run twice to be thorough
	
	// Additional operations should still work fine
	result, err := b.TestOpenAIGPT4oMini(ctx, "post cleanup test")
	require.NoError(t, err)
	assert.NotEmpty(t, result)
	
	t.Logf("Resource cleanup test completed successfully")
}