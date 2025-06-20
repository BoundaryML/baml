package main

import (
	"context"
	"fmt"

	b "example.com/integ-tests/baml_client"
	baml "github.com/boundaryml/baml/engine/language_client_go/pkg"
)

func main() {
	ctx := context.Background()

	collector := baml.NewCollector()

	v2, err := b.AaaSamOutputFormat(ctx, "oranges", b.WithCollector(collector))
	if err != nil {
		panic(err)
	}
	fmt.Println(*v2)

	usage, err := collector.Usage()
	if err != nil {
		panic(err)
	}
	input_tokens, err := usage.InputTokens()
	if err != nil {
		panic(err)
	}
	output_tokens, err := usage.OutputTokens()
	if err != nil {
		panic(err)
	}
	fmt.Printf("input_tokens: %d\n", input_tokens)
	fmt.Printf("output_tokens: %d\n", output_tokens)

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
