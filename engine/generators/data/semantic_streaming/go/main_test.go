package main

import (
	"context"
	"fmt"
	b "semantic_streaming/baml_client"
	"strings"
	"testing"

	baml "github.com/boundaryml/baml/engine/language_client_go/pkg"
)

func TestParseStream(t *testing.T) {
	t.Parallel()

	raw_text := `
{
	"sixteen_digit_number": 1234567890,
	"string_with_twenty_words": "Hello, world!",
	"class_1": {
	"i_16_digits": 1234567890,
	"s_20_words": "Hello, world!",
	"literal_status": "active"
	},
	"class_2": {
	"i_16_digits": 1234567890,
	"s_20_words": "Hello, world!",
	},
	"class_done_needed": {
	"i_16_digits": 1234567890,
	"s_20_words": "Hello, world!",
	},
	"class_needed": {
	"i_16_digits": 1234567890,
	"s_20_words": "Hello, world!",
	"literal_status": "active"
	},
	`
	// parse every raw_text[:i]
	for i := 0; i < len(raw_text); i++ {
		result, err := b.ParseStream.MakeSemanticContainer(raw_text[:i])
		if err != nil {
			msg := err.Error()
			if !(strings.Contains(msg, "Missing required field: class_done_needed") || strings.Contains(msg, "Missing required field: class_needed")) {
				t.Fatalf("Error in ParseStream: %v", msg)
			}
		} else {
			fmt.Printf("-> result: %+v\n", result)
		}
	}
}

func TestMakeSemanticContainerStream(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	stream, err := b.Stream.MakeSemanticContainer(ctx)
	if err != nil {
		t.Fatalf("Error starting MakeSemanticContainer stream: %v", err)
	}

	var referenceString *string
	var referenceInt *int64
	gotFinal := false

	for msg := range stream {
		if msg.Error != nil {
			t.Fatalf("Error in stream: %v", msg.Error)
		}
		if msg.IsFinal {
			gotFinal = true
			final := msg.Final()
			if final == nil {
				t.Errorf("Expected non-nil final result")
			}
		} else {
			msgStream := msg.Stream()
			if msgStream == nil {
				t.Errorf("Expected non-nil stream result")
				continue
			}

			fmt.Printf("msgStream: %+v\n", msgStream)

			// Stability checks for numeric and @stream.done fields
			if msgStream.Sixteen_digit_number != nil {
				if referenceInt == nil {
					referenceInt = msgStream.Sixteen_digit_number
				} else {
					if *referenceInt != *msgStream.Sixteen_digit_number {
						t.Errorf("Sixteen_digit_number changed: %d != %d", *referenceInt, *msgStream.Sixteen_digit_number)
					}
				}
			}
			if msgStream.String_with_twenty_words != nil {
				if referenceString == nil {
					referenceString = msgStream.String_with_twenty_words
				} else {
					if *referenceString != *msgStream.String_with_twenty_words {
						t.Errorf("String_with_twenty_words changed: %s != %s", *referenceString, *msgStream.String_with_twenty_words)
					}
				}
			}

			// Checks for @stream.with_state (simulate with s_20_words length and final_string)
			if msgStream.Class_needed.S_20_words.Value != nil {
				words := len(splitWords(*msgStream.Class_needed.S_20_words.Value))
				if words < 3 && msgStream.Final_string == nil {
					if msgStream.Class_needed.S_20_words.State != baml.StreamStatePending {
						t.Errorf("Class_needed.S_20_words.State is not Pending: %s", msgStream.Class_needed.S_20_words.State)
					}
				}
			}
			if msgStream.Final_string != nil {
				if msgStream.Class_needed.S_20_words.State != baml.StreamStatePending {
					t.Errorf("Class_needed.S_20_words.State is not Complete: %s", msgStream.Class_needed.S_20_words.State)
				}
			}

			// Checks for @stream.not_null
			for _, sub := range msgStream.Three_small_things {
				if sub.I_16_digits == 0 {
					t.Errorf("three_small_things.i_16_digits is null/zero")
				}
			}
		}
	}

	if !gotFinal {
		t.Errorf("Expected to receive a final result from stream")
	}
}

func TestMakeSemanticContainer(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.MakeSemanticContainer(ctx)
	if err != nil {
		t.Fatalf("Error in MakeSemanticContainer: %v", err)
	}

	// Basic validation - check if struct has valid data
	if result.BamlTypeName() == "" {
		t.Errorf("Expected valid result from MakeSemanticContainer")
	}
}

func splitWords(s string) []string {
	// Simple split on spaces, ignoring multiple spaces
	var words []string
	start := -1
	for i, c := range s {
		if c != ' ' && start == -1 {
			start = i
		}
		if c == ' ' && start != -1 {
			words = append(words, s[start:i])
			start = -1
		}
	}
	if start != -1 {
		words = append(words, s[start:])
	}
	return words
}
