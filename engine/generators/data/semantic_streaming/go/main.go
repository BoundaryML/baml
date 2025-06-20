package main

import (
	"context"
	"fmt"
	b "semantic_streaming/baml_client"
)

func main() {
	ctx := context.Background()
	stream, err := b.Stream.MakeSemanticContainer(ctx)
	if err != nil {
		panic(err)
	}

	var referenceString *string
	var referenceInt *int64

	for msg := range stream {
		if msg.IsFinal {
			fmt.Println("final-----")
			fmt.Println(msg.Final())
		} else {
			fmt.Println("stream-----")
			fmt.Println(msg.Stream())
		}

		msgStream := msg.Stream()

		// Stability checks for numeric and @stream.done fields
		if msgStream.Sixteen_digit_number != nil {
			if referenceInt == nil {
				referenceInt = msgStream.Sixteen_digit_number
			} else {
				if *referenceInt != *msgStream.Sixteen_digit_number {
					panic(fmt.Sprintf("Sixteen_digit_number changed: %d != %d", *referenceInt, *msgStream.Sixteen_digit_number))
				}
			}
		}
		if msgStream.String_with_twenty_words != nil {
			if referenceString == nil {
				referenceString = msgStream.String_with_twenty_words
			} else {
				if *referenceString != *msgStream.String_with_twenty_words {
					panic(fmt.Sprintf("String_with_twenty_words changed: %s != %s", *referenceString, *msgStream.String_with_twenty_words))
				}
			}
		}

		// Checks for @stream.with_state (simulate with s_20_words length and final_string)
		if msgStream.Class_needed.S_20_words.Value != nil {
			words := len(splitWords(*msgStream.Class_needed.S_20_words.Value))
			if words < 3 && msgStream.Final_string == nil {
				fmt.Printf("%+v\n", msg)
				if msgStream.Class_needed.S_20_words.State != "Incomplete" {
					panic("Class_needed.S_20_words.State is not Incomplete")
				}
			}
		}
		if msgStream.Final_string != nil {
			if msgStream.Class_needed.S_20_words.State != "Complete" {
				panic("Class_needed.S_20_words.State is not Complete")
			}
		}

		// Checks for @stream.not_null
		for _, sub := range *msgStream.Three_small_things {
			if sub.I_16_digits == 0 {
				panic("three_small_things.i_16_digits is null/zero")
			}
		}
	}

	final, err := b.MakeSemanticContainer(ctx)
	if err != nil {
		panic(err)
	}
	fmt.Printf("Final: %+v\n", final)
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
