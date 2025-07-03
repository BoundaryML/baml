package main

import (
	"context"
	"fmt"
	b "optional_nullable/baml_client"
	"os"
)

func main() {
	ctx := context.Background()

	// Test optional fields
	fmt.Println("Testing OptionalFields...")
	optionalResult, err := b.TestOptionalFields(ctx, "test optional fields")
	if err != nil {
		fmt.Printf("Error testing optional fields: %v\n", err)
		os.Exit(1)
	}

	// Verify optional field values
	if optionalResult.RequiredString != "hello" {
		fmt.Printf("Expected requiredString to be 'hello', got '%s'\n", optionalResult.RequiredString)
		os.Exit(1)
	}
	if optionalResult.OptionalString == nil {
		fmt.Printf("Expected optionalString to be present, got nil\n")
		os.Exit(1)
	} else if *optionalResult.OptionalString != "world" {
		fmt.Printf("Expected optionalString to be 'world', got '%s'\n", *optionalResult.OptionalString)
		os.Exit(1)
	}
	if optionalResult.RequiredInt != 42 {
		fmt.Printf("Expected requiredInt to be 42, got %d\n", optionalResult.RequiredInt)
		os.Exit(1)
	}
	if optionalResult.OptionalInt != nil {
		fmt.Printf("Expected optionalInt to be omitted, got %v\n", *optionalResult.OptionalInt)
		os.Exit(1)
	}
	if !optionalResult.RequiredBool {
		fmt.Printf("Expected requiredBool to be true, got false\n")
		os.Exit(1)
	}
	if optionalResult.OptionalBool == nil {
		fmt.Printf("Expected optionalBool to be present, got nil\n")
		os.Exit(1)
	} else if *optionalResult.OptionalBool != false {
		fmt.Printf("Expected optionalBool to be false, got %v\n", *optionalResult.OptionalBool)
		os.Exit(1)
	}
	if optionalResult.OptionalArray == nil {
		fmt.Printf("Expected optionalArray to be present, got nil\n")
		os.Exit(1)
	} else if len(*optionalResult.OptionalArray) != 3 {
		fmt.Printf("Expected optionalArray length 3, got %d\n", len(*optionalResult.OptionalArray))
		os.Exit(1)
	}
	if optionalResult.OptionalMap != nil {
		fmt.Printf("Expected optionalMap to be omitted, got %v\n", optionalResult.OptionalMap)
		os.Exit(1)
	}
	fmt.Println("✓ OptionalFields test passed")

	// Test nullable types
	fmt.Println("\nTesting NullableTypes...")
	nullableResult, err := b.TestNullableTypes(ctx, "test nullable types")
	if err != nil {
		fmt.Printf("Error testing nullable types: %v\n", err)
		os.Exit(1)
	}

	// Verify nullable type values
	if nullableResult.NullableString == nil {
		fmt.Printf("Expected nullableString to be present, got nil\n")
		os.Exit(1)
	} else if *nullableResult.NullableString != "present" {
		fmt.Printf("Expected nullableString to be 'present', got '%s'\n", *nullableResult.NullableString)
		os.Exit(1)
	}
	if nullableResult.NullableInt != nil {
		fmt.Printf("Expected nullableInt to be null, got %v\n", *nullableResult.NullableInt)
		os.Exit(1)
	}
	if nullableResult.NullableFloat == nil {
		fmt.Printf("Expected nullableFloat to be present, got nil\n")
		os.Exit(1)
	} else if *nullableResult.NullableFloat != 3.14 {
		fmt.Printf("Expected nullableFloat to be 3.14, got %f\n", *nullableResult.NullableFloat)
		os.Exit(1)
	}
	if nullableResult.NullableBool != nil {
		fmt.Printf("Expected nullableBool to be null, got %v\n", *nullableResult.NullableBool)
		os.Exit(1)
	}
	if nullableResult.NullableArray == nil {
		fmt.Printf("Expected nullableArray to be present, got nil\n")
		os.Exit(1)
	} else if len(*nullableResult.NullableArray) != 2 {
		fmt.Printf("Expected nullableArray length 2, got %d\n", len(*nullableResult.NullableArray))
		os.Exit(1)
	}
	if nullableResult.NullableObject != nil {
		fmt.Printf("Expected nullableObject to be null, got %v\n", nullableResult.NullableObject)
		os.Exit(1)
	}
	fmt.Println("✓ NullableTypes test passed")

	// Test mixed optional nullable
	fmt.Println("\nTesting MixedOptionalNullable...")
	mixedResult, err := b.TestMixedOptionalNullable(ctx, "test mixed optional nullable")
	if err != nil {
		fmt.Printf("Error testing mixed optional nullable: %v\n", err)
		os.Exit(1)
	}

	// Verify mixed optional nullable values
	if mixedResult.Id <= 0 {
		fmt.Printf("Expected id to be positive, got %d\n", mixedResult.Id)
		os.Exit(1)
	}
	if len(mixedResult.Tags) < 0 {
		fmt.Printf("Expected tags to be non-null array, got length %d\n", len(mixedResult.Tags))
		os.Exit(1)
	}
	// Check primary user is present
	if mixedResult.PrimaryUser.Id <= 0 {
		fmt.Printf("Expected primaryUser.id to be positive, got %d\n", mixedResult.PrimaryUser.Id)
		os.Exit(1)
	}
	if mixedResult.PrimaryUser.Name == "" {
		fmt.Printf("Expected primaryUser.name to be non-empty\n")
		os.Exit(1)
	}
	fmt.Println("✓ MixedOptionalNullable test passed")

	// Test all null
	fmt.Println("\nTesting AllNull...")
	allNullResult, err := b.TestAllNull(ctx, "test all null")
	if err != nil {
		fmt.Printf("Error testing all null: %v\n", err)
		os.Exit(1)
	}

	// Verify all fields are null
	if allNullResult.NullableString != nil {
		fmt.Printf("Expected nullableString to be null, got '%s'\n", *allNullResult.NullableString)
		os.Exit(1)
	}
	if allNullResult.NullableInt != nil {
		fmt.Printf("Expected nullableInt to be null, got %d\n", *allNullResult.NullableInt)
		os.Exit(1)
	}
	if allNullResult.NullableFloat != nil {
		fmt.Printf("Expected nullableFloat to be null, got %f\n", *allNullResult.NullableFloat)
		os.Exit(1)
	}
	if allNullResult.NullableBool != nil {
		fmt.Printf("Expected nullableBool to be null, got %v\n", *allNullResult.NullableBool)
		os.Exit(1)
	}
	if allNullResult.NullableArray != nil {
		fmt.Printf("Expected nullableArray to be null, got %v\n", allNullResult.NullableArray)
		os.Exit(1)
	}
	if allNullResult.NullableObject != nil {
		fmt.Printf("Expected nullableObject to be null, got %v\n", allNullResult.NullableObject)
		os.Exit(1)
	}
	fmt.Println("✓ AllNull test passed")

	// Test all optional omitted
	fmt.Println("\nTesting AllOptionalOmitted...")
	allOptionalResult, err := b.TestAllOptionalOmitted(ctx, "test all optional omitted")
	if err != nil {
		fmt.Printf("Error testing all optional omitted: %v\n", err)
		os.Exit(1)
	}

	// Verify required fields have values and optional fields are omitted
	if allOptionalResult.RequiredString == "" {
		fmt.Printf("Expected requiredString to be non-empty\n")
		os.Exit(1)
	}
	if allOptionalResult.OptionalString != nil {
		fmt.Printf("Expected optionalString to be omitted, got '%s'\n", *allOptionalResult.OptionalString)
		os.Exit(1)
	}
	if allOptionalResult.RequiredInt == 0 {
		fmt.Printf("Expected requiredInt to be non-zero\n")
		os.Exit(1)
	}
	if allOptionalResult.OptionalInt != nil {
		fmt.Printf("Expected optionalInt to be omitted, got %d\n", *allOptionalResult.OptionalInt)
		os.Exit(1)
	}
	if allOptionalResult.OptionalBool != nil {
		fmt.Printf("Expected optionalBool to be omitted, got %v\n", *allOptionalResult.OptionalBool)
		os.Exit(1)
	}
	if allOptionalResult.OptionalArray != nil {
		fmt.Printf("Expected optionalArray to be omitted, got %v\n", allOptionalResult.OptionalArray)
		os.Exit(1)
	}
	if allOptionalResult.OptionalMap != nil {
		fmt.Printf("Expected optionalMap to be omitted, got %v\n", allOptionalResult.OptionalMap)
		os.Exit(1)
	}
	fmt.Println("✓ AllOptionalOmitted test passed")

	fmt.Println("\n✅ All optional/nullable type tests passed!")
}
