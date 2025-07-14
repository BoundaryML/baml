package main

import (
	b "asserts/baml_client"
	"asserts/baml_client/stream_types"
	"context"
	"fmt"
	"testing"
)

func TestPersonTest(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.PersonTest(ctx)
	if err != nil {
		t.Fatalf("Error in PersonTest: %v", err)
	}
	fmt.Println(result)
}

func TestPersonTestStream(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	channel, err := b.Stream.PersonTest(ctx)
	if err != nil {
		t.Fatalf("Error starting PersonTest stream: %v", err)
	}

	gotFinal := false
	streamCount := 0
	streamCumulative := []stream_types.Person{}

	for result := range channel {
		if result.IsError {
			t.Fatalf("Error in stream: %v", result.Error)
		}

		if result.IsFinal {
			gotFinal = true
			final := result.Final()
			fmt.Println("Final", final)
			if final.Age <= 0 {
				t.Errorf("Expected age to be greater than 0")
			}
		} else {
			streamCount++
			stream := result.Stream()
			fmt.Println("Stream", stream)
			streamCumulative = append(streamCumulative, *stream)
		}
	}

	if !gotFinal {
		t.Fatalf("Expected to not receive a final result from stream")
	}

	if len(streamCumulative) != streamCount {
		t.Fatalf("Expected %d stream results, got %d", streamCount, len(streamCumulative))
	}

}
