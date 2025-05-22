package main

import (
	"context"
	"encoding/json"
	"fmt"

	b "example.com/integ-tests/baml_client"
)

func main() {
	ctx := context.Background()

	// registry := baml.NewClientRegistry()
	// registry.AddLlmClient("a", "openai", map[string]any{"a": "b"})
	// registry.SetPrimaryClient("a")

	v2, err := b.AaaSamOutputFormat(ctx, "oranges")
	if err != nil {
		panic(err)
	}
	fmt.Println(*v2)

	// v2, err = b.AaaSamOutputFormat(ctx, "pineapple")
	// if err != nil {
	// 	panic(err)
	// }
	// fmt.Println(*v2)

	// stream := b.Stream.AaaSamOutputFormat(ctx, "pineapple")
	// for chunk := range stream {
	// 	fmt.Println(chunk)
	// }

	stream := b.Stream.AaaSamOutputFormat(ctx, "pineapple")
	for chunk := range stream {
		if chunk.IsFinal {
			jsonstr, err := json.Marshal(*chunk.Final())
			if err != nil {
				panic(err)
			}
			fmt.Println("---FINAL---")
			fmt.Println(string(jsonstr))
		} else {
			jsonstr, err := json.Marshal(chunk.Stream())
			if err != nil {
				panic(err)
			}
			fmt.Println(string(jsonstr))
		}
	}
}
