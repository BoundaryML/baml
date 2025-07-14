package main

import (
	"context"
	"testing"
	b "union_types_extended/baml_client"
)

func TestPrimitiveUnions(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestPrimitiveUnions(ctx, "test primitive unions")
	if err != nil {
		t.Fatalf("Error testing primitive unions: %v", err)
	}

	// Verify primitive union values
	if !result.StringOrInt.IsString() && !result.StringOrInt.IsInt() {
		t.Errorf("Expected stringOrInt to have a value")
	}
	if !result.StringOrFloat.IsString() && !result.StringOrFloat.IsFloat() {
		t.Errorf("Expected stringOrFloat to have a value")
	}
	if !result.IntOrFloat.IsInt() && !result.IntOrFloat.IsFloat() {
		t.Errorf("Expected intOrFloat to have a value")
	}
	if !result.BoolOrString.IsBool() && !result.BoolOrString.IsString() {
		t.Errorf("Expected boolOrString to have a value")
	}
	if !result.AnyPrimitive.IsString() && !result.AnyPrimitive.IsInt() && !result.AnyPrimitive.IsFloat() && !result.AnyPrimitive.IsBool() {
		t.Errorf("Expected anyPrimitive to have a value")
	}
}

func TestComplexUnions(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestComplexUnions(ctx, "test complex unions")
	if err != nil {
		t.Fatalf("Error testing complex unions: %v", err)
	}

	// Verify complex union values
	if !result.UserOrProduct.IsUser() && !result.UserOrProduct.IsProduct() {
		t.Errorf("Expected userOrProduct to have a value")
	}
	if !result.UserOrProductOrAdmin.IsUser() && !result.UserOrProductOrAdmin.IsProduct() && !result.UserOrProductOrAdmin.IsAdmin() {
		t.Errorf("Expected userOrProductOrAdmin to have a value")
	}
	if !result.DataOrError.IsDataResponse() && !result.DataOrError.IsErrorResponse() {
		t.Errorf("Expected dataOrError to have a value")
	}
	if !result.MultiTypeResult.IsSuccess() && !result.MultiTypeResult.IsWarning() && !result.MultiTypeResult.IsError() {
		t.Errorf("Expected multiTypeResult to have a value")
	}
}

func TestDiscriminatedUnions(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestDiscriminatedUnions(ctx, "test discriminated unions")
	if err != nil {
		t.Fatalf("Error testing discriminated unions: %v", err)
	}

	// Verify discriminated union values
	if !result.Shape.IsCircle() && !result.Shape.IsRectangle() && !result.Shape.IsTriangle() {
		t.Errorf("Expected shape to have a value")
	}
	// Check if shape is a circle with the expected discriminator
	if result.Shape.IsCircle() {
		circle := result.Shape.Circle()
		if circle == nil {
			t.Errorf("Expected non-nil circle")
			return
		}
		if circle.Shape != "circle" {
			t.Errorf("Expected shape.shape to be 'circle', got '%s'", circle.Shape)
		}
		if circle.Radius != 5.0 {
			t.Errorf("Expected circle.radius to be 5.0, got %f", circle.Radius)
		}
	} else {
		t.Errorf("Expected shape to be a Circle")
	}

	if !result.Animal.IsDog() && !result.Animal.IsCat() && !result.Animal.IsBird() {
		t.Errorf("Expected animal to have a value")
	}
	// Check if animal is a dog
	if result.Animal.IsDog() {
		dog := result.Animal.Dog()
		if dog == nil {
			t.Errorf("Expected non-nil dog")
			return
		}
		if dog.Species != "dog" {
			t.Errorf("Expected animal.species to be 'dog', got '%s'", dog.Species)
		}
		if dog.Breed == "" {
			t.Errorf("Expected dog.breed to be non-empty")
		}
		if !dog.GoodBoy {
			t.Errorf("Expected dog.goodBoy to be true")
		}
	} else {
		t.Errorf("Expected animal to be a Dog")
	}

	if !result.Response.IsApiSuccess() && !result.Response.IsApiError() && !result.Response.IsApiPending() {
		t.Errorf("Expected response to have a value")
	}
	// Check if response is an error
	if result.Response.IsApiError() {
		apiError := result.Response.ApiError()
		if apiError == nil {
			t.Errorf("Expected non-nil apiError")
			return
		}
		if apiError.Status != "error" {
			t.Errorf("Expected response.status to be 'error', got '%s'", apiError.Status)
		}
		if apiError.Message != "Not found" {
			t.Errorf("Expected error.message to be 'Not found', got '%s'", apiError.Message)
		}
		if apiError.Code != 404 {
			t.Errorf("Expected error.code to be 404, got %d", apiError.Code)
		}
	} else {
		t.Errorf("Expected response to be an ApiError")
	}
}

func TestUnionArrays(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestUnionArrays(ctx, "test union arrays")
	if err != nil {
		t.Fatalf("Error testing union arrays: %v", err)
	}

	// Verify union array contents
	if len(result.MixedArray) != 4 {
		t.Errorf("Expected mixedArray length 4, got %d", len(result.MixedArray))
	}

	if len(result.NullableItems) != 4 {
		t.Errorf("Expected nullableItems length 4, got %d", len(result.NullableItems))
	}

	if len(result.ObjectArray) < 2 {
		t.Errorf("Expected at least 2 objects in objectArray, got %d", len(result.ObjectArray))
	}

	if len(result.NestedUnionArray) != 4 {
		t.Errorf("Expected nestedUnionArray length 4, got %d", len(result.NestedUnionArray))
	}
}
