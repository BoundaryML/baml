package main

import (
	"context"
	"encoding/json"
	"fmt"
	b "sample/baml_client"

	baml "github.com/boundaryml/baml/engine/language_client_go/pkg"
)

func main() {
	collector := baml.NewCollector()

	result, err := b.Foo(context.Background(), 8192, b.WithCollector(collector))
	if err != nil {
		panic(err)
	}
	fmt.Println(result)

	channel, err := b.Stream.Foo(context.Background(), 8192, b.WithCollector(collector))
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

	usage, err := collector.Usage()
	if err != nil {
		panic(err)
	}
	input_tokens, err := usage.InputTokens()
	if err != nil {
		panic(err)
	}
	fmt.Println("input_tokens", input_tokens)

	output_tokens, err := usage.OutputTokens()
	if err != nil {
		panic(err)
	}
	fmt.Println("output_tokens", output_tokens)
}
