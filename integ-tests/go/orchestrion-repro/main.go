package main

import (
	"context"
	"fmt"
	"log"

	b "example.com/integ-tests/baml_client"
)

func main() {
	log.Println("Starting BAML Orchestrion reproduction test...")

	ctx := context.Background()

	log.Println("Calling BAML function...")
	result, err := b.AaaSamOutputFormat(ctx, "oranges")
	if err != nil {
		log.Fatalf("BAML function call failed: %v", err)
	}

	fmt.Printf("Success! Result: %v\n", result)
	log.Println("Test completed successfully!")
}
