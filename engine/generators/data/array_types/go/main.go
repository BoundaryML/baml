package main

import (
	b "array_types/baml_client"
	"context"
	"fmt"
	"os"
)

func main() {
	ctx := context.Background()

	// Test simple arrays
	fmt.Println("Testing SimpleArrays...")
	simpleResult, err := b.TestSimpleArrays(ctx, "test simple arrays")
	if err != nil {
		fmt.Printf("Error testing simple arrays: %v\n", err)
		os.Exit(1)
	}

	// Verify simple array contents
	if len(simpleResult.Strings) != 3 {
		fmt.Printf("Expected strings length 3, got %d\n", len(simpleResult.Strings))
		os.Exit(1)
	}
	if len(simpleResult.Integers) != 5 {
		fmt.Printf("Expected integers length 5, got %d\n", len(simpleResult.Integers))
		os.Exit(1)
	}
	if len(simpleResult.Floats) != 3 {
		fmt.Printf("Expected floats length 3, got %d\n", len(simpleResult.Floats))
		os.Exit(1)
	}
	if len(simpleResult.Booleans) != 4 {
		fmt.Printf("Expected booleans length 4, got %d\n", len(simpleResult.Booleans))
		os.Exit(1)
	}
	fmt.Println("✓ SimpleArrays test passed")

	// Test nested arrays
	fmt.Println("\nTesting NestedArrays...")
	nestedResult, err := b.TestNestedArrays(ctx, "test nested arrays")
	if err != nil {
		fmt.Printf("Error testing nested arrays: %v\n", err)
		os.Exit(1)
	}

	// Verify nested array structure
	if len(nestedResult.Matrix) != 3 {
		fmt.Printf("Expected matrix length 3, got %d\n", len(nestedResult.Matrix))
		os.Exit(1)
	}
	if len(nestedResult.Matrix[0]) != 3 {
		fmt.Printf("Expected matrix[0] length 3, got %d\n", len(nestedResult.Matrix[0]))
		os.Exit(1)
	}
	if len(nestedResult.StringMatrix) != 2 {
		fmt.Printf("Expected stringMatrix length 2, got %d\n", len(nestedResult.StringMatrix))
		os.Exit(1)
	}
	if len(nestedResult.ThreeDimensional) != 2 {
		fmt.Printf("Expected threeDimensional length 2, got %d\n", len(nestedResult.ThreeDimensional))
		os.Exit(1)
	}
	fmt.Println("✓ NestedArrays test passed")

	// Test object arrays
	fmt.Println("\nTesting ObjectArrays...")
	objectResult, err := b.TestObjectArrays(ctx, "test object arrays")
	if err != nil {
		fmt.Printf("Error testing object arrays: %v\n", err)
		os.Exit(1)
	}

	// Verify object array contents
	if len(objectResult.Users) < 3 {
		fmt.Printf("Expected at least 3 users, got %d\n", len(objectResult.Users))
		os.Exit(1)
	}
	if len(objectResult.Products) < 2 {
		fmt.Printf("Expected at least 2 products, got %d\n", len(objectResult.Products))
		os.Exit(1)
	}
	if len(objectResult.Tags) < 4 {
		fmt.Printf("Expected at least 4 tags, got %d\n", len(objectResult.Tags))
		os.Exit(1)
	}

	// Verify user objects have required fields
	for i, user := range objectResult.Users {
		if user.Id <= 0 {
			fmt.Printf("User %d has invalid id: %d\n", i, user.Id)
			os.Exit(1)
		}
		if user.Name == "" {
			fmt.Printf("User %d has empty name\n", i)
			os.Exit(1)
		}
		if user.Email == "" {
			fmt.Printf("User %d has empty email\n", i)
			os.Exit(1)
		}
	}
	fmt.Println("✓ ObjectArrays test passed")

	// Test mixed arrays
	fmt.Println("\nTesting MixedArrays...")
	mixedResult, err := b.TestMixedArrays(ctx, "test mixed arrays")
	if err != nil {
		fmt.Printf("Error testing mixed arrays: %v\n", err)
		os.Exit(1)
	}

	// Verify mixed array contents
	if len(mixedResult.PrimitiveArray) != 4 {
		fmt.Printf("Expected primitiveArray length 4, got %d\n", len(mixedResult.PrimitiveArray))
		os.Exit(1)
	}
	if len(mixedResult.NullableArray) != 4 {
		fmt.Printf("Expected nullableArray length 4, got %d\n", len(mixedResult.NullableArray))
		os.Exit(1)
	}
	if len(mixedResult.OptionalItems) < 2 {
		fmt.Printf("Expected at least 2 optionalItems, got %d\n", len(mixedResult.OptionalItems))
		os.Exit(1)
	}
	if len(mixedResult.ArrayOfArrays) < 2 {
		fmt.Printf("Expected at least 2 arrayOfArrays, got %d\n", len(mixedResult.ArrayOfArrays))
		os.Exit(1)
	}
	if len(mixedResult.ComplexMixed) < 2 {
		fmt.Printf("Expected at least 2 complexMixed items, got %d\n", len(mixedResult.ComplexMixed))
		os.Exit(1)
	}
	fmt.Println("✓ MixedArrays test passed")

	// Test empty arrays
	fmt.Println("\nTesting EmptyArrays...")
	emptyResult, err := b.TestEmptyArrays(ctx, "test empty arrays")
	if err != nil {
		fmt.Printf("Error testing empty arrays: %v\n", err)
		os.Exit(1)
	}

	// Verify all arrays are empty
	if len(emptyResult.Strings) != 0 {
		fmt.Printf("Expected empty strings array, got length %d\n", len(emptyResult.Strings))
		os.Exit(1)
	}
	if len(emptyResult.Integers) != 0 {
		fmt.Printf("Expected empty integers array, got length %d\n", len(emptyResult.Integers))
		os.Exit(1)
	}
	if len(emptyResult.Floats) != 0 {
		fmt.Printf("Expected empty floats array, got length %d\n", len(emptyResult.Floats))
		os.Exit(1)
	}
	if len(emptyResult.Booleans) != 0 {
		fmt.Printf("Expected empty booleans array, got length %d\n", len(emptyResult.Booleans))
		os.Exit(1)
	}
	fmt.Println("✓ EmptyArrays test passed")

	// Test large arrays
	fmt.Println("\nTesting LargeArrays...")
	largeResult, err := b.TestLargeArrays(ctx, "test large arrays")
	if err != nil {
		fmt.Printf("Error testing large arrays: %v\n", err)
		os.Exit(1)
	}

	// Verify large array sizes
	if len(largeResult.Strings) < 40 {
		fmt.Printf("Expected at least 40 strings, got %d\n", len(largeResult.Strings))
		os.Exit(1)
	}
	if len(largeResult.Integers) < 50 {
		fmt.Printf("Expected at least 50 integers, got %d\n", len(largeResult.Integers))
		os.Exit(1)
	}
	if len(largeResult.Floats) < 20 {
		fmt.Printf("Expected at least 20 floats, got %d\n", len(largeResult.Floats))
		os.Exit(1)
	}
	if len(largeResult.Booleans) < 15 {
		fmt.Printf("Expected at least 15 booleans, got %d\n", len(largeResult.Booleans))
		os.Exit(1)
	}
	fmt.Println("✓ LargeArrays test passed")

	fmt.Println("\n✅ All array type tests passed!")
}
