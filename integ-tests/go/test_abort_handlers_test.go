package main

import (
	"context"
	"testing"
	"time"

	"example.com/integ-tests/baml_client"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestAbortHandlerManualCancellation(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	
	// Cancel after 100ms
	go func() {
		time.Sleep(100 * time.Millisecond)
		cancel()
	}()
	
	// This should be cancelled before completion
	_, err := baml_client.TestRetryConstant(ctx)
	
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "context canceled")
}

func TestAbortHandlerTimeoutCancellation(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 200*time.Millisecond)
	defer cancel()
	
	// This should timeout before all retries complete
	_, err := baml_client.TestRetryExponential(ctx)
	
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "deadline exceeded")
}

func TestAbortHandlerStreamingCancellation(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	
	stream, err := baml_client.Stream.TestFallbackClient(ctx)
	require.NoError(t, err)
	
	// Cancel after 50ms
	go func() {
		time.Sleep(50 * time.Millisecond)
		cancel()
	}()
	
	count := 0
	for range stream {
		count++
	}
	
	// Should have stopped early due to cancellation
	assert.Less(t, count, 10, "Stream should have been cancelled early")
}

func TestAbortHandlerRetryChainCancellation(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 300*time.Millisecond)
	defer cancel()
	
	start := time.Now()
	_, err := baml_client.TestRetryExponential(ctx)
	duration := time.Since(start)
	
	assert.Error(t, err)
	// Should have been cancelled before all exponential retries complete
	// Exponential delays would sum up to more than 300ms
	assert.Less(t, duration, 400*time.Millisecond, "Should have cancelled before all retries")
}

func TestAbortHandlerFallbackChainCancellation(t *testing.T) {
	// Test with a function that has fallback clients
	ctx, cancel := context.WithTimeout(context.Background(), 150*time.Millisecond)
	defer cancel()
	
	start := time.Now()
	_, err := baml_client.TestFallbackClient(ctx)
	duration := time.Since(start)
	
	assert.Error(t, err)
	assert.Less(t, duration, 200*time.Millisecond, "Should have cancelled during fallback chain")
}

func TestAbortHandlerNoInterferenceWithNormalOperation(t *testing.T) {
	// Test that operations complete normally when not cancelled
	ctx := context.Background()
	
	// Use a function that should succeed quickly
	result, err := baml_client.ExtractNames(ctx, "My name is John Doe")
	
	// Should complete successfully (or fail due to LLM, but not due to cancellation)
	// We're just checking that cancellation doesn't interfere when not triggered
	if err != nil {
		assert.NotContains(t, err.Error(), "context canceled")
		assert.NotContains(t, err.Error(), "deadline exceeded")
	} else {
		assert.NotEmpty(t, result)
	}
}

func TestAbortHandlerMultipleConcurrentCancellations(t *testing.T) {
	// Test multiple concurrent operations being cancelled
	ctx, cancel := context.WithCancel(context.Background())
	
	errChan := make(chan error, 3)
	
	// Start multiple concurrent operations
	for i := 0; i < 3; i++ {
		go func(idx int) {
			_, err := baml_client.TestRetryConstant(ctx)
			errChan <- err
		}(i)
	}
	
	// Cancel all operations after 100ms
	time.Sleep(100 * time.Millisecond)
	cancel()
	
	// Collect all errors
	for i := 0; i < 3; i++ {
		err := <-errChan
		assert.Error(t, err)
		assert.Contains(t, err.Error(), "context canceled")
	}
}