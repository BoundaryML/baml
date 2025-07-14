package main

import (
	"context"
	"testing"
	b "unions/baml_client"
	types "unions/baml_client/types"
)

func TestJsonInputStream(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	category := types.Union2KresourceOrKservice__NewKservice()
	input := types.ExistingSystemComponent{
		Id:          1,
		Name:        "Hello",
		Type:        "service",
		Category:    category,
		Explanation: "Hello",
	}
	array := []types.ExistingSystemComponent{input}

	stream, err := b.Stream.JsonInput(ctx, array)
	if err != nil {
		t.Fatalf("Error starting JsonInput stream: %v", err)
	}

	gotFinal := false
	streamCount := 0

	for msg := range stream {
		if msg.IsFinal {
			gotFinal = true
			final := msg.Final()
			if final == nil {
				t.Errorf("Expected non-nil final result")
			} else if len(*final) == 0 {
				t.Errorf("Expected non-empty final result")
			}
		} else {
			streamCount++
			stream := msg.Stream()
			if stream == nil {
				t.Errorf("Expected non-nil stream result")
			}
		}
	}

	if !gotFinal {
		t.Errorf("Expected to receive a final result from stream")
	}
}

func TestJsonInput(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	category := types.Union2KresourceOrKservice__NewKservice()
	input := types.ExistingSystemComponent{
		Id:          1,
		Name:        "Hello",
		Type:        "service",
		Category:    category,
		Explanation: "Hello",
	}
	array := []types.ExistingSystemComponent{input}

	result, err := b.JsonInput(ctx, array)
	if err != nil {
		t.Fatalf("Error in JsonInput: %v", err)
	}

	// Basic validation
	if len(result) == 0 {
		t.Errorf("Expected non-empty result from JsonInput")
	}
}
