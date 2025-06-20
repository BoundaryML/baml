package main

import (
	"context"
	"encoding/json"
	"fmt"
	b "recursive_types/baml_client"
	types "recursive_types/baml_client/types"
)

func main() {
	// result, err := b.Foo(context.Background(), 8192)
	// if err != nil {
	// 	panic(err)
	// }
	// fmt.Println(result)

	input := types.Union5FloatOrIntOrListJSONOrMapStringKeyJSONValueOrString{}
	input.SetString("Hello")
	output, err := b.JsonInput(context.Background(), &input)
	if err != nil {
		panic(err)
	}
	fmt.Println(output)

	channel, err := b.Stream.Foo(context.Background(), 8192)
	if err != nil {
		panic(err)
	}
	for result := range channel {
		if result.IsFinal {
			fmt.Println("final-----")
			str, err := json.Marshal(result.Final())
			if err != nil {
				panic(err)
			}
			fmt.Println(string(str))
		} else {
			fmt.Println("stream-----")
			str, err := json.Marshal(result.Stream())
			if err != nil {
				panic(err)
			}
			fmt.Println(string(str))
		}
	}
}
