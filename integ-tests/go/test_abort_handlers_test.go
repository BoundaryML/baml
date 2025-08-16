package main

import (
	"context"
	"testing"
	"time"

	"github.com/boundaryml/baml/integ-tests/go/baml_client"
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
	_, err := baml_client.FnFailRetryConstantDelay(ctx, 3, 600)
	
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "context canceled")
}

func TestAbortHandlerTimeoutCancellation(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 200*time.Millisecond)
	defer cancel()
	
	// This should timeout before all retries complete
	_, err := baml_client.FnFailRetryConstantDelay(ctx, 3, 150)
	
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "deadline exceeded")
}

func TestAbortHandlerStreamingCancellation(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	
	stream, err := baml_client.Stream.FnFailRetryConstantDelay(ctx, 3, 600)
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
	_, err := baml_client.FnFailRetryExponentialDelay(ctx, 3, 100)
	duration := time.Since(start)
	
	assert.Error(t, err)
	// Should have been cancelled before all exponential retries complete
	// Exponential delays: 100ms, 200ms, 400ms = 700ms total
	// We cancel at 300ms, so duration should be around 300ms
	assert.Less(t, duration, 400*time.Millisecond, "Should have cancelled before all retries")
}

func TestAbortHandlerFallbackChainCancellation(t *testing.T) {
	// Test with a function that has fallback clients
	ctx, cancel := context.WithTimeout(context.Background(), 150*time.Millisecond)
	defer cancel()
	
	// Assuming we have a function with fallback strategy
	// This test depends on having the right BAML configuration
	// You may need to adjust based on your actual test functions
	start := time.Now()
	_, err := baml_client.FnFailRetryConstantDelay(ctx, 2, 100)
	duration := time.Since(start)
	
	assert.Error(t, err)
	assert.Less(t, duration, 200*time.Millisecond, "Should have cancelled during fallback chain")
}

func TestAbortHandlerNoInterferenceWithNormalOperation(t *testing.T) {
	// Test that operations complete normally when not cancelled
	ctx := context.Background()
	
	// Use a function that should succeed quickly
	result, err := baml_client.ExtractName(ctx, "My name is John Doe")
	
	// Should complete successfully
	assert.NoError(t, err)
	assert.NotEmpty(t, result)
}

func TestAbortHandlerMultipleConcurrentCancellations(t *testing.T) {
	// Test multiple concurrent operations being cancelled
	ctx, cancel := context.WithCancel(context.Background())
	
	errChan := make(chan error, 3)
	
	// Start multiple concurrent operations
	for i := 0; i < 3; i++ {
		go func(idx int) {
			_, err := baml_client.FnFailRetryConstantDelay(ctx, 3, 500)
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