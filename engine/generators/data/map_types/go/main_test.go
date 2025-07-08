package main

import (
	"context"
	b "map_types/baml_client"
	"math"
	"testing"
)

func TestSimpleMaps(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestSimpleMaps(ctx, "test simple maps")
	if err != nil {
		t.Fatalf("Error testing simple maps: %v", err)
	}

	// Verify simple map contents
	if len(result.StringToString) != 2 {
		t.Errorf("Expected stringToString length 2, got %d", len(result.StringToString))
	}
	if result.StringToString["key1"] != "value1" {
		t.Errorf("Expected stringToString['key1'] to be 'value1', got '%s'", result.StringToString["key1"])
	}

	if len(result.StringToInt) != 3 {
		t.Errorf("Expected stringToInt length 3, got %d", len(result.StringToInt))
	}
	if result.StringToInt["one"] != 1 {
		t.Errorf("Expected stringToInt['one'] to be 1, got %d", result.StringToInt["one"])
	}

	if len(result.StringToFloat) != 2 {
		t.Errorf("Expected stringToFloat length 2, got %d", len(result.StringToFloat))
	}
	if math.Abs(result.StringToFloat["pi"]-3.14159) > 0.0001 {
		t.Errorf("Expected stringToFloat['pi'] to be ~3.14159, got %f", result.StringToFloat["pi"])
	}

	if len(result.StringToBool) != 2 {
		t.Errorf("Expected stringToBool length 2, got %d", len(result.StringToBool))
	}
	if !result.StringToBool["isTrue"] {
		t.Errorf("Expected stringToBool['isTrue'] to be true, got false")
	}

	if len(result.IntToString) != 3 {
		t.Errorf("Expected intToString length 3, got %d", len(result.IntToString))
	}
	if result.IntToString["1"] != "one" {
		t.Errorf("Expected intToString['1'] to be 'one', got '%s'", result.IntToString["1"])
	}
}

func TestComplexMaps(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestComplexMaps(ctx, "test complex maps")
	if err != nil {
		t.Fatalf("Error testing complex maps: %v", err)
	}

	// Verify complex map contents
	if len(result.UserMap) < 2 {
		t.Errorf("Expected at least 2 users in userMap, got %d", len(result.UserMap))
	}
	for key, user := range result.UserMap {
		if user.Name == "" {
			t.Errorf("User '%s' has empty name", key)
		}
		if user.Email == "" {
			t.Errorf("User '%s' has empty email", key)
		}
	}

	if len(result.ProductMap) < 3 {
		t.Errorf("Expected at least 3 products in productMap, got %d", len(result.ProductMap))
	}
	for key, product := range result.ProductMap {
		if product.Name == "" {
			t.Errorf("Product %s has empty name", key)
		}
		if product.Price <= 0 {
			t.Errorf("Product %s has invalid price: %f", key, product.Price)
		}
	}

	if len(result.NestedMap) < 1 {
		t.Errorf("Expected at least 1 entry in nestedMap, got %d", len(result.NestedMap))
	}

	if len(result.ArrayMap) != 2 {
		t.Errorf("Expected arrayMap length 2, got %d", len(result.ArrayMap))
	}

	if len(result.MapArray) < 2 {
		t.Errorf("Expected at least 2 maps in mapArray, got %d", len(result.MapArray))
	}
}

func TestNestedMaps(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestNestedMaps(ctx, "test nested maps")
	if err != nil {
		t.Fatalf("Error testing nested maps: %v", err)
	}

	// Verify nested map structure
	if len(result.Simple) < 2 {
		t.Errorf("Expected at least 2 entries in simple map, got %d", len(result.Simple))
	}

	if len(result.OneLevelNested) < 2 {
		t.Errorf("Expected at least 2 entries in oneLevelNested, got %d", len(result.OneLevelNested))
	}
	for key, innerMap := range result.OneLevelNested {
		if len(innerMap) < 2 {
			t.Errorf("Expected at least 2 entries in oneLevelNested['%s'], got %d", key, len(innerMap))
		}
	}

	if len(result.TwoLevelNested) < 2 {
		t.Errorf("Expected at least 2 entries in twoLevelNested, got %d", len(result.TwoLevelNested))
	}

	if len(result.MapOfArrays) < 2 {
		t.Errorf("Expected at least 2 entries in mapOfArrays, got %d", len(result.MapOfArrays))
	}

	if len(result.MapOfMaps) < 2 {
		t.Errorf("Expected at least 2 entries in mapOfMaps, got %d", len(result.MapOfMaps))
	}
}

func TestEdgeCaseMaps(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestEdgeCaseMaps(ctx, "test edge case maps")
	if err != nil {
		t.Fatalf("Error testing edge case maps: %v", err)
	}

	// Verify edge case map contents
	if len(result.EmptyMap) != 0 {
		t.Errorf("Expected emptyMap to be empty, got length %d", len(result.EmptyMap))
	}

	if len(result.NullableValues) != 2 {
		t.Errorf("Expected nullableValues length 2, got %d", len(result.NullableValues))
	}
	if result.NullableValues["present"] == nil || *result.NullableValues["present"] != "value" {
		t.Errorf("Expected nullableValues['present'] to be 'value', got '%v'", result.NullableValues["present"])
	}

	if len(result.UnionValues) != 3 {
		t.Errorf("Expected unionValues length 3, got %d", len(result.UnionValues))
	}
}

func TestLargeMaps(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestLargeMaps(ctx, "test large maps")
	if err != nil {
		t.Fatalf("Error testing large maps: %v", err)
	}

	// Verify large map sizes (LLMs are fuzzy, so we may get a few less)
	if len(result.StringToString) < 15 {
		t.Errorf("Expected at least 20 entries in stringToString, got %d", len(result.StringToString))
	}
	if len(result.StringToInt) < 15 {
		t.Errorf("Expected at least 20 entries in stringToInt, got %d", len(result.StringToInt))
	}
	if len(result.StringToFloat) < 15 {
		t.Errorf("Expected at least 20 entries in stringToFloat, got %d", len(result.StringToFloat))
	}
	if len(result.StringToBool) < 15 {
		t.Errorf("Expected at least 20 entries in stringToBool, got %d", len(result.StringToBool))
	}
	if len(result.IntToString) < 15 {
		t.Errorf("Expected at least 20 entries in intToString, got %d", len(result.IntToString))
	}
}

// Test top-level map return types
func TestTopLevelStringMap(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestTopLevelStringMap(ctx, "test string map")
	if err != nil {
		t.Fatalf("Error testing top-level string map: %v", err)
	}

	if len(result) != 3 {
		t.Errorf("Expected 3 entries in string map, got %d", len(result))
	}
	if result["first"] != "Hello" || result["second"] != "World" || result["third"] != "BAML" {
		t.Errorf("Unexpected values in string map")
	}
}

func TestTopLevelIntMap(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestTopLevelIntMap(ctx, "test int map")
	if err != nil {
		t.Fatalf("Error testing top-level int map: %v", err)
	}

	if len(result) != 4 {
		t.Errorf("Expected 4 entries in int map, got %d", len(result))
	}
	if result["one"] != 1 || result["two"] != 2 || result["ten"] != 10 || result["hundred"] != 100 {
		t.Errorf("Unexpected values in int map")
	}
}

func TestTopLevelFloatMap(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestTopLevelFloatMap(ctx, "test float map")
	if err != nil {
		t.Fatalf("Error testing top-level float map: %v", err)
	}

	if len(result) != 3 {
		t.Errorf("Expected 3 entries in float map, got %d", len(result))
	}
	if math.Abs(result["pi"]-3.14159) > 0.0001 || math.Abs(result["e"]-2.71828) > 0.0001 {
		t.Errorf("Unexpected values in float map")
	}
}

func TestTopLevelBoolMap(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestTopLevelBoolMap(ctx, "test bool map")
	if err != nil {
		t.Fatalf("Error testing top-level bool map: %v", err)
	}

	if len(result) != 3 {
		t.Errorf("Expected 3 entries in bool map, got %d", len(result))
	}
	if !result["isActive"] || result["isDisabled"] || !result["isEnabled"] {
		t.Errorf("Unexpected values in bool map")
	}
}

func TestTopLevelNestedMap(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestTopLevelNestedMap(ctx, "test nested map")
	if err != nil {
		t.Fatalf("Error testing top-level nested map: %v", err)
	}

	if len(result) != 2 {
		t.Errorf("Expected 2 entries in nested map, got %d", len(result))
	}
	if len(result["users"]) != 2 || len(result["roles"]) != 2 {
		t.Errorf("Unexpected structure in nested map")
	}
}

func TestTopLevelMapOfArrays(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestTopLevelMapOfArrays(ctx, "test map of arrays")
	if err != nil {
		t.Fatalf("Error testing top-level map of arrays: %v", err)
	}

	if len(result) != 3 {
		t.Errorf("Expected 3 entries in map of arrays, got %d", len(result))
	}
	if len(result["evens"]) != 4 || len(result["odds"]) != 4 || len(result["primes"]) != 5 {
		t.Errorf("Unexpected array lengths in map of arrays")
	}
}

func TestTopLevelEmptyMap(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestTopLevelEmptyMap(ctx, "test empty map")
	if err != nil {
		t.Fatalf("Error testing top-level empty map: %v", err)
	}

	if len(result) != 0 {
		t.Errorf("Expected empty map, got %d entries", len(result))
	}
}

func TestTopLevelMapWithNullable(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestTopLevelMapWithNullable(ctx, "use jsut a json map")
	if err != nil {
		t.Fatalf("Error testing top-level map with nullable: %v", err)
	}

	if len(result) != 3 {
		t.Errorf("Expected 3 entries in nullable map, got %d", len(result))
	}
	if result["present"] == nil || *result["present"] != "value" {
		t.Errorf("Expected 'present' to have value 'value'")
	}
	if result["absent"] != nil {
		t.Errorf("Expected 'absent' to be nil")
	}
}

func TestTopLevelMapOfObjects(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestTopLevelMapOfObjects(ctx, "test object map")
	if err != nil {
		t.Fatalf("Error testing top-level map of objects: %v", err)
	}

	if len(result) != 2 {
		t.Errorf("Expected 2 entries in object map, got %d", len(result))
	}
	for key, user := range result {
		if user.Name == "" || user.Email == "" {
			t.Errorf("User %s has empty fields", key)
		}
	}
}
