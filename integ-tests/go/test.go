package main

import (
	"context"
	"fmt"

	b "example.com/integ-tests/baml_client"
)

func main() {
	ctx := context.Background()

	v2, err := b.AaaSamOutputFormat(ctx, "oranges")
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
