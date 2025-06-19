package main

import (
	b "asserts/baml_client"
	"context"
	"fmt"
)

func main() {

	result, err := b.PersonTest(context.Background())
	if err != nil {
		panic(err)
	}
	fmt.Println(result)

	channel, err := b.Stream.PersonTest(context.Background())
	if err != nil {
		panic(err)
	}
	for result := range channel {
		if result.IsFinal {
			fmt.Println("final-----")
			fmt.Println(result.Final())
		} else {
			fmt.Println("stream-----")
			fmt.Println(result.Stream())
		}
	}
}
