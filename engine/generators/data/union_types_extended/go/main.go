package main

import (
	"context"
	"fmt"
	"os"
	b "union_types_extended/baml_client"
)

func main() {
	ctx := context.Background()

	// Test primitive unions
	fmt.Println("Testing PrimitiveUnions...")
	primitiveResult, err := b.TestPrimitiveUnions(ctx, "test primitive unions")
	if err != nil {
		fmt.Printf("Error testing primitive unions: %v\n", err)
		os.Exit(1)
	}

	// Verify primitive union values
	// Note: Union type validation will depend on the actual generated Go types
	// The exact type checking will depend on how the Go client handles unions
	if !primitiveResult.StringOrInt.IsString() && !primitiveResult.StringOrInt.IsInt() {
		fmt.Printf("Expected stringOrInt to have a value\n")
		os.Exit(1)
	}
	if !primitiveResult.StringOrFloat.IsString() && !primitiveResult.StringOrFloat.IsFloat() {
		fmt.Printf("Expected stringOrFloat to have a value\n")
		os.Exit(1)
	}
	if !primitiveResult.IntOrFloat.IsInt() && !primitiveResult.IntOrFloat.IsFloat() {
		fmt.Printf("Expected intOrFloat to have a value\n")
		os.Exit(1)
	}
	if !primitiveResult.BoolOrString.IsBool() && !primitiveResult.BoolOrString.IsString() {
		fmt.Printf("Expected boolOrString to have a value\n")
		os.Exit(1)
	}
	if !primitiveResult.AnyPrimitive.IsString() && !primitiveResult.AnyPrimitive.IsInt() && !primitiveResult.AnyPrimitive.IsFloat() && !primitiveResult.AnyPrimitive.IsBool() {
		fmt.Printf("Expected anyPrimitive to have a value\n")
		os.Exit(1)
	}
	fmt.Println("✓ PrimitiveUnions test passed")

	// Test complex unions
	fmt.Println("\nTesting ComplexUnions...")
	complexResult, err := b.TestComplexUnions(ctx, "test complex unions")
	if err != nil {
		fmt.Printf("Error testing complex unions: %v\n", err)
		os.Exit(1)
	}

	// Verify complex union values
	if !complexResult.UserOrProduct.IsUser() && !complexResult.UserOrProduct.IsProduct() {
		fmt.Printf("Expected userOrProduct to have a value\n")
		os.Exit(1)
	}
	if !complexResult.UserOrProductOrAdmin.IsUser() && !complexResult.UserOrProductOrAdmin.IsProduct() && !complexResult.UserOrProductOrAdmin.IsAdmin() {
		fmt.Printf("Expected userOrProductOrAdmin to have a value\n")
		os.Exit(1)
	}
	if !complexResult.DataOrError.IsDataResponse() && !complexResult.DataOrError.IsErrorResponse() {
		fmt.Printf("Expected dataOrError to have a value\n")
		os.Exit(1)
	}
	if !complexResult.MultiTypeResult.IsSuccess() && !complexResult.MultiTypeResult.IsWarning() && !complexResult.MultiTypeResult.IsError() {
		fmt.Printf("Expected multiTypeResult to have a value\n")
		os.Exit(1)
	}
	fmt.Println("✓ ComplexUnions test passed")

	// Test discriminated unions
	fmt.Println("\nTesting DiscriminatedUnions...")
	discResult, err := b.TestDiscriminatedUnions(ctx, "test discriminated unions")
	if err != nil {
		fmt.Printf("Error testing discriminated unions: %v\n", err)
		os.Exit(1)
	}

	// Verify discriminated union values
	if !discResult.Shape.IsCircle() && !discResult.Shape.IsRectangle() && !discResult.Shape.IsTriangle() {
		fmt.Printf("Expected shape to have a value\n")
		os.Exit(1)
	}
	// Check if shape is a circle with the expected discriminator
	if discResult.Shape.IsCircle() {
		circle := discResult.Shape.Circle()
		if circle.Shape != "circle" {
			fmt.Printf("Expected shape.shape to be 'circle', got '%s'\n", circle.Shape)
			os.Exit(1)
		}
		if circle.Radius != 5.0 {
			fmt.Printf("Expected circle.radius to be 5.0, got %f\n", circle.Radius)
			os.Exit(1)
		}
	} else {
		fmt.Printf("Expected shape to be a Circle\n")
		os.Exit(1)
	}

	if !discResult.Animal.IsDog() && !discResult.Animal.IsCat() && !discResult.Animal.IsBird() {
		fmt.Printf("Expected animal to have a value\n")
		os.Exit(1)
	}
	// Check if animal is a dog
	if discResult.Animal.IsDog() {
		dog := discResult.Animal.Dog()
		if dog.Species != "dog" {
			fmt.Printf("Expected animal.species to be 'dog', got '%s'\n", dog.Species)
			os.Exit(1)
		}
		if dog.Breed == "" {
			fmt.Printf("Expected dog.breed to be non-empty\n")
			os.Exit(1)
		}
		if !dog.GoodBoy {
			fmt.Printf("Expected dog.goodBoy to be true\n")
			os.Exit(1)
		}
	} else {
		fmt.Printf("Expected animal to be a Dog\n")
		os.Exit(1)
	}

	if !discResult.Response.IsApiSuccess() && !discResult.Response.IsApiError() && !discResult.Response.IsApiPending() {
		fmt.Printf("Expected response to have a value\n")
		os.Exit(1)
	}
	// Check if response is an error
	if discResult.Response.IsApiError() {
		apiError := discResult.Response.ApiError()
		if apiError.Status != "error" {
			fmt.Printf("Expected response.status to be 'error', got '%s'\n", apiError.Status)
			os.Exit(1)
		}
		if apiError.Message != "Not found" {
			fmt.Printf("Expected error.message to be 'Not found', got '%s'\n", apiError.Message)
			os.Exit(1)
		}
		if apiError.Code != 404 {
			fmt.Printf("Expected error.code to be 404, got %d\n", apiError.Code)
			os.Exit(1)
		}
	} else {
		fmt.Printf("Expected response to be an ApiError\n")
		os.Exit(1)
	}
	fmt.Println("✓ DiscriminatedUnions test passed")

	// Test union arrays
	fmt.Println("\nTesting UnionArrays...")
	unionResult, err := b.TestUnionArrays(ctx, "test union arrays")
	if err != nil {
		fmt.Printf("Error testing union arrays: %v\n", err)
		os.Exit(1)
	}

	// Verify union array contents
	if len(unionResult.MixedArray) != 4 {
		fmt.Printf("Expected mixedArray length 4, got %d\n", len(unionResult.MixedArray))
		os.Exit(1)
	}

	if len(unionResult.NullableItems) != 4 {
		fmt.Printf("Expected nullableItems length 4, got %d\n", len(unionResult.NullableItems))
		os.Exit(1)
	}

	if len(unionResult.ObjectArray) < 2 {
		fmt.Printf("Expected at least 2 objects in objectArray, got %d\n", len(unionResult.ObjectArray))
		os.Exit(1)
	}

	if len(unionResult.NestedUnionArray) != 4 {
		fmt.Printf("Expected nestedUnionArray length 4, got %d\n", len(unionResult.NestedUnionArray))
		os.Exit(1)
	}

	fmt.Println("✓ UnionArrays test passed")

	fmt.Println("\n✅ All union type tests passed!")
}
