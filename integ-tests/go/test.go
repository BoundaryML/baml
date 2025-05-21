package main

import (
	"context"
	"fmt"

	b "example.com/integ-tests/baml_client"
	baml "github.com/boundaryml/baml/engine/language_client_go/pkg"
)

func main() {
	ctx := context.Background()

	registry := baml.NewClientRegistry()
	registry.AddLlmClient("a", "openai", map[string]any{"a": "b"})
	registry.SetPrimaryClient("a")

	v2, err := b.AaaSamOutputFormat(ctx, "oranges", b.WithClientRegistry(registry))
	if err != nil {
		panic(err)
	}
	fmt.Println(*v2)

	v2, err = b.AaaSamOutputFormat(ctx, "pineapple")
	if err != nil {
		panic(err)
	}
	fmt.Println(*v2)

	stream := b.Stream.AaaSamOutputFormat(ctx, "pineapple")
	for chunk := range stream {
		fmt.Println(chunk)
	}
}
