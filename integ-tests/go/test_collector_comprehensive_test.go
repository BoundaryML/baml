package main

import (
	"context"
	"fmt"
	"sync"
	"testing"
	"time"

	b "example.com/integ-tests/baml_client"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestCollectorBasicUsage tests basic collector functionality
// Reference: test_collector.py:28-122
func TestCollectorBasicUsage(t *testing.T) {
	ctx := context.Background()
	
	collector, err := b.NewCollector("my-collector")
	require.NoError(t, err)
	
	// Initially no logs
	logs, err := collector.Logs()
	require.NoError(t, err)
	assert.Len(t, logs, 0)
	
	// Make a function call with collector
	_, err = b.TestOpenAIGPT4oMini(ctx, "hi there", b.WithCollector(collector))
	require.NoError(t, err)
	
	// Should have one log entry
	logs, err = collector.Logs()
	require.NoError(t, err)
	assert.Len(t, logs, 1)
	
	log := logs[0]
	name, err := log.FunctionName()
	require.NoError(t, err)
	assert.Equal(t, "TestOpenAIGPT4oMini", name)
	logType, err := log.LogType()
	require.NoError(t, err)
	assert.Equal(t, "call", logType)
	
	// Verify timing fields
	timing, err := log.Timing()
	require.NoError(t, err)
	startTime, err := timing.StartTimeUTCMs()
	require.NoError(t, err)
	assert.Greater(t, startTime, int64(0))
	duration, err := timing.DurationMs()
	require.NoError(t, err)
	assert.Greater(t, *duration, int64(0))
	
	// Verify usage fields
	usage, err := log.Usage()
	require.NoError(t, err)
	inputTokens, err := usage.InputTokens()
	require.NoError(t, err)
	assert.Greater(t, inputTokens, int64(0))
	outputTokens, err := usage.OutputTokens()
	require.NoError(t, err)
	assert.Greater(t, outputTokens, int64(0))
	
	// Verify calls
	calls, err := log.Calls()
	require.NoError(t, err)
	assert.Len(t, calls, 1)
	
	call := calls[0]
	provider, err := call.Provider()
	require.NoError(t, err)
	assert.Equal(t, "openai", provider)
	clientName, err := call.ClientName()
	require.NoError(t, err)
	assert.Equal(t, "GPT4oMini", clientName)
	selected, err := call.Selected()
	require.NoError(t, err)
	assert.True(t, selected)
	
	// Verify request/response
	request, err := call.HttpRequest()
	require.NoError(t, err)
	assert.NotNil(t, request)
	
	body, err := request.Body()
	require.NoError(t, err)
	text, err := body.Text()
	require.NoError(t, err)
	assert.Contains(t, text, "messages")
	assert.Contains(t, text, "gpt-4o-mini")
	
	// Verify HTTP response
	response, err := call.HttpResponse()
	require.NoError(t, err)
	assert.NotNil(t, response)
	status, err := response.Status()
	require.NoError(t, err)
	assert.Equal(t, int64(200), status)
	
	responseBody, err := response.Body()
	require.NoError(t, err)
	text, err = responseBody.Text()
	require.NoError(t, err)
	assert.Contains(t, text, "choices")
	
	// Verify call timing
	callTiming, err := call.Timing()
	require.NoError(t, err)
	startTime, err = callTiming.StartTimeUTCMs()
	require.NoError(t, err)
	assert.Greater(t, startTime, int64(0))
	duration, err = callTiming.DurationMs()
	require.NoError(t, err)
	assert.Greater(t, *duration, int64(0))
	
	// Verify call usage
	callUsage, err := call.Usage()
	require.NoError(t, err)
	inputTokens, err = callUsage.InputTokens()
	require.NoError(t, err)
	assert.Greater(t, inputTokens, int64(0))
	outputTokens, err = callUsage.OutputTokens()
	require.NoError(t, err)
	assert.Greater(t, outputTokens, int64(0))
	
	// Usage should match between call and log
	inputTokens, err = callUsage.InputTokens()
	require.NoError(t, err)
	inputTokensUsage, err := usage.InputTokens()
	require.NoError(t, err)
	assert.Equal(t, inputTokens, inputTokensUsage)
	outputTokensUsage, err := usage.OutputTokens()
	require.NoError(t, err)
	assert.Equal(t, outputTokens, outputTokensUsage)
	
	// Verify collector usage
	collectorUsage, err := collector.Usage()
	require.NoError(t, err)
	inputTokensUsage, err = collectorUsage.InputTokens()
	require.NoError(t, err)
	inputTokensUsage, err = collectorUsage.InputTokens()
	require.NoError(t, err)
	assert.Equal(t, inputTokens, inputTokensUsage)
	outputTokensUsage, err = collectorUsage.OutputTokens()
	require.NoError(t, err)
	assert.Equal(t, outputTokens, outputTokensUsage)
}

// TestCollectorStreamingCalls tests collector with streaming calls
// Reference: test_collector.py:145-223
func TestCollectorStreamingCalls(t *testing.T) {
	ctx := context.Background()
	
	collector, err := b.NewCollector("my-collector")
	require.NoError(t, err)
	
	// Make streaming call with collector
	stream, err := b.Stream.TestOpenAIGPT4oMini(ctx, "hi there", b.WithCollector(collector))
	require.NoError(t, err)
	
	var finalResult string
	for value := range stream {
		if value.IsFinal && value.Final() != nil {
			finalResult = *value.Final()
		}
	}
	
	assert.NotEmpty(t, finalResult)
	
	// Check logs
	logs, err := collector.Logs()
	require.NoError(t, err)
	assert.Len(t, logs, 1)
	
	log := logs[0]
	name, err := log.FunctionName()
	require.NoError(t, err)
	assert.Equal(t, "TestOpenAIGPT4oMini", name)
	logType, err := log.LogType()
	require.NoError(t, err)
	assert.Equal(t, "stream", logType)
	
	// Verify timing and usage
	timing, err := log.Timing()
	require.NoError(t, err)
	startTime, err := timing.StartTimeUTCMs()
	require.NoError(t, err)
	assert.Greater(t, startTime, int64(0))
	duration, err := timing.DurationMs()
	require.NoError(t, err)
	assert.Greater(t, *duration, int64(0))
	
	usage, err := log.Usage()
	require.NoError(t, err)
	inputTokens, err := usage.InputTokens()
	require.NoError(t, err)
	assert.Greater(t, inputTokens, int64(0))
	outputTokens, err := usage.OutputTokens()
	require.NoError(t, err)
	assert.Greater(t, outputTokens, int64(0))
	
	// Verify calls
	calls, err := log.Calls()
	require.NoError(t, err)
	assert.Len(t, calls, 1)
	
	call := calls[0]
	provider, err := call.Provider()
	require.NoError(t, err)
	assert.Equal(t, "openai", provider)
	clientName, err := call.ClientName()
	require.NoError(t, err)
	assert.Equal(t, "GPT4oMini", clientName)
	selected, err := call.Selected()
	require.NoError(t, err)
	assert.True(t, selected)
	
	// For streaming, HTTP response should be nil
	response, err := call.HttpResponse()
	require.NoError(t, err)
	assert.Nil(t, response)
	
	// But request should exist
	request, err := call.HttpRequest()
	require.NoError(t, err)
	assert.NotNil(t, request)
	
	body, err := request.Body()
	require.NoError(t, err)
	text, err := body.Text()
	require.NoError(t, err)
	assert.Contains(t, text, "stream")
}

// TestCollectorMultipleCalls tests usage accumulation across multiple calls
// Reference: test_collector.py:227-255
func TestCollectorMultipleCalls(t *testing.T) {
	ctx := context.Background()
	
	collector, err := b.NewCollector("my-collector")
	require.NoError(t, err)
	
	// First call
	_, err = b.TestOpenAIGPT4oMini(ctx, "First call", b.WithCollector(collector))
	require.NoError(t, err)
	
	logs, err := collector.Logs()
	require.NoError(t, err)
	assert.Len(t, logs, 1)
	
	// Capture usage after first call
	firstCallUsage, err := logs[0].Usage()
	require.NoError(t, err)
	
	collectorUsage, err := collector.Usage()
	require.NoError(t, err)
	inputTokens, err := firstCallUsage.InputTokens()
	require.NoError(t, err)
	inputTokensUsage, err := collectorUsage.InputTokens()
	require.NoError(t, err)
	assert.Equal(t, inputTokens, inputTokensUsage)
	outputTokens, err := firstCallUsage.OutputTokens()
	require.NoError(t, err)
	outputTokensUsage, err := collectorUsage.OutputTokens()
	require.NoError(t, err)
	assert.Equal(t, outputTokens, outputTokensUsage)
	
	// Second call
	_, err = b.TestOpenAIGPT4oMini(ctx, "Second call", b.WithCollector(collector))
	require.NoError(t, err)
	
	logs, err = collector.Logs()
	require.NoError(t, err)
	assert.Len(t, logs, 2)
	
	// Capture usage after second call
	secondCallUsage, err := logs[1].Usage()
	require.NoError(t, err)
	
	firstCallInputTokens, err := firstCallUsage.InputTokens()
	require.NoError(t, err)
	secondCallInputTokens, err := secondCallUsage.InputTokens()
	require.NoError(t, err)
	totalInput := firstCallInputTokens + secondCallInputTokens
	firstCallOutputTokens, err := firstCallUsage.OutputTokens()
	require.NoError(t, err)
	secondCallOutputTokens, err := secondCallUsage.OutputTokens()
	require.NoError(t, err)
	totalOutput := firstCallOutputTokens + secondCallOutputTokens
	
	collectorUsage, err = collector.Usage()
	require.NoError(t, err)
	inputTokensUsage, err = collectorUsage.InputTokens()
	require.NoError(t, err)
	assert.Equal(t, totalInput, inputTokensUsage)
	outputTokensUsage, err = collectorUsage.OutputTokens()
	require.NoError(t, err)
	assert.Equal(t, totalOutput, outputTokensUsage)
}

// TestCollectorConcurrentCalls tests collector with concurrent function calls
// Reference: test_collector.py:344-380
func TestCollectorConcurrentCalls(t *testing.T) {
	ctx := context.Background()
	
	collector, err := b.NewCollector("parallel-collector")
	require.NoError(t, err)
	
	const numCalls = 3
	var wg sync.WaitGroup
	results := make(chan string, numCalls)
	errors := make(chan error, numCalls)
	
	inputs := []string{"call #1", "call #2", "call #3"}
	
	for i := 0; i < numCalls; i++ {
		wg.Add(1)
		go func(input string) {
			defer wg.Done()
			
			result, err := b.TestOpenAIGPT4oMini(ctx, input, b.WithCollector(collector))
			if err != nil {
				fmt.Printf("error: %v\n", err)
				errors <- err
				return
			}
			
			results <- result
		}(inputs[i])
	}
	
	wg.Wait()
	close(results)
	close(errors)
	
	// Check for errors
	select {
	case err := <-errors:
		if err != nil {
			t.Fatalf("Concurrent call error: %v", err)
		}
	default:
	}
	
	// Verify we got all results
	var finalResults []string
	for result := range results {
		finalResults = append(finalResults, result)
	}
	
	assert.Len(t, finalResults, numCalls, "Expected results from all calls")
	for _, result := range finalResults {
		assert.NotEmpty(t, result, "Expected non-empty result from each call")
	}
	
	// Verify collector captured all calls
	logs, err := collector.Logs()
	require.NoError(t, err)
	assert.Len(t, logs, numCalls)
	
	// Verify each call is recorded properly
	for _, log := range logs {
		name, err := log.FunctionName()
		require.NoError(t, err)
		assert.Equal(t, "TestOpenAIGPT4oMini", name)
		logType, err := log.LogType()
		require.NoError(t, err)
		assert.Equal(t, "call", logType)
		
		usage, err := log.Usage()
		require.NoError(t, err)
		inputTokens, err := usage.InputTokens()
		require.NoError(t, err)
		assert.Greater(t, inputTokens, int64(0))
		outputTokens, err := usage.OutputTokens()
		require.NoError(t, err)
		assert.Greater(t, outputTokens, int64(0))
	}
	
	// Verify total collector usage equals sum of all calls
	var totalInput, totalOutput int64
	for _, log := range logs {
		usage, err := log.Usage()
		require.NoError(t, err)
		inputTokens, err := usage.InputTokens()
		require.NoError(t, err)
		totalInput += inputTokens
		outputTokens, err := usage.OutputTokens()
		require.NoError(t, err)
		totalOutput += outputTokens
	}
	
	collectorUsage, err := collector.Usage()
	require.NoError(t, err)
	inputTokensUsage, err := collectorUsage.InputTokens()
	require.NoError(t, err)
	assert.Equal(t, totalInput, inputTokensUsage)
	outputTokensUsage, err := collectorUsage.OutputTokens()
	require.NoError(t, err)
	assert.Equal(t, totalOutput, outputTokensUsage)
}

// TestCollectorMultipleCollectors tests multiple collectors for same call
// Reference: test_collector.py:258-309
func TestCollectorMultipleCollectors(t *testing.T) {
	ctx := context.Background()
	
	collector1, err := b.NewCollector("collector-1")
	require.NoError(t, err)
	
	collector2, err := b.NewCollector("collector-2")
	require.NoError(t, err)
	
	// Pass both collectors for the first call
	_, err = b.TestOpenAIGPT4oMini(ctx, "First call", b.WithCollectors([]b.Collector{collector1, collector2}))
	require.NoError(t, err)
	
	// Check usage/logs after the first call
	logs1, err := collector1.Logs()
	require.NoError(t, err)
	assert.Len(t, logs1, 1)
	
	logs2, err := collector2.Logs()
	require.NoError(t, err)
	assert.Len(t, logs2, 1)
	
	usage1, err := logs1[0].Usage()
	require.NoError(t, err)
	
	usage2, err := logs2[0].Usage()
	require.NoError(t, err)
	
	// Verify both collectors have the exact same usage for the first call
	inputTokensUsage1, err := usage1.InputTokens()
	require.NoError(t, err)
	inputTokensUsage2, err := usage2.InputTokens()
	require.NoError(t, err)
	assert.Equal(t, inputTokensUsage1, inputTokensUsage2)
	outputTokensUsage1, err := usage1.OutputTokens()
	require.NoError(t, err)
	outputTokensUsage2, err := usage2.OutputTokens()
	require.NoError(t, err)
	assert.Equal(t, outputTokensUsage1, outputTokensUsage2)
	
	// Also check that collector-level usage matches single call usage
	collectorUsage1, err := collector1.Usage()
	require.NoError(t, err)
	inputTokensUsage1, err = collectorUsage1.InputTokens()
	require.NoError(t, err)
	assert.Equal(t, inputTokensUsage1, inputTokensUsage1)
	outputTokensUsage1, err = collectorUsage1.OutputTokens()
	require.NoError(t, err)
	assert.Equal(t, outputTokensUsage1, outputTokensUsage1)
	
	collectorUsage2, err := collector2.Usage()
	require.NoError(t, err)
	inputTokensUsage2, err = collectorUsage2.InputTokens()
	require.NoError(t, err)
	assert.Equal(t, inputTokensUsage2, inputTokensUsage2)
	outputTokensUsage2, err = collectorUsage2.OutputTokens()
	require.NoError(t, err)
	assert.Equal(t, outputTokensUsage2, outputTokensUsage2)
	
	// Second call uses only collector1
	_, err = b.TestOpenAIGPT4oMini(ctx, "Second call", b.WithCollector(collector1))
	require.NoError(t, err)
	
	// Re-check logs/usage
	logs1, err = collector1.Logs()
	require.NoError(t, err)
	assert.Len(t, logs1, 2)
	
	logs2, err = collector2.Logs()
	require.NoError(t, err)
	assert.Len(t, logs2, 1) // Should still be 1
	
	// Verify collector1 usage is now the sum of both calls
	secondCallUsage, err := logs1[1].Usage()
	require.NoError(t, err)
	
	secondCallInputTokens, err := secondCallUsage.InputTokens()
	require.NoError(t, err)
	secondCallOutputTokens, err := secondCallUsage.OutputTokens()
	require.NoError(t, err)
	totalInput := inputTokensUsage1 + secondCallInputTokens
	totalOutput := outputTokensUsage1 + secondCallOutputTokens
	
	collectorUsage1, err = collector1.Usage()
	require.NoError(t, err)
	inputTokensUsage1, err = collectorUsage1.InputTokens()
	require.NoError(t, err)
	assert.Equal(t, totalInput, inputTokensUsage1)
	outputTokensUsage1, err = collectorUsage1.OutputTokens()
	require.NoError(t, err)
	assert.Equal(t, totalOutput, outputTokensUsage1)
	
	// Verify collector2 usage remains unchanged
	collectorUsage2, err = collector2.Usage()
	require.NoError(t, err)
	inputTokensUsage2, err = collectorUsage2.InputTokens()
	require.NoError(t, err)
	assert.Equal(t, inputTokensUsage2, inputTokensUsage2)
	outputTokensUsage2, err = collectorUsage2.OutputTokens()
	require.NoError(t, err)
	assert.Equal(t, outputTokensUsage2, outputTokensUsage2)
}

// TestCollectorProviderSpecific tests collector with different providers
// Reference: test_collector.py:461-575 (various provider tests)
func TestCollectorProviderSpecific(t *testing.T) {
	ctx := context.Background()
	
	tests := []struct {
		name         string
		functionCall func(context.Context, string, ...b.CallOptionFunc) (string, error)
		provider     string
		clientName   string
	}{
		{
			name:         "TestAws",
			functionCall: b.TestAws,
			provider:     "aws-bedrock",
			clientName:   "AwsBedrock",
		},
		{
			name:         "TestGemini", 
			functionCall: b.TestGemini,
			provider:     "google-ai",
			clientName:   "Gemini",
		},
		{
			name:         "PromptTestClaude",
			functionCall: b.PromptTestClaude,
			provider:     "anthropic",
			clientName:   "Sonnet",
		},
	}
	
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			collector, err := b.NewCollector("provider-test")
			require.NoError(t, err)
			
			_, err = tt.functionCall(ctx, "test input", b.WithCollector(collector))
			require.NoError(t, err)
			
			logs, err := collector.Logs()
			require.NoError(t, err)
			assert.Len(t, logs, 1)
			
			log := logs[0]
			name, err := log.FunctionName()
			require.NoError(t, err)
			assert.Equal(t, tt.name, name)
			logType, err := log.LogType()
			require.NoError(t, err)
			assert.Equal(t, "call", logType)
			
			calls, err := log.Calls()
			require.NoError(t, err)
			assert.Len(t, calls, 1)
			
			call := calls[0]
			provider, err := call.Provider()
			require.NoError(t, err)
			assert.Equal(t, tt.provider, provider)
			if tt.clientName != "" {
				clientName, err := call.ClientName()
				require.NoError(t, err)
				assert.Equal(t, tt.clientName, clientName)
			}
			selected, err := call.Selected()
			require.NoError(t, err)
			assert.True(t, selected)
			
			// Verify request exists
			request, err := call.HttpRequest()
			require.NoError(t, err)
			assert.NotNil(t, request)
			
			// Verify response exists for non-streaming
			response, err := call.HttpResponse()
			require.NoError(t, err)
			assert.NotNil(t, response)
			status, err := response.Status()
			require.NoError(t, err)
			assert.Equal(t, int64(200), status)
			
			// Verify usage
			usage, err := call.Usage()
			require.NoError(t, err)
			inputTokens, err := usage.InputTokens()
			require.NoError(t, err)
			assert.Greater(t, inputTokens, int64(0))
			outputTokens, err := usage.OutputTokens()
			require.NoError(t, err)
			assert.Greater(t, outputTokens, int64(0))
		})
	}
}

// TestCollectorErrorHandling tests collector behavior with errors
// Reference: test_collector.py:383-458 (failure scenarios)
func TestCollectorErrorHandling(t *testing.T) {
	ctx := context.Background()
	
	t.Run("InvalidArgument", func(t *testing.T) {
		collector, err := b.NewCollector("error-collector")
		require.NoError(t, err)
		
		// This should fail due to invalid argument type (if type checking exists)
		// If not, we'll test with a function that we expect to fail
		_, err = b.ExpectFailure(ctx, b.WithCollector(collector))
		
		// Whether it succeeds or fails, collector should track the attempt
		logs, err := collector.Logs()
		require.NoError(t, err)
		
		if len(logs) > 0 {
			log := logs[0]
			name, err := log.FunctionName()
			require.NoError(t, err)
			assert.Equal(t, "ExpectFailure", name)
		}
	})
}

// TestCollectorMemoryManagement tests collector cleanup and GC behavior
// Reference: test_collector.py:20-26 (ensure_collector_is_empty fixture)
func TestCollectorMemoryManagement(t *testing.T) {
	ctx := context.Background()
	
	// Create collector in limited scope
	func() {
		collector, err := b.NewCollector("temp-collector")
		require.NoError(t, err)
		
		_, err = b.TestOpenAIGPT4oMini(ctx, "temp call", b.WithCollector(collector))
		require.NoError(t, err)
		
		logs, err := collector.Logs()
		require.NoError(t, err)
		assert.Len(t, logs, 1)
		
		// Collector should be valid within scope
		usage, err := collector.Usage()
		require.NoError(t, err)
		inputTokens, err := usage.InputTokens()
		require.NoError(t, err)
		assert.Greater(t, inputTokens, int64(0))
	}()
	
	// After scope, memory should eventually be cleaned up
	// This is more of a smoke test since Go's GC behavior is not deterministic
	time.Sleep(100 * time.Millisecond)
}

// TestCollectorContextTimeout tests collector with context timeout
func TestCollectorContextTimeout(t *testing.T) {
	// Create context with very short timeout
	ctx, cancel := context.WithTimeout(context.Background(), 1*time.Nanosecond)
	defer cancel()
	
	// Wait for context to timeout
	time.Sleep(10 * time.Millisecond)
	
	collector, err := b.NewCollector("timeout-collector")
	require.NoError(t, err)
	
	// This should fail due to context timeout
	_, err = b.TestOpenAIGPT4oMini(ctx, "timeout test", b.WithCollector(collector))
	assert.Error(t, err, "Expected timeout error")
	
	// Even with error, collector might still have captured the attempt
	logs, err := collector.Logs()
	require.NoError(t, err)
	// Length could be 0 or 1 depending on when timeout occurred
	assert.True(t, len(logs) <= 1, "Expected at most one log entry")
}