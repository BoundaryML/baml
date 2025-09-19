package main

import (
	"context"
	"strings"
	"testing"

	b "example.com/integ-tests/baml_client"
	"example.com/integ-tests/baml_client/types"
	baml "github.com/boundaryml/baml/engine/language_client_go/pkg"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestHTTPErrors tests various HTTP status code errors
// Reference: test_functions.py:1194-1226
func TestHTTPErrors(t *testing.T) {
	ctx := context.Background()
	
	t.Run("HTTP401InvalidAPIKey", func(t *testing.T) {
		clientRegistry := baml.NewClientRegistry()
		
		clientRegistry.AddLlmClient("MyClient", "openai", map[string]interface{}{
			"model":   "gpt-4o-mini",
			"api_key": "INVALID_KEY",
		})
		
		clientRegistry.SetPrimaryClient("MyClient")
		
		_, err := b.TestOpenAIGPT4oMini(ctx, "test", b.WithClientRegistry(clientRegistry))
		assert.Error(t, err, "Expected HTTP 401 error")
		
		// Should be a client HTTP error with status code 401
		assert.Contains(t, err.Error(), "401")
		
		// Verify it's the right type of error
		errorMsg := strings.ToLower(err.Error())
		assert.True(t,
			strings.Contains(errorMsg, "unauthorized") ||
				strings.Contains(errorMsg, "invalid") ||
				strings.Contains(errorMsg, "authentication") ||
				strings.Contains(errorMsg, "401"),
			"Expected authentication-related error, got: %s", err.Error())
	})
	
	t.Run("HTTP404ModelNotFound", func(t *testing.T) {
		clientRegistry := baml.NewClientRegistry()
		
		clientRegistry.AddLlmClient("MyClient", "openai", map[string]interface{}{
			"model": "random-model-that-does-not-exist",
		})
		
		clientRegistry.SetPrimaryClient("MyClient")
		
		_, err := b.TestOpenAIGPT4oMini(ctx, "test", b.WithClientRegistry(clientRegistry))
		assert.Error(t, err, "Expected HTTP 404 error")
		
		// Should be a client HTTP error with status code 404
		assert.Contains(t, err.Error(), "404")
		
		// Verify it's the right type of error
		errorMsg := strings.ToLower(err.Error())
		assert.True(t,
			strings.Contains(errorMsg, "not found") ||
				strings.Contains(errorMsg, "model") ||
				strings.Contains(errorMsg, "404"),
			"Expected model not found error, got: %s", err.Error())
	})
	
	t.Run("HTTPConnectionError", func(t *testing.T) {
		clientRegistry := baml.NewClientRegistry()
		
		clientRegistry.AddLlmClient("MyClient", "openai", map[string]interface{}{
			"model":    "gpt-4o-mini",
			"base_url": "https://does-not-exist.com",
		})
		
		clientRegistry.SetPrimaryClient("MyClient")
		
		_, err := b.TestOpenAIGPT4oMini(ctx, "test", b.WithClientRegistry(clientRegistry))
		assert.Error(t, err, "Expected connection error")
		
		// Should be a connection-related error
		errorMsg := strings.ToLower(err.Error())
		assert.True(t,
			strings.Contains(errorMsg, "connect") ||
				strings.Contains(errorMsg, "connection") ||
				strings.Contains(errorMsg, "network") ||
				strings.Contains(errorMsg, "dns") ||
				strings.Contains(errorMsg, "timeout"),
			"Expected connection-related error, got: %s", err.Error())
	})
}

// TestValidationErrors tests input validation errors
// Reference: test_functions.py:1184-1192, 1228-1230
func TestValidationErrors(t *testing.T) {
	ctx := context.Background()
	t.Run("ValidationError", func(t *testing.T) {
		// Test functions that should fail validation
		_, err := b.DummyOutputFunction(ctx, "dummy input")
		assert.Error(t, err, "Expected validation error")
		
		errorMsg := strings.ToLower(err.Error())
		assert.True(t,
			strings.Contains(errorMsg, "validation") ||
				strings.Contains(errorMsg, "parse") ||
				strings.Contains(errorMsg, "failed"),
			"Expected validation error, got: %s", err.Error())
	})
}

// TestSerializationErrors tests serialization and deserialization errors
// Reference: test_functions.py:1080-1116
func TestSerializationErrors(t *testing.T) {
	ctx := context.Background()
	
	t.Run("SerializationException", func(t *testing.T) {
		// Test function that should fail to serialize response
		_, err := b.DummyOutputFunction(ctx, "dummy input")
		assert.Error(t, err, "Expected serialization exception")
		
		assert.Contains(t, err.Error(), "Failed to coerce", "Expected coercion failure message")
	})
	
	t.Run("StreamSerializationException", func(t *testing.T) {
		// Test streaming serialization exception
		stream, err := b.Stream.DummyOutputFunction(ctx, "dummy input")
		if err != nil {
			// Error occurred before streaming started
			assert.Contains(t, err.Error(), "Failed to coerce")
			return
		}
		
		// Error should occur during streaming
		var streamError error
		for value := range stream {
			if value.IsError {
				streamError = value.Error
				break
			}
		}
		
		assert.Error(t, streamError, "Expected serialization error during streaming")
		if streamError != nil {
			assert.Contains(t, streamError.Error(), "Failed to coerce")
		}
	})
}

// TestNetworkErrors tests network connectivity and timeout errors
func TestNetworkErrors(t *testing.T) {
	ctx := context.Background()
	
	t.Run("ConnectionTimeout", func(t *testing.T) {
		clientRegistry := baml.NewClientRegistry()
		
		// Use a URL that will timeout (non-routable IP)
		clientRegistry.AddLlmClient("TimeoutClient", "openai", map[string]interface{}{
			"model":    "gpt-4o-mini",
			"base_url": "http://10.255.255.1", // Non-routable IP
		})
		clientRegistry.SetPrimaryClient("TimeoutClient")
		
		_, err := b.TestOpenAIGPT4oMini(ctx, "test", b.WithClientRegistry(clientRegistry))
		assert.Error(t, err, "Expected network timeout error")
		
		errorMsg := strings.ToLower(err.Error())
		assert.True(t,
			strings.Contains(errorMsg, "connect error") ||
				strings.Contains(errorMsg, "timedout") ||
				strings.Contains(errorMsg, "network"),
			"Expected network-related error, got: %s", err.Error())
	})
	
	t.Run("DNSResolutionError", func(t *testing.T) {
		clientRegistry := baml.NewClientRegistry()
		
		// Use a domain that doesn't exist
		clientRegistry.AddLlmClient("DNSClient", "openai", map[string]interface{}{
			"model":    "gpt-4o-mini",
			"base_url": "https://this-domain-definitely-does-not-exist-12345.com",
		})
		clientRegistry.SetPrimaryClient("DNSClient")
		
		_, err := b.TestOpenAIGPT4oMini(ctx, "test", b.WithClientRegistry(clientRegistry))
		assert.Error(t, err, "Expected DNS resolution error")
		
		errorMsg := strings.ToLower(err.Error())
		assert.True(t,
			strings.Contains(errorMsg, "dns") ||
				strings.Contains(errorMsg, "resolve") ||
				strings.Contains(errorMsg, "host") ||
				strings.Contains(errorMsg, "connection"),
			"Expected DNS-related error, got: %s", err.Error())
	})
}

// TestConstraintErrors tests constraint validation errors
// Reference: test_functions.py:114-126
func TestConstraintErrors(t *testing.T) {
	ctx := context.Background()
	
	t.Run("MalformedConstraintReturn", func(t *testing.T) {
		_, err := b.ReturnMalformedConstraints(ctx, 1)
		assert.Error(t, err, "Expected error for malformed constraints")
		assert.Contains(t, err.Error(), "Failed to coerce value")
	})
	
	t.Run("MalformedConstraintInput", func(t *testing.T) {
		_, err := b.UseMalformedConstraints(ctx, types.MalformedConstraints2{Foo: 2})
		assert.Error(t, err, "Expected error when using malformed constraints")
		assert.Contains(t, err.Error(), "number has no method named length")
	})
	
	t.Run("BlockConstraintFailure", func(t *testing.T) {
		blockConstraint := types.BlockConstraintForParam{
			Bcfp:  1,
			Bcfp2: "too long!",
		}
		
		_, err := b.UseBlockConstraint(ctx, blockConstraint)
		assert.Error(t, err, "Expected error for failing block constraint")
		assert.Contains(t, err.Error(), "Failed assert: hi")
	})
}

// TestFinishReasonErrors tests finish reason related errors
// Reference: test_functions.py:522-526
func TestFinishReasonErrors(t *testing.T) {
	ctx := context.Background()
	
	_, err := b.TestOpenAIWithFinishReasonError(ctx, "test")
	assert.Error(t, err, "Expected finish reason error")
	assert.Contains(t, err.Error(), "Finish reason")
}

// TestStreamingErrors tests errors that occur during streaming
func TestStreamingErrors(t *testing.T) {
	ctx := context.Background()
	
	t.Run("StreamSerializationError", func(t *testing.T) {
		// Test streaming function that should fail serialization
		stream, err := b.Stream.DummyOutputFunction(ctx, "dummy input")
		if err != nil {
			// Error before streaming starts
			assert.Contains(t, err.Error(), "Failed to coerce")
			return
		}
		
		var hasError bool
		for value := range stream {
			if value.IsError {
				hasError = true
				assert.Contains(t, value.Error.Error(), "Failed to coerce")
				break
			}
		}
		
		assert.True(t, hasError, "Expected error during streaming")
	})
	
	t.Run("StreamingValidationError", func(t *testing.T) {
		// Test streaming with failing assertion
		stream, err := b.Stream.StreamFailingAssertion(ctx, "Yoshimi battles the pink robots", 300)
		require.NoError(t, err)
		
		var finalError error
		for value := range stream {
			if value.IsFinal {
				if value.IsError {
					finalError = value.Error
				}
			}
		}
		
		assert.Error(t, finalError, "Expected validation error in final result")
		if finalError != nil {
			errorMsg := strings.ToLower(finalError.Error())
			assert.True(t,
				strings.Contains(errorMsg, "validation") ||
					strings.Contains(errorMsg, "assert") ||
					strings.Contains(errorMsg, "failed"),
				"Expected validation error, got: %s", finalError.Error())
		}
	})
}

// TestConcurrentErrorHandling tests error handling with concurrent operations
func TestConcurrentErrorHandling(t *testing.T) {
	ctx := context.Background()
	
	const numConcurrent = 3
	errors := make(chan error, numConcurrent)
	
	// Start multiple operations that should fail
	for i := 0; i < numConcurrent; i++ {
		go func() {
			_, err := b.DummyOutputFunction(ctx, "dummy input")
			errors <- err
		}()
	}
	
	// Collect errors
	var errorCount int
	for i := 0; i < numConcurrent; i++ {
		err := <-errors
		if err != nil {
			errorCount++
			assert.Contains(t, err.Error(), "Failed to coerce")
		}
	}
	
	assert.Equal(t, numConcurrent, errorCount, "Expected all concurrent operations to fail")
}

// TestErrorMessageFormat tests error message formatting and content
// Reference: test_functions.py:1241-1255
func TestErrorMessageFormat(t *testing.T) {
	ctx := context.Background()
	
	_, err := b.DummyOutputFunction(ctx, "blah")
	assert.Error(t, err, "Expected validation error")
	
	// Error should contain useful information
	errorMsg := err.Error()
	assert.Contains(t, errorMsg, "Failed to coerce", "Expected parse failure message")
	
	// For BAML validation errors, we might expect additional context
	// The exact format depends on the Go client implementation
	assert.NotEmpty(t, errorMsg, "Expected non-empty error message")
}

// TestErrorRecovery tests error recovery patterns
func TestErrorRecovery(t *testing.T) {
	ctx := context.Background()
	
	// Test that after an error, the system can still handle valid requests
	_, err := b.DummyOutputFunction(ctx, "should fail")
	assert.Error(t, err, "Expected first call to fail")
	
	// Second call should work normally
	result, err := b.TestOpenAIGPT4oMini(ctx, "should work")
	require.NoError(t, err)
	assert.NotEmpty(t, result, "Expected second call to succeed after error")
}

// TestClientRegistryErrors tests errors related to client registry
func TestClientRegistryErrors(t *testing.T) {
	ctx := context.Background()
	
	t.Run("NonexistentClient", func(t *testing.T) {
		clientRegistry := baml.NewClientRegistry()
		
		clientRegistry.SetPrimaryClient("DoesNotExist")
		_, err := b.TestOpenAIGPT4oMini(ctx, "test", b.WithClientRegistry(clientRegistry))
		
		assert.Error(t, err, "Expected error for non-existent client")
	})
	
	t.Run("ClientResponseTypeMismatch", func(t *testing.T) {
		clientRegistry := baml.NewClientRegistry()
		
		clientRegistry.AddLlmClient("MismatchClient", "openai", map[string]interface{}{
			"client_response_type": "anthropic",
			"model":                "gpt-4o",
		})
		clientRegistry.SetPrimaryClient("MismatchClient")
		
		_, err := b.TestOpenAI(ctx, "test", b.WithClientRegistry(clientRegistry))
		assert.Error(t, err, "Expected error for response type mismatch")
	})
}

// TestFallbackErrors tests that fallback error chains include all failure details
// Reference: test_fallbacks.py:test_fallback_errors()
func TestFallbackErrors(t *testing.T) {
	ctx := context.Background()
	
	// Call function with fallback chain that always fails
	_, err := b.FnFallbackAlwaysFails(ctx, "lorem ipsum")
	
	// Should get an error since all fallback clients will fail
	assert.Error(t, err, "Expected error from failing fallback chain")
	
	errorMsg := err.Error()
	t.Logf("Fallback error message: %s", errorMsg)
	
	// Verify that error message includes information about all failed clients
	// The fallback client is configured with these non-existent models:
	// "openai/gpt-0-noexist", "openai/gpt-1-noexist", "openai/gpt-2-noexist"
	assert.Contains(t, errorMsg, "gpt-0-noexist", "Expected first fallback client in error")
	assert.Contains(t, errorMsg, "gpt-1-noexist", "Expected second fallback client in error")
	assert.Contains(t, errorMsg, "gpt-2-noexist", "Expected third fallback client in error")
	
	// Verify the error message is comprehensive and informative
	assert.NotEmpty(t, errorMsg, "Expected non-empty error message")
	assert.True(t, len(errorMsg) > 50, "Expected detailed error message, got: %s", errorMsg)
}
