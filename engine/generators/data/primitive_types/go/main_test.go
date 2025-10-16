package main

import (
	"context"
	"primitive_types/baml_client"
	"testing"
)

// Test top-level primitive types
func TestTopLevelString(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := baml_client.TestTopLevelString(ctx, "test string")
	if err != nil {
		t.Fatalf("Error testing top-level string: %v", err)
	}
	if result != "Hello from BAML!" {
		t.Errorf("Expected 'Hello from BAML!', got '%s'", result)
	}
}

func TestTopLevelInt(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := baml_client.TestTopLevelInt(ctx, "test int")
	if err != nil {
		t.Fatalf("Error testing top-level int: %v", err)
	}
	if result != 42 {
		t.Errorf("Expected 42, got %d", result)
	}
}

func TestTopLevelFloat(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := baml_client.TestTopLevelFloat(ctx, "test float")
	if err != nil {
		t.Fatalf("Error testing top-level float: %v", err)
	}
	if result < 3.14 || result > 3.15 {
		t.Errorf("Expected ~3.14159, got %f", result)
	}
}

func TestTopLevelBool(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := baml_client.TestTopLevelBool(ctx, "test bool")
	if err != nil {
		t.Fatalf("Error testing top-level bool: %v", err)
	}
	if !result {
		t.Errorf("Expected true, got false")
	}
}

// TODO(vbv): Top level null is not supported yet.
// func TestTopLevelNull(t *testing.T) {
// 	t.Parallel()
// 	ctx := context.Background()

// 	result, err := baml_client.TestTopLevelNull(ctx, "test null")
// 	if err != nil {
// 		t.Fatalf("Error testing top-level null: %v", err)
// 	}
// 	if result != nil {
// 		t.Errorf("Expected nil, got %v", result)
// 	}
// }

func TestPrimitiveTypes(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := baml_client.TestPrimitiveTypes(ctx, "test input")
	if err != nil {
		t.Fatalf("Error testing primitive types: %v", err)
	}

	// Verify primitive values
	if result.StringField != "Hello, BAML!" {
		t.Errorf("Expected stringField to be 'Hello, BAML!', got '%s'", result.StringField)
	}
	if result.IntField != 42 {
		t.Errorf("Expected intField to be 42, got %d", result.IntField)
	}
	if result.FloatField < 3.14 || result.FloatField > 3.15 {
		t.Errorf("Expected floatField to be ~3.14159, got %f", result.FloatField)
	}
	if !result.BoolField {
		t.Errorf("Expected boolField to be true, got false")
	}
	if result.NullField != nil {
		t.Errorf("Expected nullField to be nil, got %v", result.NullField)
	}
}

func TestPrimitiveArrays(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := baml_client.TestPrimitiveArrays(ctx, "test arrays")
	if err != nil {
		t.Fatalf("Error testing primitive arrays: %v", err)
	}

	// Verify array contents
	if len(result.StringArray) != 3 {
		t.Errorf("Expected stringArray length 3, got %d", len(result.StringArray))
	}
	if len(result.IntArray) != 5 {
		t.Errorf("Expected intArray length 5, got %d", len(result.IntArray))
	}
	if len(result.FloatArray) != 4 {
		t.Errorf("Expected floatArray length 4, got %d", len(result.FloatArray))
	}
	if len(result.BoolArray) != 4 {
		t.Errorf("Expected boolArray length 4, got %d", len(result.BoolArray))
	}
}

func TestPrimitiveMaps(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := baml_client.TestPrimitiveMaps(ctx, "test maps")
	if err != nil {
		t.Fatalf("Error testing primitive maps: %v", err)
	}

	// Verify map contents
	if len(result.StringMap) != 2 {
		t.Errorf("Expected stringMap length 2, got %d", len(result.StringMap))
	}
	if len(result.IntMap) != 3 {
		t.Errorf("Expected intMap length 3, got %d", len(result.IntMap))
	}
	if len(result.FloatMap) != 2 {
		t.Errorf("Expected floatMap length 2, got %d", len(result.FloatMap))
	}
	if len(result.BoolMap) != 2 {
		t.Errorf("Expected boolMap length 2, got %d", len(result.BoolMap))
	}
}

func TestMixedPrimitives(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := baml_client.TestMixedPrimitives(ctx, "test mixed")
	if err != nil {
		t.Fatalf("Error testing mixed primitives: %v", err)
	}

	// Basic validation for mixed types
	if result.Name == "" {
		t.Errorf("Expected name to be non-empty")
	}
	if result.Age <= 0 {
		t.Errorf("Expected age to be positive, got %d", result.Age)
	}
}

func TestEmptyCollections(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := baml_client.TestEmptyCollections(ctx, "test empty")
	if err != nil {
		t.Fatalf("Error testing empty collections: %v", err)
	}

	// Verify empty arrays
	if len(result.StringArray) != 0 {
		t.Errorf("Expected empty stringArray, got length %d", len(result.StringArray))
	}
	if len(result.IntArray) != 0 {
		t.Errorf("Expected empty intArray, got length %d", len(result.IntArray))
	}
	if len(result.FloatArray) != 0 {
		t.Errorf("Expected empty floatArray, got length %d", len(result.FloatArray))
	}
	if len(result.BoolArray) != 0 {
		t.Errorf("Expected empty boolArray, got length %d", len(result.BoolArray))
	}
}
