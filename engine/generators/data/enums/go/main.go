package main

import (
	"context"
	"encoding/json"
	b "enums/baml_client"
	"enums/baml_client/types"
	"fmt"
)

func main() {
	res, err := b.ConsumeTestEnum(context.Background(), types.TestEnumD)
	if err != nil {
		panic(err)
	}
	fmt.Println(res)

	// Test enum with aliases
	result, err := b.FnTestAliasedEnumOutput(context.Background(), "mehhhhh")
	if err != nil {
		panic(err)
	}
	str, err := json.Marshal(result)
	if err != nil {
		panic(err)
	}
	fmt.Println(string(str))

	// Test enum with different inputs to get different variants
	testInputs := []string{
		"I am so angry right now",              // Should map to A (k1)
		"I'm feeling really happy",             // Should map to B (k22)
		"This makes me sad",                    // Should map to C (k11)
		"I don't understand",                   // Should map to D (k44)
		"I'm so excited!",                      // Should map to E (no alias)
		"k5",                                   // Should map to F (k5)
		"I'm bored and this is a long message", // Should map to G (k6)
	}

	for _, input := range testInputs {
		fmt.Printf("\nTesting input: %s\n", input)
		result, err := b.FnTestAliasedEnumOutput(context.Background(), input)
		if err != nil {
			panic(err)
		}
		str, err := json.Marshal(result)
		if err != nil {
			panic(err)
		}
		fmt.Println(string(str))
	}
}
