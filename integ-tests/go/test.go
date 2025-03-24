package main

import (
	"context"
	"fmt"

	b "example.com/integ-tests/baml_client"
)

func main() {
	ctx := context.Background()
	for v := range b.Stream.TestOllama(ctx) {
		fmt.Println(v)
	}
}
