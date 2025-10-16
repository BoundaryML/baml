package main

import (
	"context"
	"sync"
	"testing"
	"time"

	b "example.com/integ-tests/baml_client"
	"example.com/integ-tests/baml_client/stream_types"
	"example.com/integ-tests/baml_client/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestBasicStreaming tests basic streaming functionality
// Reference: test_functions.py:642-677
func TestBasicStreaming(t *testing.T) {
	ctx := context.Background()
	
	stream, err := b.Stream.PromptTestStreaming(ctx, "Programming languages are fun to create")
	require.NoError(t, err)
	
	var msgs []string
	startTime := time.Now()
	var firstMsgTime, lastMsgTime time.Time
	
	for value := range stream {
		if value.IsError {
			t.Fatalf("Stream error: %v", value.Error)
		}
		
		if !value.IsFinal && value.Stream() != nil {
			msg := *value.Stream()
			msgs = append(msgs, msg)
			
			if len(msgs) == 1 {
				firstMsgTime = time.Now()
			}
			lastMsgTime = time.Now()
		}
		
		if value.IsFinal && value.Final() != nil {
			final := *value.Final()
			
			// Verify timing constraints
			assert.True(t, firstMsgTime.Sub(startTime) <= 1500*time.Millisecond, 
				"Expected first message within 1.5 seconds")
			assert.True(t, lastMsgTime.Sub(startTime) >= 1*time.Second,
				"Expected last message after 1 second")
			
			// Verify we got streaming responses
			assert.NotEmpty(t, final, "Expected non-empty final response")
			assert.NotEmpty(t, msgs, "Expected at least one streamed response")
			
			// Verify message continuity
			for i := 1; i < len(msgs); i++ {
				assert.True(t, len(msgs[i]) >= len(msgs[i-1]), 
					"Expected messages to be continuous and growing")
			}
			
			// Final message should match last stream message
			if len(msgs) > 0 {
				assert.Equal(t, msgs[len(msgs)-1], final, 
					"Expected last stream message to match final response")
			}
		}
	}
	
	assert.NotEmpty(t, msgs, "Should have received at least one message")
}

// TestStreamingUniterated tests getting final response without iteration
// Reference: test_functions.py:680-684
func TestStreamingUniterated(t *testing.T) {
	ctx := context.Background()
	
	stream, err := b.Stream.PromptTestStreaming(ctx, "The color blue makes me sad")
	require.NoError(t, err)
	
	// Don't iterate, just get final result
	var final string
	for value := range stream {
		if value.IsFinal && value.Final() != nil {
			final = *value.Final()
			break
		}
	}
	
	assert.NotEmpty(t, final, "Expected non-empty final response")
}

// TestStreamingClaude tests Claude-specific streaming
// Reference: test_functions.py:731-751
func TestStreamingClaude(t *testing.T) {
	ctx := context.Background()
	
	stream, err := b.Stream.PromptTestClaude(ctx, "Mt Rainier is tall")
	require.NoError(t, err)
	
	var msgs []string
	var final string
	
	for value := range stream {
		if value.IsError {
			t.Fatalf("Stream error: %v", value.Error)
		}
		
		if !value.IsFinal && value.Stream() != nil {
			msgs = append(msgs, *value.Stream())
		}
		
		if value.IsFinal && value.Final() != nil {
			final = *value.Final()
		}
	}
	
	assert.NotEmpty(t, final, "Expected non-empty final response")
	assert.NotEmpty(t, msgs, "Expected at least one streamed response")
	
	// Verify message continuity
	for i := 1; i < len(msgs); i++ {
		assert.True(t, len(msgs[i]) >= len(msgs[i-1]),
			"Expected messages to be continuous")
	}
	
	if len(msgs) > 0 {
		assert.Equal(t, msgs[len(msgs)-1], final,
			"Expected last stream message to match final response")
	}
}

// TestStreamingGemini tests Gemini-specific streaming
// Reference: test_functions.py:755-776
func TestStreamingGemini(t *testing.T) {
	ctx := context.Background()
	
	stream, err := b.Stream.TestGemini(ctx, "Dr.Pepper")
	require.NoError(t, err)
	
	var msgs []string
	var final string
	
	for value := range stream {
		if value.IsError {
			t.Fatalf("Stream error: %v", value.Error)
		}
		
		if !value.IsFinal && value.Stream() != nil {
			msg := *value.Stream()
			if msg != "" { // Filter out empty messages like Python version
				msgs = append(msgs, msg)
			}
		}
		
		if value.IsFinal && value.Final() != nil {
			final = *value.Final()
		}
	}
	
	assert.NotEmpty(t, final, "Expected non-empty final response")
	assert.NotEmpty(t, msgs, "Expected at least one streamed response")
	
	// Verify message continuity
	for i := 1; i < len(msgs); i++ {
		assert.True(t, len(msgs[i]) >= len(msgs[i-1]),
			"Expected messages to be continuous")
	}
	
	if len(msgs) > 0 {
		assert.Equal(t, msgs[len(msgs)-1], final,
			"Expected last stream message to match final response")
	}
}

// TestConcurrentStreaming tests multiple concurrent streams
// Reference: test_functions.py:344-380 (parallel async calls pattern)
func TestConcurrentStreaming(t *testing.T) {
	ctx := context.Background()
	
	const numStreams = 3
	var wg sync.WaitGroup
	results := make(chan string, numStreams)
	errors := make(chan error, numStreams)
	
	inputs := []string{
		"Tell me about Go",
		"Tell me about Python", 
		"Tell me about Rust",
	}
	
	for i := 0; i < numStreams; i++ {
		wg.Add(1)
		go func(input string) {
			defer wg.Done()
			
			stream, err := b.Stream.PromptTestStreaming(ctx, input)
			if err != nil {
				errors <- err
				return
			}
			
			var final string
			for value := range stream {
				if value.IsError {
					errors <- value.Error
					return
				}
				
				if value.IsFinal && value.Final() != nil {
					final = *value.Final()
				}
			}
			
			results <- final
		}(inputs[i])
	}
	
	wg.Wait()
	close(results)
	close(errors)
	
	// Check for errors
	select {
	case err := <-errors:
		if err != nil {
			t.Fatalf("Concurrent streaming error: %v", err)
		}
	default:
	}
	
	// Verify we got all results
	var finalResults []string
	for result := range results {
		finalResults = append(finalResults, result)
	}
	
	assert.Len(t, finalResults, numStreams, "Expected results from all streams")
	for _, result := range finalResults {
		assert.NotEmpty(t, result, "Expected non-empty result from each stream")
	}
}

// TestNestedClassStreaming tests streaming with nested class outputs
// Reference: test_functions.py:967-978
func TestNestedClassStreaming(t *testing.T) {
	ctx := context.Background()
	
	stream, err := b.Stream.FnOutputClassNested(ctx, "My name is Harrison. My hair is black and I'm 6 feet tall.")
	require.NoError(t, err)
	
	var msgs []stream_types.TestClassNested
	var final types.TestClassNested
	
	for value := range stream {
		if value.IsError {
			t.Fatalf("Stream error: %v", value.Error)
		}
		
		if !value.IsFinal && value.Stream() != nil {
			msgs = append(msgs, *value.Stream())
		}
		
		if value.IsFinal && value.Final() != nil {
			final = *value.Final()
		}
	}
	
	assert.NotEmpty(t, msgs, "Expected at least one streamed response")
	assert.NotEmpty(t, final.Prop1, "Expected final response to have prop1")
}

// TestStreamBigNumbers tests streaming with large numbers
// Reference: test_functions.py:1260-1285
func TestStreamBigNumbers(t *testing.T) {
	ctx := context.Background()
	
	stream, err := b.Stream.StreamBigNumbers(ctx, 12)
	require.NoError(t, err)
	
	var msgs []stream_types.BigNumbers
	var final types.BigNumbers
	
	for value := range stream {
		if value.IsError {
			t.Fatalf("Stream error: %v", value.Error)
		}
		
		if !value.IsFinal && value.Stream() != nil {
			msgs = append(msgs, *value.Stream())
		}
		
		if value.IsFinal && value.Final() != nil {
			final = *value.Final()
		}
	}
	
	// Verify that partial fields are either nil or match final result
	for _, msg := range msgs {
		if msg.A != nil {
			assert.Equal(t, *msg.A, final.A, "Partial field should match final when not nil")
		}
		if msg.B != nil {
			assert.Equal(t, *msg.B, final.B, "Partial field should match final when not nil")
		}
	}
}

// TestStreamCompoundNumbers tests streaming with compound objects
// Reference: test_functions.py:1289-1304
func TestStreamCompoundNumbers(t *testing.T) {
	ctx := context.Background()
	
	stream, err := b.Stream.StreamingCompoundNumbers(ctx, 12, false)
	require.NoError(t, err)
	
	var msgs []stream_types.CompoundBigNumbers
	var final types.CompoundBigNumbers
	
	for value := range stream {
		if value.IsError {
			t.Fatalf("Stream error: %v", value.Error)
		}
		
		if !value.IsFinal && value.Stream() != nil {
			msgs = append(msgs, *value.Stream())
		}
		
		if value.IsFinal && value.Final() != nil {
			final = *value.Final()
		}
	}
	
	// Verify partial compound objects match final when not nil
	for _, msg := range msgs {
		if msg.Big != nil {
			if msg.Big.A != nil {
				assert.Equal(t, *msg.Big.A, final.Big.A, "Nested partial field should match final")
			}
			if msg.Big.B != nil {
				assert.Equal(t, *msg.Big.B, final.Big.B, "Nested partial field should match final") 
			}
		}
		
		if msg.Another != nil {
			if msg.Another.A != nil {
				assert.Equal(t, *msg.Another.A, final.Another.A, "Another nested field should match final")
			}
			if msg.Another.B != nil {
				assert.Equal(t, *msg.Another.B, final.Another.B, "Another nested field should match final")
			}
		}
		
		// Check list elements
		for i, msgEntry := range msg.Big_nums {
			if i < len(final.Big_nums) {
				if msgEntry.A != nil {
					assert.Equal(t, *msgEntry.A, final.Big_nums[i].A, "List entry should match final")
				}
				if msgEntry.B != nil {
					assert.Equal(t, *msgEntry.B, final.Big_nums[i].B, "List entry should match final")
				}
			}
		}
	}
}

// TestStreamingWithYapping tests streaming with verbose output
// Reference: test_functions.py:1308-1323
func TestStreamingWithYapping(t *testing.T) {
	ctx := context.Background()
	
	stream, err := b.Stream.StreamingCompoundNumbers(ctx, 12, true)
	require.NoError(t, err)
	
	var msgs []stream_types.CompoundBigNumbers
	var final types.CompoundBigNumbers
	
	for value := range stream {
		if value.IsError {
			t.Fatalf("Stream error: %v", value.Error)
		}
		
		if !value.IsFinal && value.Stream() != nil {
			msgs = append(msgs, *value.Stream())
		}
		
		if value.IsFinal && value.Final() != nil {
			final = *value.Final()
		}
	}
	
	// Even with yapping=true, the streaming behavior should be consistent
	assert.NotEmpty(t, msgs, "Expected streaming messages even with yapping")
	assert.NotZero(t, final.Big.A, "Expected final result to have valid big numbers")
	assert.NotZero(t, final.Big.B, "Expected final result to have valid big numbers")
}