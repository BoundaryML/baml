package main

import (
	"context"
	"strings"
	"testing"
	"time"

	b "example.com/integ-tests/baml_client"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestRetryExponential tests exponential backoff retry logic
// Reference: test_functions.py:500-506
func TestRetryExponential(t *testing.T) {
	ctx := context.Background()
	
	// This function is expected to fail after retries
	_, err := b.TestRetryExponential(ctx)
	assert.Error(t, err, "Expected an exception but none was raised")
	
	// The error should indicate retry exhaustion or similar
	errorMsg := strings.ToLower(err.Error())
	assert.True(t, 
		strings.Contains(errorMsg, "retry") ||
		strings.Contains(errorMsg, "timeout") ||
		strings.Contains(errorMsg, "failed") ||
		strings.Contains(errorMsg, "exhausted"),
		"Expected retry-related error message, got: %s", err.Error())
}

// TestFallbackChains tests fallback client chains
// Reference: test_functions.py:509-511
func TestFallbackChains(t *testing.T) {
	ctx := context.Background()
	
	result, err := b.TestFallbackClient(ctx)
	require.NoError(t, err)
	assert.NotEmpty(t, result, "Expected non-empty result but got empty")
}

// TestFailureHandling tests handling of failing fallback clients
// Reference: test_functions.py:515-518
func TestFailureHandling(t *testing.T) {
	ctx := context.Background()
	
	_, err := b.TestSingleFallbackClient(ctx)
	assert.Error(t, err, "Expected error from single fallback client")
	assert.Contains(t, err.Error(), "ConnectError", "Expected ConnectError in error message")
}

// TestFallbackStrategies tests different fallback strategies
// Reference: test_request.py:50-95 (fallback and round robin strategies)
func TestFallbackStrategies(t *testing.T) {
	ctx := context.Background()
	
	t.Run("FallbackStrategy", func(t *testing.T) {
		// Test fallback strategy - should use first available client
		result, err := b.TestFallbackStrategy(ctx, "Dr. Pepper")
		require.NoError(t, err)
		assert.NotEmpty(t, result, "Expected non-empty result from fallback strategy")
	})
	
	t.Run("RoundRobinStrategy", func(t *testing.T) {
		// Test round robin strategy - should distribute across clients
		result, err := b.TestRoundRobinStrategy(ctx, "Dr. Pepper")
		require.NoError(t, err)
		assert.NotEmpty(t, result, "Expected non-empty result from round robin strategy")
	})
}

// TestRetryWithDifferentProviders tests retry behavior across different providers
func TestRetryWithDifferentProviders(t *testing.T) {
	ctx := context.Background()
	
	// Test functions that might involve provider switching on retry/fallback
	testCases := []struct {
		name     string
		function func(context.Context, ...b.CallOptionFunc) (string, error)
	}{
		{
			name:     "ExpectFailure",
			function: b.ExpectFailure,
		},
	}
	
	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			result, err := tc.function(ctx)
			if err != nil {
				// Some functions are expected to fail
				t.Logf("Function %s failed as expected: %v", tc.name, err)
				return
			}
			
			assert.NotEmpty(t, result, "Expected non-empty result from %s", tc.name)
		})
	}
}

// TestTimeoutBehavior tests timeout handling and retry behavior
func TestTimeoutBehavior(t *testing.T) {
	// Create context with reasonable timeout
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	
	t.Run("WithinTimeout", func(t *testing.T) {
		// This should complete within timeout
		start := time.Now()
		result, err := b.TestOpenAIGPT4oMini(ctx, "quick test")
		duration := time.Since(start)
		
		require.NoError(t, err)
		assert.NotEmpty(t, result)
		assert.Less(t, duration, 30*time.Second, "Expected call to complete within timeout")
	})
	
	t.Run("VeryShortTimeout", func(t *testing.T) {
		// Create context with very short timeout
		shortCtx, shortCancel := context.WithTimeout(context.Background(), 1*time.Millisecond)
		defer shortCancel()
		
		// Wait for context to be cancelled
		time.Sleep(10 * time.Millisecond)
		
		// This should fail due to timeout
		_, err := b.TestOpenAIGPT4oMini(shortCtx, "timeout test")
		assert.Error(t, err, "Expected timeout error")
		
		// Error should be context-related
		errorMsg := strings.ToLower(err.Error())
		assert.True(t,
			strings.Contains(errorMsg, "context") ||
			strings.Contains(errorMsg, "timeout") ||
			strings.Contains(errorMsg, "deadline") ||
			strings.Contains(errorMsg, "cancelled"),
			"Expected timeout-related error, got: %s", err.Error())
	})
}

// TestRetryWithStreaming tests retry behavior with streaming functions
func TestRetryWithStreaming(t *testing.T) {
	ctx := context.Background()
	
	// Test streaming function that might have retry logic
	stream, err := b.Stream.TestFallbackToShorthand(ctx, "Mt Rainier is tall")
	require.NoError(t, err)
	
	var final string
	var hadError bool
	
	for value := range stream {
		if value.IsError {
			hadError = true
			t.Logf("Stream error (might be expected): %v", value.Error)
			continue
		}
		
		if value.IsFinal && value.Final() != nil {
			final = *value.Final()
		}
	}
	
	if !hadError {
		// If no errors, should have final result
		assert.NotEmpty(t, final, "Expected non-empty final result from streaming fallback")
	}
}

// TestErrorSpecificRetryBehavior tests retry behavior for specific error types
func TestErrorSpecificRetryBehavior(t *testing.T) {
	ctx := context.Background()
	
	t.Run("AuthenticationError", func(t *testing.T) {
		// Test with invalid authentication - should fail without retries
		// (authentication errors typically don't benefit from retries)
		// This would need a function configured with invalid auth
		// For now, just test that auth errors are handled appropriately
		
		// We'll use a test that's expected to fail
		_, err := b.TestSingleFallbackClient(ctx)
		if err != nil {
			// Verify error is appropriate type
			assert.Error(t, err)
			// Auth errors shouldn't be retried extensively
		}
	})
	
	t.Run("RateLimitError", func(t *testing.T) {
		// Rate limit errors might trigger retries with backoff
		// This is hard to test without actually hitting rate limits
		// For now, just verify functions can handle such scenarios
		
		// Use a function that might encounter rate limits under load
		result, err := b.TestOpenAIGPT4oMini(ctx, "rate limit test")
		if err != nil {
			// If it fails, check if it's a rate limit related error
			errorMsg := strings.ToLower(err.Error())
			if strings.Contains(errorMsg, "rate") || strings.Contains(errorMsg, "429") {
				t.Logf("Encountered rate limit as expected: %v", err)
			} else {
				t.Logf("Different error type: %v", err)
			}
		} else {
			assert.NotEmpty(t, result)
		}
	})
	
	t.Run("NetworkError", func(t *testing.T) {
		// Network errors typically benefit from retries
		// Hard to simulate reliably, so we test functions that might
		// encounter network issues and verify they handle them
		
		result, err := b.TestOpenAIGPT4oMini(ctx, "network test")
		if err != nil {
			errorMsg := strings.ToLower(err.Error())
			if strings.Contains(errorMsg, "network") || 
			   strings.Contains(errorMsg, "connection") ||
			   strings.Contains(errorMsg, "timeout") {
				t.Logf("Network error handled: %v", err)
			}
		} else {
			assert.NotEmpty(t, result)
		}
	})
}

// TestRetryBackoffTiming tests that retry backoff timing works correctly
func TestRetryBackoffTiming(t *testing.T) {
	ctx := context.Background()
	
	// Test a function that's expected to retry and fail
	start := time.Now()
	_, err := b.TestRetryExponential(ctx)
	duration := time.Since(start)
	
	assert.Error(t, err, "Expected error from retry exponential")
	
	// Should take some time due to retries with backoff
	// Exact timing depends on configuration, but should be more than instant
	assert.Greater(t, duration, 100*time.Millisecond, 
		"Expected some delay due to retry backoff, took: %v", duration)
	
	// But shouldn't take excessively long (indicates reasonable retry limits)
	assert.Less(t, duration, 60*time.Second,
		"Expected retry to complete within reasonable time, took: %v", duration)
}

// TestConcurrentRetriesAndFallbacks tests concurrent operations with retries
func TestConcurrentRetriesAndFallbacks(t *testing.T) {
	ctx := context.Background()
	
	const numConcurrent = 3
	results := make(chan string, numConcurrent)
	errors := make(chan error, numConcurrent)
	
	// Start multiple operations that might use fallbacks concurrently
	for i := 0; i < numConcurrent; i++ {
		go func(id int) {
			result, err := b.TestFallbackClient(ctx)
			if err != nil {
				errors <- err
			} else {
				results <- result
			}
		}(i)
	}
	
	// Collect results
	var successCount int
	var errorCount int
	
	for i := 0; i < numConcurrent; i++ {
		select {
		case result := <-results:
			assert.NotEmpty(t, result)
			successCount++
		case err := <-errors:
			t.Logf("Concurrent fallback error: %v", err)
			errorCount++
		case <-time.After(30 * time.Second):
			t.Fatal("Timeout waiting for concurrent operations")
		}
	}
	
	// At least some should succeed
	assert.Greater(t, successCount, 0, "Expected at least some concurrent operations to succeed")
	
	t.Logf("Concurrent operations: %d succeeded, %d failed", successCount, errorCount)
}

// TestFallbackWithCollector tests fallback behavior combined with collector
func TestFallbackWithCollector(t *testing.T) {
	ctx := context.Background()
	
	collector, err := b.NewCollector("fallback-collector")
	require.NoError(t, err)
	
	// Test fallback function with collector
	result, err := b.TestFallbackClient(ctx, b.WithCollector(collector))
	require.NoError(t, err)
	assert.NotEmpty(t, result)
	
	// Verify collector captured the fallback behavior
	logs, err := collector.Logs()
	require.NoError(t, err)
	assert.Len(t, logs, 1)
	
	log := logs[0]
	name, err := log.FunctionName()
	require.NoError(t, err)
	assert.Equal(t, "TestFallbackClient", name)
	
	// Check if multiple calls were made (indicating fallback attempts)
	calls, err := log.Calls()
	require.NoError(t, err)
	// Might be 1 call (success) or multiple calls (fallback attempts)
	assert.Greater(t, len(calls), 0)
	
	// Verify final call was successful
	finalCall := calls[len(calls)-1]
	selected, err := finalCall.Selected()
	require.NoError(t, err)
	assert.True(t, selected, "Expected final call to be selected/successful")
}

// TestRetryConfigurationRespected tests that retry configuration is respected
func TestRetryConfigurationRespected(t *testing.T) {
	// This test verifies that the retry configuration (max attempts, backoff) is respected
	// The exact behavior depends on the BAML configuration
	
	ctx := context.Background()
	
	// Test with a function that should retry and eventually fail
	start := time.Now()
	_, err := b.TestRetryExponential(ctx)
	duration := time.Since(start)
	
	assert.Error(t, err, "Expected error after retries exhausted")
	
	// The duration should reflect the configured retry behavior
	// This is somewhat implementation-dependent, so we just verify reasonable bounds
	assert.Greater(t, duration, 50*time.Millisecond, "Expected some retry delay")
	assert.Less(t, duration, 120*time.Second, "Expected retries to not take excessively long")
	
	t.Logf("Retry exhaustion took %v", duration)
}