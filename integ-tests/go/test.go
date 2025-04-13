package main

import (
	"context"
	"fmt"

	b "example.com/integ-tests/baml_client"
	"example.com/integ-tests/baml_client/types"
)

func main() {
	ctx := context.Background()

	param := types.Union__List__string__string{}
	param.SetString("oranges")
	v2, err := b.AaaSamOutputFormat(ctx, &param)
	if err != nil {
		panic(err)
	}
	fmt.Println(*v2)

	v2, err = b.AaaSamOutputFormat(ctx, nil)
	if err != nil {
		panic(err)
	}
	fmt.Println(*v2)

	stream := b.Stream.AaaSamOutputFormat(ctx, &param)
	for chunk := range stream {
		fmt.Println(chunk)
	}
}
