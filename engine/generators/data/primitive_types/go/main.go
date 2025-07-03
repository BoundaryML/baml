package main

import (
	"context"
	"fmt"
	"os"

	"primitive_types/baml_client"
)

func main() {
	ctx := context.Background()

	// Test basic primitive types
	fmt.Println("Testing PrimitiveTypes...")
	primitiveResult, err := baml_client.TestPrimitiveTypes(ctx, "test input")
	if err != nil {
		fmt.Printf("Error testing primitive types: %v\n", err)
		os.Exit(1)
	}

	// Verify primitive values
	if primitiveResult.StringField != "Hello, BAML!" {
		fmt.Printf("Expected stringField to be 'Hello, BAML!', got '%s'\n", primitiveResult.StringField)
		os.Exit(1)
	}
	if primitiveResult.IntField != 42 {
		fmt.Printf("Expected intField to be 42, got %d\n", primitiveResult.IntField)
		os.Exit(1)
	}
	if primitiveResult.FloatField < 3.14 || primitiveResult.FloatField > 3.15 {
		fmt.Printf("Expected floatField to be ~3.14159, got %f\n", primitiveResult.FloatField)
		os.Exit(1)
	}
	if !primitiveResult.BoolField {
		fmt.Printf("Expected boolField to be true, got false\n")
		os.Exit(1)
	}
	if primitiveResult.NullField != nil {
		fmt.Printf("Expected nullField to be nil, got %v\n", primitiveResult.NullField)
		os.Exit(1)
	}
	fmt.Println("✓ PrimitiveTypes test passed")

	// Test primitive arrays
	fmt.Println("\nTesting PrimitiveArrays...")
	arrayResult, err := baml_client.TestPrimitiveArrays(ctx, "test arrays")
	if err != nil {
		fmt.Printf("Error testing primitive arrays: %v\n", err)
		os.Exit(1)
	}

	// Verify array contents
	if len(arrayResult.StringArray) != 3 {
		fmt.Printf("Expected stringArray length 3, got %d\n", len(arrayResult.StringArray))
		os.Exit(1)
	}
	if len(arrayResult.IntArray) != 5 {
		fmt.Printf("Expected intArray length 5, got %d\n", len(arrayResult.IntArray))
		os.Exit(1)
	}
	if len(arrayResult.FloatArray) != 4 {
		fmt.Printf("Expected floatArray length 4, got %d\n", len(arrayResult.FloatArray))
		os.Exit(1)
	}
	if len(arrayResult.BoolArray) != 4 {
		fmt.Printf("Expected boolArray length 4, got %d\n", len(arrayResult.BoolArray))
		os.Exit(1)
	}
	fmt.Println("✓ PrimitiveArrays test passed")

	// Test primitive maps
	fmt.Println("\nTesting PrimitiveMaps...")
	mapResult, err := baml_client.TestPrimitiveMaps(ctx, "test maps")
	if err != nil {
		fmt.Printf("Error testing primitive maps: %v\n", err)
		os.Exit(1)
	}

	// Verify map contents
	if len(mapResult.StringMap) != 2 {
		fmt.Printf("Expected stringMap length 2, got %d\n", len(mapResult.StringMap))
		os.Exit(1)
	}
	if len(mapResult.IntMap) != 3 {
		fmt.Printf("Expected intMap length 3, got %d\n", len(mapResult.IntMap))
		os.Exit(1)
	}
	if len(mapResult.FloatMap) != 2 {
		fmt.Printf("Expected floatMap length 2, got %d\n", len(mapResult.FloatMap))
		os.Exit(1)
	}
	if len(mapResult.BoolMap) != 2 {
		fmt.Printf("Expected boolMap length 2, got %d\n", len(mapResult.BoolMap))
		os.Exit(1)
	}
	fmt.Println("✓ PrimitiveMaps test passed")

	// Test mixed primitives
	fmt.Println("\nTesting MixedPrimitives...")
	mixedResult, err := baml_client.TestMixedPrimitives(ctx, "test mixed")
	if err != nil {
		fmt.Printf("Error testing mixed primitives: %v\n", err)
		os.Exit(1)
	}

	// Basic validation for mixed types
	if mixedResult.Name == "" {
		fmt.Printf("Expected name to be non-empty\n")
		os.Exit(1)
	}
	if mixedResult.Age <= 0 {
		fmt.Printf("Expected age to be positive, got %d\n", mixedResult.Age)
		os.Exit(1)
	}
	fmt.Println("✓ MixedPrimitives test passed")

	// Test empty collections
	fmt.Println("\nTesting EmptyCollections...")
	emptyResult, err := baml_client.TestEmptyCollections(ctx, "test empty")
	if err != nil {
		fmt.Printf("Error testing empty collections: %v\n", err)
		os.Exit(1)
	}

	// Verify empty arrays
	if len(emptyResult.StringArray) != 0 {
		fmt.Printf("Expected empty stringArray, got length %d\n", len(emptyResult.StringArray))
		os.Exit(1)
	}
	if len(emptyResult.IntArray) != 0 {
		fmt.Printf("Expected empty intArray, got length %d\n", len(emptyResult.IntArray))
		os.Exit(1)
	}
	if len(emptyResult.FloatArray) != 0 {
		fmt.Printf("Expected empty floatArray, got length %d\n", len(emptyResult.FloatArray))
		os.Exit(1)
	}
	if len(emptyResult.BoolArray) != 0 {
		fmt.Printf("Expected empty boolArray, got length %d\n", len(emptyResult.BoolArray))
		os.Exit(1)
	}
	fmt.Println("✓ EmptyCollections test passed")

	fmt.Println("\n✅ All primitive type tests passed!")
}
