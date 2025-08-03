package main

import (
	"context"
	"testing"

	b "example.com/integ-tests/baml_client"
	"example.com/integ-tests/baml_client/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestModularAPIPatterns tests modular API request/response patterns
// Reference: Various Python tests demonstrating modular patterns

// TestConfigurationOptions tests different configuration options
func TestConfigurationOptions(t *testing.T) {
	ctx := context.Background()
	
	t.Run("WithCollector", func(t *testing.T) {
		collector, err := b.NewCollector("options-test")
		require.NoError(t, err)
		
		result, err := b.TestOpenAIGPT4oMini(ctx, "test with collector", b.WithCollector(collector))
		require.NoError(t, err)
		assert.NotEmpty(t, result)
		
		// Verify collector captured the call
		logs, err := collector.Logs()
		require.NoError(t, err)
		assert.Len(t, logs, 1)
	})
	
	t.Run("WithTypeBuilder", func(t *testing.T) {
		tb, err := b.NewTypeBuilder()
		require.NoError(t, err)
		
		// Modify a class
		personClass, err := tb.Person()
		require.NoError(t, err)
		
		stringType, err := tb.String()
		require.NoError(t, err)
		
		_, err = personClass.AddProperty("nickname", stringType)
		require.NoError(t, err)
		
		result, err := b.ExtractPeople(ctx, "My name is John and my nickname is Johnny", b.WithTypeBuilder(tb))
		require.NoError(t, err)
		assert.NotEmpty(t, result)
	})
	
	t.Run("CombinedOptions", func(t *testing.T) {
		collector, err := b.NewCollector("combined-test")
		require.NoError(t, err)
		
		tb, err := b.NewTypeBuilder()
		require.NoError(t, err)
		
		// Use both options together
		result, err := b.ExtractPeople(ctx, "My name is Alice", 
			b.WithCollector(collector),
			b.WithTypeBuilder(tb))
		require.NoError(t, err)
		assert.NotEmpty(t, result)
		
		// Verify collector worked
		logs, err := collector.Logs()
		require.NoError(t, err)
		assert.Len(t, logs, 1)
	})
}

// TestRequestResponsePatterns tests different request/response patterns
func TestRequestResponsePatterns(t *testing.T) {
	ctx := context.Background()
	
	t.Run("SimpleStringToString", func(t *testing.T) {
		result, err := b.TestOpenAIGPT4oMini(ctx, "Hello")
		require.NoError(t, err)
		assert.NotEmpty(t, result)
	})
	
	t.Run("StringToStructuredOutput", func(t *testing.T) {
		result, err := b.ExtractPeople(ctx, "My name is Bob and I have brown hair")
		require.NoError(t, err)
		assert.NotEmpty(t, result)
		assert.NotNil(t, result[0].Name)
		assert.Equal(t, "Bob", *result[0].Name)
	})
	
	t.Run("StructuredInputToOutput", func(t *testing.T) {
		input := types.NamedArgsSingleClass{
			Key:       "test-key",
			Key_two:   true,
			Key_three: 42,
		}
		
		result, err := b.TestFnNamedArgsSingleClass(ctx, input)
		require.NoError(t, err)
		assert.Contains(t, result, "42")
	})
	
	t.Run("MapInputPattern", func(t *testing.T) {
		inputMap := map[string]string{"key": "value"}
		result, err := b.TestFnNamedArgsSingleMapStringToString(ctx, inputMap)
		require.NoError(t, err)
		assert.NotEmpty(t, result)
	})
}

// TestParseAPIPatterns tests the Parse API patterns
func TestParseAPIPatterns(t *testing.T) {
	t.Run("ParseLinkedList", func(t *testing.T) {
		jsonInput := `{
			"len": 3,
			"head": {
				"data": 1,
				"next": {
					"data": 2,
					"next": {
						"data": 3,
						"next": null
					}
				}
			}
		}`
		
		result, err := b.Parse.BuildLinkedList(jsonInput)
		require.NoError(t, err)
		assert.Equal(t, int64(3), result.Len)
		assert.NotNil(t, result.Head)
		assert.Equal(t, int64(1), result.Head.Data)
	})
	
	t.Run("ParseExtractResume", func(t *testing.T) {
		jsonInput := `{
			"name": "John Doe",
			"email": "john@example.com",
			"phone": "123-456-7890",
			"experience": ["Software Engineer at Google"],
			"education": [],
			"skills": ["Go", "Python"]
		}`
		
		result, err := b.Parse.ExtractResume(jsonInput)
		require.NoError(t, err)
		assert.NotNil(t, result.Name)
		assert.Equal(t, "John Doe", result.Name)
		assert.NotNil(t, result.Email)
		assert.Equal(t, "john@example.com", result.Email)
	})
}

// TestStreamAPIPatterns tests streaming API patterns
func TestStreamAPIPatterns(t *testing.T) {
	ctx := context.Background()
	
	t.Run("BasicStreaming", func(t *testing.T) {
		stream, err := b.Stream.TestOpenAIGPT4oMini(ctx, "Tell me a short story")
		require.NoError(t, err)
		
		var chunks []string
		var final string
		
		for value := range stream {
			if value.IsError {
				t.Fatalf("Stream error: %v", value.Error)
			}
			
			if !value.IsFinal && value.Stream() != nil {
				chunks = append(chunks, *value.Stream())
			}
			
			if value.IsFinal && value.Final() != nil {
				final = *value.Final()
			}
		}
		
		assert.Greater(t, len(chunks), 0, "Expected streaming chunks")
		assert.NotEmpty(t, final, "Expected final result")
	})
	
	t.Run("StreamingWithCollector", func(t *testing.T) {
		collector, err := b.NewCollector("stream-collector")
		require.NoError(t, err)
		
		stream, err := b.Stream.TestOpenAIGPT4oMini(ctx, "Hello streaming", b.WithCollector(collector))
		require.NoError(t, err)
		
		var final string
		for value := range stream {
			if value.IsFinal && value.Final() != nil {
				final = *value.Final()
			}
		}
		
		assert.NotEmpty(t, final)
		
		// Verify collector captured streaming call
		logs, err := collector.Logs()
		require.NoError(t, err)
		assert.Len(t, logs, 1)
		
		logType, err := logs[0].LogType()
		require.NoError(t, err)
		assert.Equal(t, "stream", logType)
	})
}

// TestAPIConsistency tests consistency across different API patterns
func TestAPIConsistency(t *testing.T) {
	ctx := context.Background()
	
	// Test that the same function works consistently across sync/async/parse patterns
	input := "My name is Charlie"
	
	t.Run("SyncCall", func(t *testing.T) {
		result, err := b.ExtractPeople(ctx, input)
		require.NoError(t, err)
		assert.NotEmpty(t, result)
		assert.NotNil(t, result[0].Name)
		assert.Equal(t, "Charlie", *result[0].Name)
	})
	
	t.Run("StreamCall", func(t *testing.T) {
		stream, err := b.Stream.ExtractPeople(ctx, input)
		require.NoError(t, err)
		
		var final []types.Person
		for value := range stream {
			if value.IsFinal && value.Final() != nil {
				final = *value.Final()
			}
		}
		
		assert.NotEmpty(t, final)
		assert.NotNil(t, final[0].Name)
		assert.Equal(t, "Charlie", *final[0].Name)
	})
}

// TestErrorHandlingPatterns tests error handling across different API patterns
func TestErrorHandlingPatterns(t *testing.T) {
	ctx := context.Background()
	
	t.Run("SyncErrorHandling", func(t *testing.T) {
		_, err := b.DummyOutputFunction(ctx, "should fail")
		assert.Error(t, err)
		assert.Contains(t, err.Error(), "Failed to coerce")
	})
	
	t.Run("StreamErrorHandling", func(t *testing.T) {
		stream, err := b.Stream.DummyOutputFunction(ctx, "should fail")
		if err != nil {
			// Error before streaming starts
			assert.Contains(t, err.Error(), "Failed to coerce")
			return
		}
		
		// Error during streaming
		var hadError bool
		for value := range stream {
			if value.IsError {
				hadError = true
				assert.Contains(t, value.Error.Error(), "Failed to coerce")
				break
			}
		}
		
		assert.True(t, hadError, "Expected streaming error")
	})
	
	t.Run("ParseErrorHandling", func(t *testing.T) {
		_, err := b.Parse.ExtractResume("invalid json")
		assert.Error(t, err)
	})
}

// TestConfigurationPatterns tests different configuration patterns
func TestConfigurationPatterns(t *testing.T) {
	ctx := context.Background()
	
	t.Run("DefaultConfiguration", func(t *testing.T) {
		// Test with default configuration
		result, err := b.TestOpenAIGPT4oMini(ctx, "default config test")
		require.NoError(t, err)
		assert.NotEmpty(t, result)
	})
	
	t.Run("CustomCollectorConfiguration", func(t *testing.T) {
		collector, err := b.NewCollector("custom-config")
		require.NoError(t, err)
		
		result, err := b.TestOpenAIGPT4oMini(ctx, "custom collector config", b.WithCollector(collector))
		require.NoError(t, err)
		assert.NotEmpty(t, result)
		
		// Verify custom configuration worked
		logs, err := collector.Logs()
		require.NoError(t, err)
		assert.Len(t, logs, 1)
		
		// Check collector captured usage data
		usage, err := collector.Usage()
		require.NoError(t, err)
		inputTokens, err := usage.InputTokens()
		require.NoError(t, err)
		assert.Greater(t, inputTokens, int64(0))
	})
}

// TestAsyncPatterns tests asynchronous patterns (using Go routines)
func TestAsyncPatterns(t *testing.T) {
	ctx := context.Background()
	
	t.Run("ConcurrentCalls", func(t *testing.T) {
		const numCalls = 3
		results := make(chan string, numCalls)
		errors := make(chan error, numCalls)
		
		for i := 0; i < numCalls; i++ {
			go func(id int) {
				result, err := b.TestOpenAIGPT4oMini(ctx, "concurrent test")
				if err != nil {
					errors <- err
				} else {
					results <- result
				}
			}(i)
		}
		
		// Collect results
		var successCount int
		for i := 0; i < numCalls; i++ {
			select {
			case result := <-results:
				assert.NotEmpty(t, result)
				successCount++
			case err := <-errors:
				t.Logf("Concurrent call error: %v", err)
			}
		}
		
		assert.Greater(t, successCount, 0, "Expected at least some concurrent calls to succeed")
	})
	
	t.Run("ConcurrentStreaming", func(t *testing.T) {
		const numStreams = 2
		results := make(chan string, numStreams)
		
		for i := 0; i < numStreams; i++ {
			go func(id int) {
				stream, err := b.Stream.TestOpenAIGPT4oMini(ctx, "concurrent stream test")
				if err != nil {
					t.Errorf("Stream %d error: %v", id, err)
					return
				}
				
				var final string
				for value := range stream {
					if value.IsFinal && value.Final() != nil {
						final = *value.Final()
					}
				}
				
				results <- final
			}(i)
		}
		
		// Collect streaming results
		for i := 0; i < numStreams; i++ {
			result := <-results
			assert.NotEmpty(t, result, "Expected non-empty result from concurrent stream %d", i)
		}
	})
}