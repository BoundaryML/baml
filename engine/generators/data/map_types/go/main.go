package main

import (
	"context"
	"fmt"
	b "map_types/baml_client"
	"math"
	"os"
)

func main() {
	ctx := context.Background()

	// Test simple maps
	fmt.Println("Testing SimpleMaps...")
	simpleResult, err := b.TestSimpleMaps(ctx, "test simple maps")
	if err != nil {
		fmt.Printf("Error testing simple maps: %v\n", err)
		os.Exit(1)
	}

	// Verify simple map contents
	if len(simpleResult.StringToString) != 2 {
		fmt.Printf("Expected stringToString length 2, got %d\n", len(simpleResult.StringToString))
		os.Exit(1)
	}
	if simpleResult.StringToString["key1"] != "value1" {
		fmt.Printf("Expected stringToString['key1'] to be 'value1', got '%s'\n", simpleResult.StringToString["key1"])
		os.Exit(1)
	}

	if len(simpleResult.StringToInt) != 3 {
		fmt.Printf("Expected stringToInt length 3, got %d\n", len(simpleResult.StringToInt))
		os.Exit(1)
	}
	if simpleResult.StringToInt["one"] != 1 {
		fmt.Printf("Expected stringToInt['one'] to be 1, got %d\n", simpleResult.StringToInt["one"])
		os.Exit(1)
	}

	if len(simpleResult.StringToFloat) != 2 {
		fmt.Printf("Expected stringToFloat length 2, got %d\n", len(simpleResult.StringToFloat))
		os.Exit(1)
	}
	if math.Abs(simpleResult.StringToFloat["pi"]-3.14159) > 0.0001 {
		fmt.Printf("Expected stringToFloat['pi'] to be ~3.14159, got %f\n", simpleResult.StringToFloat["pi"])
		os.Exit(1)
	}

	if len(simpleResult.StringToBool) != 2 {
		fmt.Printf("Expected stringToBool length 2, got %d\n", len(simpleResult.StringToBool))
		os.Exit(1)
	}
	if !simpleResult.StringToBool["isTrue"] {
		fmt.Printf("Expected stringToBool['isTrue'] to be true, got false\n")
		os.Exit(1)
	}

	if len(simpleResult.IntToString) != 3 {
		fmt.Printf("Expected intToString length 3, got %d\n", len(simpleResult.IntToString))
		os.Exit(1)
	}
	if simpleResult.IntToString["1"] != "one" {
		fmt.Printf("Expected intToString['1'] to be 'one', got '%s'\n", simpleResult.IntToString["1"])
		os.Exit(1)
	}
	fmt.Println("✓ SimpleMaps test passed")

	// Test complex maps
	fmt.Println("\nTesting ComplexMaps...")
	complexResult, err := b.TestComplexMaps(ctx, "test complex maps")
	if err != nil {
		fmt.Printf("Error testing complex maps: %v\n", err)
		os.Exit(1)
	}

	// Verify complex map contents
	if len(complexResult.UserMap) < 2 {
		fmt.Printf("Expected at least 2 users in userMap, got %d\n", len(complexResult.UserMap))
		os.Exit(1)
	}
	for key, user := range complexResult.UserMap {
		if user.Name == "" {
			fmt.Printf("User '%s' has empty name\n", key)
			os.Exit(1)
		}
		if user.Email == "" {
			fmt.Printf("User '%s' has empty email\n", key)
			os.Exit(1)
		}
	}

	if len(complexResult.ProductMap) < 3 {
		fmt.Printf("Expected at least 3 products in productMap, got %d\n", len(complexResult.ProductMap))
		os.Exit(1)
	}
	for key, product := range complexResult.ProductMap {
		if product.Name == "" {
			fmt.Printf("Product %s has empty name\n", key)
			os.Exit(1)
		}
		if product.Price <= 0 {
			fmt.Printf("Product %s has invalid price: %f\n", key, product.Price)
			os.Exit(1)
		}
	}

	if len(complexResult.NestedMap) < 1 {
		fmt.Printf("Expected at least 1 entry in nestedMap, got %d\n", len(complexResult.NestedMap))
		os.Exit(1)
	}

	if len(complexResult.ArrayMap) != 2 {
		fmt.Printf("Expected arrayMap length 2, got %d\n", len(complexResult.ArrayMap))
		os.Exit(1)
	}

	if len(complexResult.MapArray) < 2 {
		fmt.Printf("Expected at least 2 maps in mapArray, got %d\n", len(complexResult.MapArray))
		os.Exit(1)
	}
	fmt.Println("✓ ComplexMaps test passed")

	// Test nested maps
	fmt.Println("\nTesting NestedMaps...")
	nestedResult, err := b.TestNestedMaps(ctx, "test nested maps")
	if err != nil {
		fmt.Printf("Error testing nested maps: %v\n", err)
		os.Exit(1)
	}

	// Verify nested map structure
	if len(nestedResult.Simple) < 2 {
		fmt.Printf("Expected at least 2 entries in simple map, got %d\n", len(nestedResult.Simple))
		os.Exit(1)
	}

	if len(nestedResult.OneLevelNested) < 2 {
		fmt.Printf("Expected at least 2 entries in oneLevelNested, got %d\n", len(nestedResult.OneLevelNested))
		os.Exit(1)
	}
	for key, innerMap := range nestedResult.OneLevelNested {
		if len(innerMap) < 2 {
			fmt.Printf("Expected at least 2 entries in oneLevelNested['%s'], got %d\n", key, len(innerMap))
			os.Exit(1)
		}
	}

	if len(nestedResult.TwoLevelNested) < 2 {
		fmt.Printf("Expected at least 2 entries in twoLevelNested, got %d\n", len(nestedResult.TwoLevelNested))
		os.Exit(1)
	}

	if len(nestedResult.MapOfArrays) < 2 {
		fmt.Printf("Expected at least 2 entries in mapOfArrays, got %d\n", len(nestedResult.MapOfArrays))
		os.Exit(1)
	}

	if len(nestedResult.MapOfMaps) < 2 {
		fmt.Printf("Expected at least 2 entries in mapOfMaps, got %d\n", len(nestedResult.MapOfMaps))
		os.Exit(1)
	}
	fmt.Println("✓ NestedMaps test passed")

	// Test edge case maps
	fmt.Println("\nTesting EdgeCaseMaps...")
	edgeResult, err := b.TestEdgeCaseMaps(ctx, "test edge case maps")
	if err != nil {
		fmt.Printf("Error testing edge case maps: %v\n", err)
		os.Exit(1)
	}

	// Verify edge case map contents
	if len(edgeResult.EmptyMap) != 0 {
		fmt.Printf("Expected emptyMap to be empty, got length %d\n", len(edgeResult.EmptyMap))
		os.Exit(1)
	}

	if len(edgeResult.NullableValues) != 2 {
		fmt.Printf("Expected nullableValues length 2, got %d\n", len(edgeResult.NullableValues))
		os.Exit(1)
	}
	if edgeResult.NullableValues["present"] == nil || *edgeResult.NullableValues["present"] != "value" {
		fmt.Printf("Expected nullableValues['present'] to be 'value', got '%v'\n", edgeResult.NullableValues["present"])
		os.Exit(1)
	}
	// Note: Go doesn't distinguish between null and empty string in maps

	if len(edgeResult.UnionValues) != 3 {
		fmt.Printf("Expected unionValues length 3, got %d\n", len(edgeResult.UnionValues))
		os.Exit(1)
	}
	fmt.Println("✓ EdgeCaseMaps test passed")

	// Test large maps
	fmt.Println("\nTesting LargeMaps...")
	largeResult, err := b.TestLargeMaps(ctx, "test large maps")
	if err != nil {
		fmt.Printf("Error testing large maps: %v\n", err)
		os.Exit(1)
	}

	// Verify large map sizes
	if len(largeResult.StringToString) < 20 {
		fmt.Printf("Expected at least 20 entries in stringToString, got %d\n", len(largeResult.StringToString))
		os.Exit(1)
	}
	if len(largeResult.StringToInt) < 20 {
		fmt.Printf("Expected at least 20 entries in stringToInt, got %d\n", len(largeResult.StringToInt))
		os.Exit(1)
	}
	if len(largeResult.StringToFloat) < 20 {
		fmt.Printf("Expected at least 20 entries in stringToFloat, got %d\n", len(largeResult.StringToFloat))
		os.Exit(1)
	}
	if len(largeResult.StringToBool) < 20 {
		fmt.Printf("Expected at least 20 entries in stringToBool, got %d\n", len(largeResult.StringToBool))
		os.Exit(1)
	}
	if len(largeResult.IntToString) < 20 {
		fmt.Printf("Expected at least 20 entries in intToString, got %d\n", len(largeResult.IntToString))
		os.Exit(1)
	}
	fmt.Println("✓ LargeMaps test passed")

	fmt.Println("\n✅ All map type tests passed!")
}
