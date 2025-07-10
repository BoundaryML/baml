package main

import (
	"context"
	b "optional_nullable/baml_client"
	"testing"
)

func TestOptionalFields(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestOptionalFields(ctx, "test optional fields")
	if err != nil {
		t.Fatalf("Error testing optional fields: %v", err)
	}

	// Verify optional field values
	if result.RequiredString != "hello" {
		t.Errorf("Expected requiredString to be 'hello', got '%s'", result.RequiredString)
	}
	if result.OptionalString == nil {
		t.Errorf("Expected optionalString to be present, got nil")
	} else if *result.OptionalString != "world" {
		t.Errorf("Expected optionalString to be 'world', got '%s'", *result.OptionalString)
	}
	if result.RequiredInt != 42 {
		t.Errorf("Expected requiredInt to be 42, got %d", result.RequiredInt)
	}
	if result.OptionalInt != nil {
		t.Errorf("Expected optionalInt to be omitted, got %v", *result.OptionalInt)
	}
	if !result.RequiredBool {
		t.Errorf("Expected requiredBool to be true, got false")
	}
	if result.OptionalBool == nil {
		t.Errorf("Expected optionalBool to be present, got nil")
	} else if *result.OptionalBool != false {
		t.Errorf("Expected optionalBool to be false, got %v", *result.OptionalBool)
	}
	if result.OptionalArray == nil {
		t.Errorf("Expected optionalArray to be present, got nil")
	} else if len(*result.OptionalArray) != 3 {
		t.Errorf("Expected optionalArray length 3, got %d", len(*result.OptionalArray))
	}
	if result.OptionalMap != nil {
		t.Errorf("Expected optionalMap to be omitted, got %v", result.OptionalMap)
	}
}

func TestNullableTypes(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestNullableTypes(ctx, "test nullable types")
	if err != nil {
		t.Fatalf("Error testing nullable types: %v", err)
	}

	// Verify nullable type values
	if result.NullableString == nil {
		t.Errorf("Expected nullableString to be present, got nil")
	} else if *result.NullableString != "present" {
		t.Errorf("Expected nullableString to be 'present', got '%s'", *result.NullableString)
	}
	if result.NullableInt != nil {
		t.Errorf("Expected nullableInt to be null, got %v", *result.NullableInt)
	}
	if result.NullableFloat == nil {
		t.Errorf("Expected nullableFloat to be present, got nil")
	} else if *result.NullableFloat != 3.14 {
		t.Errorf("Expected nullableFloat to be 3.14, got %f", *result.NullableFloat)
	}
	if result.NullableBool != nil {
		t.Errorf("Expected nullableBool to be null, got %v", *result.NullableBool)
	}
	if result.NullableArray == nil {
		t.Errorf("Expected nullableArray to be present, got nil")
	} else if len(*result.NullableArray) != 2 {
		t.Errorf("Expected nullableArray length 2, got %d", len(*result.NullableArray))
	}
	if result.NullableObject != nil {
		t.Errorf("Expected nullableObject to be null, got %v", result.NullableObject)
	}
}

func TestMixedOptionalNullable(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestMixedOptionalNullable(ctx, "test mixed optional nullable")
	if err != nil {
		t.Fatalf("Error testing mixed optional nullable: %v", err)
	}

	// Verify mixed optional nullable values
	if result.Id <= 0 {
		t.Errorf("Expected id to be positive, got %d", result.Id)
	}
	if len(result.Tags) < 0 {
		t.Errorf("Expected tags to be non-null array, got length %d", len(result.Tags))
	}
	// Check primary user is present
	if result.PrimaryUser.Id <= 0 {
		t.Errorf("Expected primaryUser.id to be positive, got %d", result.PrimaryUser.Id)
	}
	if result.PrimaryUser.Name == "" {
		t.Errorf("Expected primaryUser.name to be non-empty")
	}
}

func TestAllNull(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestAllNull(ctx, "test all null")
	if err != nil {
		t.Fatalf("Error testing all null: %v", err)
	}

	// Verify all fields are null
	if result.NullableString != nil {
		t.Errorf("Expected nullableString to be null, got '%s'", *result.NullableString)
	}
	if result.NullableInt != nil {
		t.Errorf("Expected nullableInt to be null, got %d", *result.NullableInt)
	}
	if result.NullableFloat != nil {
		t.Errorf("Expected nullableFloat to be null, got %f", *result.NullableFloat)
	}
	if result.NullableBool != nil {
		t.Errorf("Expected nullableBool to be null, got %v", *result.NullableBool)
	}
	if result.NullableArray != nil {
		t.Errorf("Expected nullableArray to be null, got %v", result.NullableArray)
	}
	if result.NullableObject != nil {
		t.Errorf("Expected nullableObject to be null, got %v", result.NullableObject)
	}
}

func TestAllOptionalOmitted(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestAllOptionalOmitted(ctx, "test all optional omitted")
	if err != nil {
		t.Fatalf("Error testing all optional omitted: %v", err)
	}

	// Verify required fields have values and optional fields are omitted
	if result.RequiredString == "" {
		t.Errorf("Expected requiredString to be non-empty")
	}
	if result.OptionalString != nil {
		t.Errorf("Expected optionalString to be omitted, got '%s'", *result.OptionalString)
	}
	if result.RequiredInt == 0 {
		t.Errorf("Expected requiredInt to be non-zero")
	}
	if result.OptionalInt != nil {
		t.Errorf("Expected optionalInt to be omitted, got %d", *result.OptionalInt)
	}
	if result.OptionalBool != nil {
		t.Errorf("Expected optionalBool to be omitted, got %v", *result.OptionalBool)
	}
	if result.OptionalArray != nil {
		t.Errorf("Expected optionalArray to be omitted, got %v", result.OptionalArray)
	}
	if result.OptionalMap != nil {
		t.Errorf("Expected optionalMap to be omitted, got %v", result.OptionalMap)
	}
}
