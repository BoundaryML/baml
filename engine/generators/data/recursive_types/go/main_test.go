package main

import (
	"context"
	b "recursive_types/baml_client"
	types "recursive_types/baml_client/types"
	"testing"
)

func TestFoo(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.Foo(ctx, 8192)
	if err != nil {
		t.Fatalf("Error in Foo: %v", err)
	}

	// Basic validation that we got a result
	if result == nil {
		t.Errorf("Expected non-nil result from Foo")
	}
}

func TestJsonInput(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	input := types.Union5FloatOrIntOrListJSONOrMapStringKeyJSONValueOrString{}
	input.SetString("Hello")

	result, err := b.JsonInput(ctx, &input)
	if err != nil {
		t.Fatalf("Error in JsonInput: %v", err)
	}

	// Basic validation
	if result == nil {
		t.Errorf("Expected non-nil result from JsonInput")
	}
}

func TestFooStream(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	channel, err := b.Stream.Foo(ctx, 8192)
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
}
