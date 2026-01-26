package main

import (
	"context"
	"testing"

	b "example.com/integ-tests/baml_client"
	"github.com/stretchr/testify/require"
)

// TestTagsPassthrough tests that function-specific tags can be passed and don't cause errors
func TestTagsPassthrough(t *testing.T) {
	ctx := context.Background()

	collector, err := b.NewCollector("tags-test-collector")
	require.NoError(t, err)

	// Test function call with tags - this verifies the tags parameter works
	functionTags := map[string]string{
		"callId":     "first",
		"version":    "v1",
		"test_type":  "go_integration",
		"component":  "baml_client",
	}

	// First call with tags
	result1, err := b.TestOpenAIGPT4oMini(
		ctx,
		"hello - call 1",
		b.WithCollector(collector),
		b.WithTags(functionTags),
	)
	require.NoError(t, err)
	require.NotEmpty(t, result1)

	// Second call with different tags
	functionTags2 := map[string]string{
		"callId":    "second",
		"version":   "v2",
		"test_type": "go_integration",
		"extra":     "data",
	}

	result2, err := b.TestOpenAIGPT4oMini(
		ctx,
		"hello - call 2",
		b.WithCollector(collector),
		b.WithTags(functionTags2),
	)
	require.NoError(t, err)
	require.NotEmpty(t, result2)

	// Verify collector received function calls
	logs, err := collector.Logs()
	require.NoError(t, err)
	require.Len(t, logs, 2)

	// Both calls should have completed successfully
	for i, log := range logs {
		functionName, err := log.FunctionName()
		require.NoError(t, err)
		require.Equal(t, "TestOpenAIGPT4oMini", functionName)

		// Verify call completed (not an error)
		calls, err := log.Calls()
		require.NoError(t, err)
		require.NotEmpty(t, calls)

		selected, err := calls[0].Selected()
		require.NoError(t, err)
		require.True(t, selected, "Call %d should have been selected", i+1)
	}

	// If we get here, tags were successfully passed without causing errors
	t.Logf("Successfully passed tags through Go client for %d function calls", len(logs))
}

// TestTagsWithEnvironmentVars tests tags work in combination with environment variables
func TestTagsWithEnvironmentVars(t *testing.T) {
	ctx := context.Background()

	collector, err := b.NewCollector("tags-env-test-collector")
	require.NoError(t, err)

	// Test with both tags and environment variables
	functionTags := map[string]string{
		"environment": "test",
		"scenario":    "tags_with_env",
	}

	envVars := map[string]string{
		"CUSTOM_VAR": "test_value",
	}

	result, err := b.TestOpenAIGPT4oMini(
		ctx,
		"test with tags and env vars",
		b.WithCollector(collector),
		b.WithTags(functionTags),
		b.WithEnv(envVars),
	)
	require.NoError(t, err)
	require.NotEmpty(t, result)

	// Verify collector received the function call
	logs, err := collector.Logs()
	require.NoError(t, err)
	require.Len(t, logs, 1)

	log := logs[0]
	functionName, err := log.FunctionName()
	require.NoError(t, err)
	require.Equal(t, "TestOpenAIGPT4oMini", functionName)

	// Verify call was successful
	calls, err := log.Calls()
	require.NoError(t, err)
	require.NotEmpty(t, calls)

	selected, err := calls[0].Selected()
	require.NoError(t, err)
	require.True(t, selected)

	t.Log("Successfully passed tags with environment variables through Go client")
}