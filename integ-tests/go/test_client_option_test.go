package main

import (
	"context"
	"testing"

	b "example.com/integ-tests/baml_client"
	baml "github.com/boundaryml/baml/engine/language_client_go/pkg"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestWithClientOption tests the WithClient shorthand option
func TestWithClientOption(t *testing.T) {
	ctx := context.Background()

	t.Run("WithClientRoutesToCorrectClient", func(t *testing.T) {
		// Use WithClient to override the default client
		// Claude is defined in the BAML files
		result, err := b.TestOpenAI(ctx, "Say hello", b.WithClient("Claude"))
		require.NoError(t, err)
		assert.NotEmpty(t, result)
	})

	t.Run("WithClientTakesPrecedenceOverWithClientRegistry", func(t *testing.T) {
		// Create a client registry with GPT4 as primary
		cr := baml.NewClientRegistry()
		cr.AddLlmClient("MyGPT", "openai", map[string]interface{}{
			"model": "gpt-4o-mini",
		})
		cr.SetPrimaryClient("MyGPT")

		// WithClient should override the client registry's primary
		// Use Claude which is defined in BAML files
		result, err := b.TestOpenAI(ctx, "Say hello",
			b.WithClientRegistry(cr),
			b.WithClient("Claude"),
		)
		require.NoError(t, err)
		assert.NotEmpty(t, result)
	})

	t.Run("WithClientRegistryStillWorks", func(t *testing.T) {
		// Verify that WithClientRegistry without WithClient still works
		cr := baml.NewClientRegistry()
		cr.AddLlmClient("MyGPT", "openai", map[string]interface{}{
			"model": "gpt-4o-mini",
		})
		cr.SetPrimaryClient("MyGPT")

		result, err := b.TestOpenAI(ctx, "Say hello", b.WithClientRegistry(cr))
		require.NoError(t, err)
		assert.NotEmpty(t, result)
	})

	t.Run("WithClientAndCollector", func(t *testing.T) {
		// Test combining WithClient with other options
		collector, err := b.NewCollector("client-option-test")
		require.NoError(t, err)

		result, err := b.TestOpenAI(ctx, "Say hello",
			b.WithClient("Claude"),
			b.WithCollector(collector),
		)
		require.NoError(t, err)
		assert.NotEmpty(t, result)

		// Verify collector captured the call
		logs, err := collector.Logs()
		require.NoError(t, err)
		assert.Len(t, logs, 1)
	})
}
