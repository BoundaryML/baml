package main

import (
	b "classes/baml_client"
	"classes/baml_client/types"
	"context"
	"testing"
)

func TestConsumeSimpleClass(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	cls := types.SimpleClass{}
	cls.Digits = 10
	cls.Words = "hello"

	result, err := b.ConsumeSimpleClass(ctx, cls)
	if err != nil {
		t.Fatalf("Error in ConsumeSimpleClass: %v", err)
	}

	// Basic validation that we got a result
	if result.Digits == 0 && result.Words == "" {
		t.Errorf("Expected non-empty result from ConsumeSimpleClass")
	}
}

func TestMakeSimpleClassStream(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	channel, err := b.Stream.MakeSimpleClass(ctx)
	if err != nil {
		t.Fatalf("Error starting MakeSimpleClass stream: %v", err)
	}

	gotFinal := false
	streamCount := 0

	for result := range channel {
		if result.IsFinal {
			gotFinal = true
			final := result.Final()
			if final == nil {
				t.Errorf("Expected non-nil final result")
			} else {
				// Validate the final result has expected fields
				if final.Digits == 0 && final.Words == "" {
					t.Errorf("Expected final result to have non-zero values")
				}
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
