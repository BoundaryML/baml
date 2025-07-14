package main

import (
	"context"
	b "sample/baml_client"
	"testing"

	baml "github.com/boundaryml/baml/engine/language_client_go/pkg"
)

func TestFoo(t *testing.T) {
	t.Parallel()
	ctx := context.Background()
	collector := baml.NewCollector()

	result, err := b.Foo(ctx, 8192, b.WithCollector(collector))
	if err != nil {
		t.Fatalf("Error in Foo: %v", err)
	}

	// Basic validation - check if the union has a valid variant
	if result.BamlTypeName() == "" {
		t.Errorf("Expected valid result from Foo")
	}

	// Test usage collection
	usage, err := collector.Usage()
	if err != nil {
		t.Fatalf("Error getting usage: %v", err)
	}

	inputTokens, err := usage.InputTokens()
	if err != nil {
		t.Fatalf("Error getting input tokens: %v", err)
	}
	if inputTokens <= 0 {
		t.Errorf("Expected positive input tokens, got %d", inputTokens)
	}

	outputTokens, err := usage.OutputTokens()
	if err != nil {
		t.Fatalf("Error getting output tokens: %v", err)
	}
	if outputTokens <= 0 {
		t.Errorf("Expected positive output tokens, got %d", outputTokens)
	}
}

func TestFooStream(t *testing.T) {
	t.Parallel()
	ctx := context.Background()
	collector := baml.NewCollector()

	channel, err := b.Stream.Foo(ctx, 8192, b.WithCollector(collector))
	if err != nil {
		t.Fatalf("Error starting Foo stream: %v", err)
	}

	gotFinal := false
	streamCount := 0

	for result := range channel {
		if result.IsFinal {
			gotFinal = true
			final := result.Final()
			if final == nil {
				t.Errorf("Expected non-nil final result")
			}
		} else {
			streamCount++
			stream := result.Stream()
			if stream == nil {
				t.Errorf("Expected non-nil stream result")
			}
		}
	}

	if !gotFinal {
		t.Errorf("Expected to receive a final result from stream")
	}

	// Test usage collection from stream
	usage, err := collector.Usage()
	if err != nil {
		t.Fatalf("Error getting usage: %v", err)
	}

	inputTokens, err := usage.InputTokens()
	if err != nil {
		t.Fatalf("Error getting input tokens: %v", err)
	}
	if inputTokens <= 0 {
		t.Errorf("Expected positive input tokens from stream, got %d", inputTokens)
	}

	outputTokens, err := usage.OutputTokens()
	if err != nil {
		t.Fatalf("Error getting output tokens: %v", err)
	}
	if outputTokens <= 0 {
		t.Errorf("Expected positive output tokens from stream, got %d", outputTokens)
	}
}
