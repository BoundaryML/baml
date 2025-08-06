package main

import (
	"context"
	"strings"
	"testing"

	b "example.com/integ-tests/baml_client"
	types "example.com/integ-tests/baml_client/types"
	baml "github.com/boundaryml/baml/engine/language_client_go/pkg"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestOpenAIProvider tests OpenAI provider functionality
// Reference: test_functions.py:606-617
func TestOpenAIProvider(t *testing.T) {
	ctx := context.Background()
	
	t.Run("OpenAIShorthand", func(t *testing.T) {
		result, err := b.TestOpenAIShorthand(ctx, "Mt Rainier is tall")
		require.NoError(t, err)
		assert.NotEmpty(t, result, "Expected non-empty result from OpenAI shorthand")
	})
	
	t.Run("OpenAIShorthandStreaming", func(t *testing.T) {
		stream, err := b.Stream.TestOpenAIShorthand(ctx, "Mt Rainier is tall")
		require.NoError(t, err)
		
		var final string
		for value := range stream {
			if value.IsFinal && value.Final() != nil {
				final = *value.Final()
			}
		}
		
		assert.NotEmpty(t, final, "Expected non-empty result from OpenAI streaming")
	})
	
	t.Run("OpenAIGPT4oMini", func(t *testing.T) {
		result, err := b.TestOpenAIGPT4oMini(ctx, "test input")
		require.NoError(t, err)
		assert.NotEmpty(t, result, "Expected non-empty result from GPT-4o-mini")
	})
	
	t.Run("OpenAIWithFinishReasonError", func(t *testing.T) {
		_, err := b.TestOpenAIWithFinishReasonError(ctx, "test")
		assert.Error(t, err, "Expected finish reason error")
		assert.Contains(t, err.Error(), "Finish reason error:")
	})
}

// TestAnthropicProvider tests Anthropic/Claude provider functionality
// Reference: test_functions.py:620-630
func TestAnthropicProvider(t *testing.T) {
	ctx := context.Background()
	
	t.Run("AnthropicShorthand", func(t *testing.T) {
		result, err := b.TestAnthropicShorthand(ctx, "Mt Rainier is tall")
		require.NoError(t, err)
		assert.NotEmpty(t, result, "Expected non-empty result from Anthropic shorthand")
	})
	
	t.Run("AnthropicShorthandStreaming", func(t *testing.T) {
		stream, err := b.Stream.TestAnthropicShorthand(ctx, "Mt Rainier is tall")
		require.NoError(t, err)
		
		var final string
		for value := range stream {
			if value.IsFinal && value.Final() != nil {
				final = *value.Final()
			}
		}
		
		assert.NotEmpty(t, final, "Expected non-empty result from Anthropic streaming")
	})
	
	t.Run("PromptTestClaude", func(t *testing.T) {
		result, err := b.PromptTestClaude(ctx, "Mt Rainier is tall")
		require.NoError(t, err)
		assert.NotEmpty(t, result, "Expected non-empty result from Claude")
	})
	
	t.Run("TestVertexClaude", func(t *testing.T) {
		result, err := b.TestVertexClaude(ctx, "donkey kong")
		require.NoError(t, err)
		assert.Contains(t, strings.ToLower(result), "donkey kong")
	})
}

// TestGoogleProvider tests Google/Gemini provider functionality  
// Reference: test_functions.py:536-567
func TestGoogleProvider(t *testing.T) {
	ctx := context.Background()
	
	t.Run("TestGemini", func(t *testing.T) {
		result, err := b.TestGemini(ctx, "Dr. Pepper")
		require.NoError(t, err)
		assert.NotEmpty(t, result, "Expected non-empty result from Gemini")
	})
	
	t.Run("TestGeminiSystem", func(t *testing.T) {
		result, err := b.TestGeminiSystem(ctx, "Dr. Pepper")
		require.NoError(t, err)
		assert.NotEmpty(t, result, "Expected non-empty result from Gemini with system prompt")
	})
	
	t.Run("TestGeminiSystemAsChat", func(t *testing.T) {
		result, err := b.TestGeminiSystemAsChat(ctx, "Dr. Pepper")
		require.NoError(t, err)
		assert.NotEmpty(t, result, "Expected non-empty result from Gemini system as chat")
	})
	
	t.Run("TestGeminiStreaming", func(t *testing.T) {
		stream, err := b.Stream.TestGemini(ctx, "Dr. Pepper")
		require.NoError(t, err)
		
		var final string
		for value := range stream {
			if value.IsFinal && value.Final() != nil {
				final = *value.Final()
			}
		}
		
		assert.NotEmpty(t, final, "Expected non-empty result from Gemini streaming")
	})
	
	t.Run("TestGeminiOpenAiGeneric", func(t *testing.T) {
		result, err := b.TestGeminiOpenAiGeneric(ctx)
		require.NoError(t, err)
		assert.NotEmpty(t, result, "Expected non-empty result from Gemini OpenAI generic")
	})
	
	t.Run("TestVertex", func(t *testing.T) {
		result, err := b.TestVertex(ctx, "donkey kong")
		require.NoError(t, err)
		assert.Contains(t, strings.ToLower(result), "donkey kong")
	})
}

// TestAWSBedrockProvider tests AWS Bedrock provider functionality
// Reference: test_functions.py:571-602
func TestAWSBedrockProvider(t *testing.T) {
	ctx := context.Background()
	
	t.Run("TestAws", func(t *testing.T) {
		result, err := b.TestAws(ctx, "Mt Rainier is tall")
		require.NoError(t, err)
		assert.NotEmpty(t, result, "Expected non-empty result from AWS")
	})
	
	t.Run("TestAwsStreaming", func(t *testing.T) {
		stream, err := b.Stream.TestAws(ctx, "Tell me a story in 8 sentences.")
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
		
		assert.Greater(t, len(chunks), 1, "Expected more than one stream chunk")
		assert.NotEmpty(t, final, "Expected non-empty final result")
	})
	
	t.Run("TestAwsClaude37", func(t *testing.T) {
		result, err := b.TestAwsClaude37(ctx, "")
		require.NoError(t, err)
		assert.NotEmpty(t, result, "Expected non-empty result from AWS Claude 3.7")
	})
	
	t.Run("TestAwsInferenceProfile", func(t *testing.T) {
		result, err := b.TestAwsInferenceProfile(ctx, "Hello, world!")
		require.NoError(t, err)
		assert.NotEmpty(t, result, "Expected non-empty result from AWS inference profile")
	})
	
	t.Run("TestAwsInvalidRegion", func(t *testing.T) {
		_, err := b.TestAwsInvalidRegion(ctx, "lightning in a rock")
		assert.Error(t, err, "Expected error for invalid AWS region")
		assert.Contains(t, err.Error(), "DispatchFailure")
	})
}

// TestProviderWithDynamicClients tests providers with dynamic client configuration
// Reference: test_functions.py:779-826
func TestProviderWithDynamicClients(t *testing.T) {
	ctx := context.Background()
	
	t.Run("DynamicGeminiModels", func(t *testing.T) {
		clientRegistry := baml.NewClientRegistry()
		
		// Test with Gemini 2.0 Flash Thinking model
		clientRegistry.AddLlmClient("GeminiFlashThinking", "google-ai", map[string]interface{}{
			"model": "gemini-2.0-flash-thinking-exp-1219",
		})
		
		clientRegistry.SetPrimaryClient("GeminiFlashThinking")
		
		result, err := b.TestGemini(ctx, "sea", b.WithClientRegistry(clientRegistry))
		require.NoError(t, err)
		assert.NotEmpty(t, result, "Expected non-empty result from dynamic Gemini client")
	})
	
	t.Run("DynamicOpenAIClient", func(t *testing.T) {
		clientRegistry := baml.NewClientRegistry()
		
		clientRegistry.AddLlmClient("MyClient", "openai", map[string]interface{}{
			"model": "gpt-3.5-turbo",
		})
		
		clientRegistry.SetPrimaryClient("MyClient")
		
		result, err := b.ExpectFailure(ctx, b.WithClientRegistry(clientRegistry))
		require.NoError(t, err)
		assert.Contains(t, strings.ToLower(result), "london")
	})
}

// TestProviderSpecificFeatures tests provider-specific features
func TestProviderSpecificFeatures(t *testing.T) {
	ctx := context.Background()
	
	t.Run("OpenAIResponsesAPI", func(t *testing.T) {
		// Test openai-responses provider if available
		result, err := b.TestOpenAIResponses(ctx, "mountains")
		if err != nil {
			t.Skipf("OpenAI Responses API not available: %v", err)
		}
		
		assert.NotEmpty(t, result, "Expected non-empty result from OpenAI Responses")
	})
	
	t.Run("OpenAIResponsesReasoning", func(t *testing.T) {
		// Test OpenAI reasoning models
		result, err := b.TestOpenAIResponsesReasoning(ctx, "a world without horses, should be titled 'A World Without Horses'. Make it short, 2 sentences.")
		if err != nil {
			t.Skipf("OpenAI Responses Reasoning not available: %v", err)
		}
		
		assert.NotEmpty(t, result, "Expected non-empty result from OpenAI reasoning")
	})
	
	t.Run("OpenAIResponsesReasoningStreaming", func(t *testing.T) {
		stream, err := b.Stream.TestOpenAIResponsesReasoning(ctx, "a world without horses, should be titled 'A World Without Horses'. Make it short, 2 sentences.")
		if err != nil {
			t.Skipf("OpenAI Responses Reasoning streaming not available: %v", err)
		}
		
		var final string
		for value := range stream {
			if value.IsFinal && value.Final() != nil {
				final = *value.Final()
			}
		}
		
		assert.NotEmpty(t, final, "Expected non-empty result from reasoning streaming")
	})
	
	t.Run("GeminiThinking", func(t *testing.T) {
		// Test Gemini thinking models
		result, err := b.TestGeminiThinking(ctx, "A mesh barrier with mounting points designed for vehicle cargo areas")
		if err != nil {
			t.Skipf("Gemini thinking not available: %v", err)
		}
		
		assert.NotEmpty(t, result, "Expected non-empty result from Gemini thinking")
		// Should mention something related to cargo/vehicle/barrier
		resultLower := strings.ToLower(result)
		hasRelevantContent := strings.Contains(resultLower, "cargo") ||
			strings.Contains(resultLower, "vehicle") ||
			strings.Contains(resultLower, "barrier") ||
			strings.Contains(resultLower, "dog") ||
			strings.Contains(resultLower, "guard") ||
			strings.Contains(resultLower, "car")
		assert.True(t, hasRelevantContent, "Expected result to contain relevant content")
	})
	
	t.Run("TestThinking", func(t *testing.T) {
		// Test general thinking functionality
		result, err := b.TestThinking(ctx, "a world without horses, should be titled 'A World Without Horses'")
		if err != nil {
			t.Skipf("Thinking models not available: %v", err)
		}
		
		assert.NotEmpty(t, result.Title, "Expected non-empty title")
		assert.NotEmpty(t, result.Content, "Expected non-empty content")
		assert.NotEmpty(t, result.Characters, "Expected non-empty characters")
	})
	
	t.Run("TestThinkingStreaming", func(t *testing.T) {
		stream, err := b.Stream.TestThinking(ctx, "a world without horses, should be titled 'A World Without Horses'")
		if err != nil {
			t.Skipf("Thinking streaming not available: %v", err)
		}
		
		var final *types.CustomStory
		for value := range stream {
			if value.IsFinal && value.Final() != nil {
				final = value.Final()
			}
		}
		
		require.NotNil(t, final, "Expected final thinking response")
		assert.NotEmpty(t, final.Title, "Expected non-empty title")
		assert.NotEmpty(t, final.Content, "Expected non-empty content")
		assert.NotEmpty(t, final.Characters, "Expected non-empty characters")
	})
}

// TestProviderFallbacks tests fallback behavior between providers
// Reference: test_functions.py:509-518, 634-638
func TestProviderFallbacks(t *testing.T) {
	ctx := context.Background()
	
	t.Run("TestFallbackClient", func(t *testing.T) {
		result, err := b.TestFallbackClient(ctx)
		require.NoError(t, err)
		assert.NotEmpty(t, result, "Expected non-empty result from fallback client")
	})
	
	t.Run("TestSingleFallbackClient", func(t *testing.T) {
		// This should fail with connection error due to failing Azure fallback
		_, err := b.TestSingleFallbackClient(ctx)
		assert.Error(t, err, "Expected connection error from single fallback client")
		assert.Contains(t, err.Error(), "ConnectError")
	})
	
	t.Run("TestFallbackToShorthand", func(t *testing.T) {
		stream, err := b.Stream.TestFallbackToShorthand(ctx, "Mt Rainier is tall")
		require.NoError(t, err)
		
		var final string
		for value := range stream {
			if value.IsFinal && value.Final() != nil {
				final = *value.Final()
			}
		}
		
		assert.NotEmpty(t, final, "Expected non-empty result from fallback to shorthand")
	})
}

// TestProviderClientResponseTypes tests client response type handling
// Reference: test_functions.py:1502-1511
func TestProviderClientResponseTypes(t *testing.T) {
	ctx := context.Background()
	
	clientRegistry := baml.NewClientRegistry()
	
	// Add client with mismatched response type
	clientRegistry.AddLlmClient("temp_client", "openai", map[string]interface{}{
		"client_response_type": "anthropic",
		"model":                "gpt-4o",
	})
	
	clientRegistry.SetPrimaryClient("temp_client")
	
	// Should fail due to response type mismatch
	_, err := b.TestOpenAI(ctx, "test", b.WithClientRegistry(clientRegistry))
	assert.Error(t, err, "Expected error due to client response type mismatch")
}

// TestProviderSpecificErrors tests provider-specific error handling
func TestProviderSpecificErrors(t *testing.T) {
	ctx := context.Background()
	
	t.Run("OpenAIInvalidKey", func(t *testing.T) {
		clientRegistry := baml.NewClientRegistry()
		
		clientRegistry.AddLlmClient("InvalidClient", "openai", map[string]interface{}{
			"model":   "gpt-4o-mini",
			"api_key": "INVALID_KEY",
		})
		
		clientRegistry.SetPrimaryClient("InvalidClient")
		
		_, err := b.TestOpenAIGPT4oMini(ctx, "test", b.WithClientRegistry(clientRegistry))
		assert.Error(t, err, "Expected authentication error")
		assert.Contains(t, err.Error(), "401")
	})
	
	t.Run("OpenAIInvalidModel", func(t *testing.T) {
		clientRegistry := baml.NewClientRegistry()
		
		clientRegistry.AddLlmClient("InvalidModelClient", "openai", map[string]interface{}{
			"model": "random-model",
		})
		
		clientRegistry.SetPrimaryClient("InvalidModelClient")
		
		_, err := b.TestOpenAIGPT4oMini(ctx, "test", b.WithClientRegistry(clientRegistry))
		assert.Error(t, err, "Expected model not found error")
		assert.Contains(t, err.Error(), "404")
	})

	t.Run("AWSInvalidRegion", func(t *testing.T) {
		_, err := b.TestAwsInvalidRegion(ctx, "test input")
		assert.Error(t, err, "Expected error for invalid AWS region")
		assert.Contains(t, err.Error(), "DispatchFailure")
	})
}

// TestProviderRateLimit tests rate limiting behavior (if applicable)
func TestProviderRateLimit(t *testing.T) {
	// This test would need to be implemented based on specific rate limiting behavior
	// For now, it's a placeholder to show the testing pattern
	t.Skip("Rate limiting tests would require specific setup")
}

// TestProviderLongRunningRequests tests handling of long-running requests
func TestProviderLongRunningRequests(t *testing.T) {
	ctx := context.Background()
	
	// Test with a request that should take some time
	result, err := b.TestCaching(ctx, "Write a detailed story about space exploration", "1. be creative and detailed")
	require.NoError(t, err)
	assert.NotEmpty(t, result, "Expected non-empty result from long-running request")
}