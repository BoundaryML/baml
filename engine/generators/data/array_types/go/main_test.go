package main

import (
	b "array_types/baml_client"
	"context"
	"testing"
)

func TestSimpleArrays(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestSimpleArrays(ctx, "test simple arrays")
	if err != nil {
		t.Fatalf("Error testing simple arrays: %v", err)
	}

	// Verify simple array contents
	if len(result.Strings) != 3 {
		t.Errorf("Expected strings length 3, got %d", len(result.Strings))
	}
	if len(result.Integers) != 5 {
		t.Errorf("Expected integers length 5, got %d", len(result.Integers))
	}
	if len(result.Floats) != 3 {
		t.Errorf("Expected floats length 3, got %d", len(result.Floats))
	}
	if len(result.Booleans) != 4 {
		t.Errorf("Expected booleans length 4, got %d", len(result.Booleans))
	}
}

func TestNestedArrays(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestNestedArrays(ctx, "test nested arrays")
	if err != nil {
		t.Fatalf("Error testing nested arrays: %v", err)
	}

	// Verify nested array structure
	if len(result.Matrix) != 3 {
		t.Errorf("Expected matrix length 3, got %d", len(result.Matrix))
	}
	if len(result.Matrix[0]) != 3 {
		t.Errorf("Expected matrix[0] length 3, got %d", len(result.Matrix[0]))
	}
	if len(result.StringMatrix) != 2 {
		t.Errorf("Expected stringMatrix length 2, got %d", len(result.StringMatrix))
	}
	if len(result.ThreeDimensional) != 2 {
		t.Errorf("Expected threeDimensional length 2, got %d", len(result.ThreeDimensional))
	}
}

func TestObjectArrays(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestObjectArrays(ctx, "test object arrays")
	if err != nil {
		t.Fatalf("Error testing object arrays: %v", err)
	}

	// Verify object array contents
	if len(result.Users) < 3 {
		t.Errorf("Expected at least 3 users, got %d", len(result.Users))
	}
	if len(result.Products) < 2 {
		t.Errorf("Expected at least 2 products, got %d", len(result.Products))
	}
	if len(result.Tags) < 4 {
		t.Errorf("Expected at least 4 tags, got %d", len(result.Tags))
	}

	// Verify user objects have required fields
	for i, user := range result.Users {
		if user.Id <= 0 {
			t.Errorf("User %d has invalid id: %d", i, user.Id)
		}
		if user.Name == "" {
			t.Errorf("User %d has empty name", i)
		}
		if user.Email == "" {
			t.Errorf("User %d has empty email", i)
		}
	}
}

func TestMixedArrays(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestMixedArrays(ctx, "test mixed arrays")
	if err != nil {
		t.Fatalf("Error testing mixed arrays: %v", err)
	}

	// Verify mixed array contents
	if len(result.PrimitiveArray) != 4 {
		t.Errorf("Expected primitiveArray length 4, got %d", len(result.PrimitiveArray))
	}
	if len(result.NullableArray) != 4 {
		t.Errorf("Expected nullableArray length 4, got %d", len(result.NullableArray))
	}
	if len(result.OptionalItems) < 2 {
		t.Errorf("Expected at least 2 optionalItems, got %d", len(result.OptionalItems))
	}
	if len(result.ArrayOfArrays) < 2 {
		t.Errorf("Expected at least 2 arrayOfArrays, got %d", len(result.ArrayOfArrays))
	}
	if len(result.ComplexMixed) < 2 {
		t.Errorf("Expected at least 2 complexMixed items, got %d", len(result.ComplexMixed))
	}
}

func TestEmptyArrays(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestEmptyArrays(ctx, "test empty arrays")
	if err != nil {
		t.Fatalf("Error testing empty arrays: %v", err)
	}

	// Verify all arrays are empty
	if len(result.Strings) != 0 {
		t.Errorf("Expected empty strings array, got length %d", len(result.Strings))
	}
	if len(result.Integers) != 0 {
		t.Errorf("Expected empty integers array, got length %d", len(result.Integers))
	}
	if len(result.Floats) != 0 {
		t.Errorf("Expected empty floats array, got length %d", len(result.Floats))
	}
	if len(result.Booleans) != 0 {
		t.Errorf("Expected empty booleans array, got length %d", len(result.Booleans))
	}
}

func TestLargeArrays(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestLargeArrays(ctx, "test large arrays")
	if err != nil {
		t.Fatalf("Error testing large arrays: %v", err)
	}

	// Verify large array sizes
	if len(result.Strings) < 40 {
		t.Errorf("Expected at least 40 strings, got %d", len(result.Strings))
	}
	if len(result.Integers) < 50 {
		t.Errorf("Expected at least 50 integers, got %d", len(result.Integers))
	}
	if len(result.Floats) < 20 {
		t.Errorf("Expected at least 20 floats, got %d", len(result.Floats))
	}
	if len(result.Booleans) < 15 {
		t.Errorf("Expected at least 15 booleans, got %d", len(result.Booleans))
	}
}

// Test top-level array return types
func TestTopLevelStringArray(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestTopLevelStringArray(ctx, "test string array")
	if err != nil {
		t.Fatalf("Error testing top-level string array: %v", err)
	}
	if len(result) != 4 {
		t.Errorf("Expected 4 strings, got %d", len(result))
	}
	if result[0] != "apple" || result[1] != "banana" || result[2] != "cherry" || result[3] != "date" {
		t.Errorf("Unexpected values in string array")
	}
}

func TestTopLevelIntArray(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestTopLevelIntArray(ctx, "test int array")
	if err != nil {
		t.Fatalf("Error testing top-level int array: %v", err)
	}
	if len(result) != 5 {
		t.Errorf("Expected 5 integers, got %d", len(result))
	}
	if result[0] != 10 || result[1] != 20 || result[2] != 30 || result[3] != 40 || result[4] != 50 {
		t.Errorf("Unexpected values in int array")
	}
}

func TestTopLevelFloatArray(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestTopLevelFloatArray(ctx, "test float array")
	if err != nil {
		t.Fatalf("Error testing top-level float array: %v", err)
	}
	if len(result) != 4 {
		t.Errorf("Expected 4 floats, got %d", len(result))
	}
	if result[0] != 1.5 || result[1] != 2.5 || result[2] != 3.5 || result[3] != 4.5 {
		t.Errorf("Unexpected values in float array")
	}
}

func TestTopLevelBoolArray(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestTopLevelBoolArray(ctx, "test bool array")
	if err != nil {
		t.Fatalf("Error testing top-level bool array: %v", err)
	}
	if len(result) != 5 {
		t.Errorf("Expected 5 booleans, got %d", len(result))
	}
	if !result[0] || result[1] || !result[2] || result[3] || !result[4] {
		t.Errorf("Unexpected values in bool array")
	}
}

func TestTopLevelNestedArray(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestTopLevelNestedArray(ctx, "test nested array")
	if err != nil {
		t.Fatalf("Error testing top-level nested array: %v", err)
	}
	if len(result) != 3 {
		t.Errorf("Expected 3 rows, got %d", len(result))
	}
	for i, row := range result {
		if len(row) != 3 {
			t.Errorf("Expected 3 columns in row %d, got %d", i, len(row))
		}
	}
}

func TestTopLevel3DArray(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestTopLevel3DArray(ctx, "test 3D array")
	if err != nil {
		t.Fatalf("Error testing top-level 3D array: %v", err)
	}
	if len(result) != 2 {
		t.Errorf("Expected 2 levels, got %d", len(result))
	}
	for i, level := range result {
		if len(level) != 2 {
			t.Errorf("Expected 2 rows in level %d, got %d", i, len(level))
		}
		for j, row := range level {
			if len(row) != 2 {
				t.Errorf("Expected 2 columns in level %d row %d, got %d", i, j, len(row))
			}
		}
	}
}

func TestTopLevelEmptyArray(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestTopLevelEmptyArray(ctx, "test empty array")
	if err != nil {
		t.Fatalf("Error testing top-level empty array: %v", err)
	}
	if len(result) != 0 {
		t.Errorf("Expected empty array, got %d elements", len(result))
	}
}

func TestTopLevelNullableArray(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestTopLevelNullableArray(ctx, "test nullable array")
	if err != nil {
		t.Fatalf("Error testing top-level nullable array: %v", err)
	}
	if len(result) != 5 {
		t.Errorf("Expected 5 elements in nullable array, got %d", len(result))
	}
	if result[0] == nil || *result[0] != "hello" {
		t.Errorf("Expected first element to be 'hello'")
	}
	if result[1] != nil {
		t.Errorf("Expected second element to be nil")
	}
}

func TestTopLevelObjectArray(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestTopLevelObjectArray(ctx, "test object array")
	if err != nil {
		t.Fatalf("Error testing top-level object array: %v", err)
	}
	if len(result) != 3 {
		t.Errorf("Expected 3 users, got %d", len(result))
	}
	for i, user := range result {
		if user.Name == "" || user.Email == "" {
			t.Errorf("User %d has empty fields", i)
		}
	}
}

func TestTopLevelMixedArray(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestTopLevelMixedArray(ctx, "test mixed array")
	if err != nil {
		t.Fatalf("Error testing top-level mixed array: %v", err)
	}
	if len(result) != 6 {
		t.Errorf("Expected 6 elements in mixed array, got %d", len(result))
	}
}

func TestTopLevelArrayOfMaps(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestTopLevelArrayOfMaps(ctx, "test array of maps")
	if err != nil {
		t.Fatalf("Error testing top-level array of maps: %v", err)
	}
	if len(result) != 3 {
		t.Errorf("Expected 3 maps in array, got %d", len(result))
	}
	if len(result[0]) != 2 || len(result[1]) != 2 || len(result[2]) != 2 {
		t.Errorf("Unexpected map sizes in array of maps")
	}
}
