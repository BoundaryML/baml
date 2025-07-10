package main

import (
	"context"
	b "enums/baml_client"
	"enums/baml_client/types"
	"testing"
)

func TestConsumeTestEnum(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.ConsumeTestEnum(ctx, types.TestEnumConfused)
	if err != nil {
		t.Fatalf("Error in ConsumeTestEnum: %v", err)
	}

	// Basic validation that we got a result
	if result == "" {
		t.Errorf("Expected non-empty result from ConsumeTestEnum")
	}
}

func TestFnTestAliasedEnumOutput(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	// Test with "mehhhhh" input
	result, err := b.FnTestAliasedEnumOutput(ctx, "mehhhhh")
	if err != nil {
		t.Fatalf("Error in FnTestAliasedEnumOutput: %v", err)
	}

	if result != types.TestEnumBored {
		t.Errorf("Expected result to be TestEnumBored, got %v", result)
	}
}

func TestFnTestAliasedEnumOutputVariants(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	// Test different inputs to get different variants
	testCases := []struct {
		name     string
		input    string
		expected types.TestEnum
	}{
		{"Angry", "I am so angry right now", types.TestEnumAngry},              // Should map to Angry (k1)
		{"Happy", "I'm feeling really happy", types.TestEnumHappy},             // Should map to Happy (k22)
		{"Sad", "This makes me sad", types.TestEnumSad},                        // Should map to Sad (k11)
		{"Confused", "I don't understand", types.TestEnumConfused},             // Should map to Confused (k44)
		{"Excited", "I'm so excited!", types.TestEnumExcited},                  // Should map to Excited (no alias)
		{"Excited2", "k5", types.TestEnumExcited},                              // Should map to Exclamation (k5)
		{"Bored", "I'm bored and this is a long message", types.TestEnumBored}, // Should map to Bored (k6)
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			result, err := b.FnTestAliasedEnumOutput(ctx, tc.input)
			if err != nil {
				t.Errorf("Error testing input '%s': %v", tc.input, err)
			}
			if result != tc.expected {
				t.Errorf("Expected result to be %v, got %v", tc.expected, result)
			}
		})
	}
}
