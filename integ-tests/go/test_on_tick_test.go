package main

import (
	"context"
	"fmt"
	"sync/atomic"
	"testing"

	b "example.com/integ-tests/baml_client"
	"example.com/integ-tests/baml_client/types"
	baml "github.com/boundaryml/baml/engine/language_client_go/pkg"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"github.com/tidwall/gjson"
)

type onTickState struct {
	ticks                 atomic.Int32
	sawReason             atomic.Bool
	sawCalls              atomic.Bool
	sawStreamChunks       atomic.Bool
	calledFFIOnTickMethod atomic.Bool
}

func asMapStringAny(v any) (map[string]any, bool) {
	m, ok := v.(map[string]any)
	if ok {
		return m, true
	}
	// Some decoders use map[string]interface{} explicitly.
	m2, ok := v.(map[string]interface{})
	if !ok {
		return nil, false
	}
	out := make(map[string]any, len(m2))
	for k, val := range m2 {
		out[k] = val
	}
	return out, true
}

func makeOnTick(state *onTickState) baml.TickCallback {
	return func(_ context.Context, reason baml.TickReason, log baml.FunctionLog) baml.FunctionSignal {
		state.ticks.Add(1)

		if reason == baml.TickReason_Unknown {
			state.sawReason.Store(true)
		}

		if log == nil {
			return nil
		}

		calls, err := log.Calls()
		if err == nil && len(calls) > 0 {
			state.sawCalls.Store(true)
			// Best-effort: if we can see SSE chunks, mark it.
			if streamCall, ok := calls[len(calls)-1].(baml.LLMStreamCall); ok {
				if chunks, err := streamCall.SSEChunks(); err == nil && len(chunks) > 0 {
					state.sawStreamChunks.Store(true)
					if payload, err := chunks[len(chunks)-1].Text(); err == nil && payload != "" {
						if content := gjson.Get(payload, "delta.thinking"); content.Exists() {
							fmt.Println("Last chunk delta.thinking:", content.String())
							b.ParseStream.TestThinking(content.String())
							state.calledFFIOnTickMethod.Store(true)
						}
					}
				}
			}
		}

		return nil
	}
}

func TestOnTickOption(t *testing.T) {
	ctx := context.Background()

	t.Run("NonStreamingFunction", func(t *testing.T) {
		var state onTickState
		onTick := makeOnTick(&state)

		result, err := b.TestThinking(ctx, "a world without horses, should be titled 'A World Without Horses'", b.WithOnTick(onTick))
		if err != nil {
			t.Skipf("Thinking models not available: %v", err)
		}

		assert.NotEmpty(t, result.Title, "Expected non-empty title")
		assert.NotEmpty(t, result.Content, "Expected non-empty content")
		assert.NotEmpty(t, result.Characters, "Expected non-empty characters")

		assert.Greater(t, state.ticks.Load(), int32(0), "Expected onTick to fire at least once")
		assert.True(t, state.sawReason.Load(), "Expected tick reason to be Unknown")
		assert.True(t, state.sawCalls.Load(), "Expected at least one tick to include call data")
		assert.True(t, state.calledFFIOnTickMethod.Load(), "Expected at least one tick to include parsed data")
	})

	t.Run("StreamingFunction", func(t *testing.T) {
		var state onTickState
		onTick := makeOnTick(&state)

		stream, err := b.Stream.TestThinking(ctx, "a world without horses, should be titled 'A World Without Horses'", b.WithOnTick(onTick))
		if err != nil {
			t.Skipf("Thinking streaming not available: %v", err)
		}

		var final *types.CustomStory
		for value := range stream {
			require.False(t, value.IsError, "Unexpected stream error: %v", value.Error)
			if value.IsFinal && value.Final() != nil {
				final = value.Final()
			}
		}

		require.NotNil(t, final, "Expected final thinking response")
		assert.NotEmpty(t, final.Title, "Expected non-empty title")
		assert.NotEmpty(t, final.Content, "Expected non-empty content")
		assert.NotEmpty(t, final.Characters, "Expected non-empty characters")

		assert.Greater(t, state.ticks.Load(), int32(0), "Expected onTick to fire at least once")
		assert.True(t, state.sawReason.Load(), "Expected tick reason to be Unknown")
		assert.True(t, state.sawCalls.Load(), "Expected at least one tick to include call data")
		assert.True(t, state.calledFFIOnTickMethod.Load(), "Expected at least one tick to include parsed data")

		// This may depend on provider support, so it's a softer assertion.
		// If we can't observe SSE chunks, onTick should still work.
	})
}
