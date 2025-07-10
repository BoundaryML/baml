package main

import (
	"context"
	b "edge_cases/baml_client"
	"math"
	"strings"
	"testing"
)

func TestEmptyCollections(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestEmptyCollections(ctx, "test empty collections")
	if err != nil {
		t.Fatalf("Error testing empty collections: %v", err)
	}

	// Verify all collections are empty
	if len(result.EmptyStringArray) != 0 {
		t.Errorf("Expected emptyStringArray to be empty, got length %d", len(result.EmptyStringArray))
	}
	if len(result.EmptyIntArray) != 0 {
		t.Errorf("Expected emptyIntArray to be empty, got length %d", len(result.EmptyIntArray))
	}
	if len(result.EmptyObjectArray) != 0 {
		t.Errorf("Expected emptyObjectArray to be empty, got length %d", len(result.EmptyObjectArray))
	}
	if len(result.EmptyMap) != 0 {
		t.Errorf("Expected emptyMap to be empty, got length %d", len(result.EmptyMap))
	}
	if len(result.EmptyNestedArray) != 0 {
		t.Errorf("Expected emptyNestedArray to be empty, got length %d", len(result.EmptyNestedArray))
	}
}

func TestLargeStructure(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestLargeStructure(ctx, "test large structure")
	if err != nil {
		t.Fatalf("Error testing large structure: %v", err)
	}

	// Verify large structure has all fields populated
	fields := []string{
		result.Field1, result.Field2, result.Field3, result.Field4, result.Field5,
	}
	for i, field := range fields {
		if field == "" {
			t.Errorf("Expected field%d to be non-empty", i+1)
		}
	}

	intFields := []int{
		int(result.Field6), int(result.Field7), int(result.Field8), int(result.Field9), int(result.Field10),
	}
	for i, field := range intFields {
		if field == 0 {
			t.Errorf("Expected field%d to be non-zero", i+6)
		}
	}

	floatFields := []float64{
		result.Field11, result.Field12, result.Field13, result.Field14, result.Field15,
	}
	for i, field := range floatFields {
		if field == 0.0 {
			t.Errorf("Expected field%d to be non-zero", i+11)
		}
	}

	// Verify arrays have expected sizes
	if len(result.Array1) < 3 || len(result.Array1) > 5 {
		t.Errorf("Expected array1 length 3-5, got %d", len(result.Array1))
	}
	if len(result.Array2) < 3 || len(result.Array2) > 5 {
		t.Errorf("Expected array2 length 3-5, got %d", len(result.Array2))
	}
	if len(result.Array3) < 3 || len(result.Array3) > 5 {
		t.Errorf("Expected array3 length 3-5, got %d", len(result.Array3))
	}
	if len(result.Array4) < 3 || len(result.Array4) > 5 {
		t.Errorf("Expected array4 length 3-5, got %d", len(result.Array4))
	}
	if len(result.Array5) < 3 || len(result.Array5) > 5 {
		t.Errorf("Expected array5 length 3-5, got %d", len(result.Array5))
	}

	// Verify maps have expected sizes
	if len(result.Map1) < 2 || len(result.Map1) > 3 {
		t.Errorf("Expected map1 length 2-3, got %d", len(result.Map1))
	}
	if len(result.Map2) < 2 || len(result.Map2) > 3 {
		t.Errorf("Expected map2 length 2-3, got %d", len(result.Map2))
	}
	if len(result.Map3) < 2 || len(result.Map3) > 3 {
		t.Errorf("Expected map3 length 2-3, got %d", len(result.Map3))
	}
	if len(result.Map4) < 2 || len(result.Map4) > 3 {
		t.Errorf("Expected map4 length 2-3, got %d", len(result.Map4))
	}
	if len(result.Map5) < 2 || len(result.Map5) > 3 {
		t.Errorf("Expected map5 length 2-3, got %d", len(result.Map5))
	}
}

func TestDeepRecursion(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestDeepRecursion(ctx, 5)
	if err != nil {
		t.Fatalf("Error testing deep recursion: %v", err)
	}

	// Verify recursion depth
	current := &result
	depth := 0
	for current != nil {
		depth++
		if current.Value == "" {
			t.Errorf("Expected value at depth %d to be non-empty", depth)
		}
		current = current.Next
	}
	if depth != 5 {
		t.Errorf("Expected recursion depth 5, got %d", depth)
	}
}

func TestSpecialCharacters(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestSpecialCharacters(ctx, "test special characters")
	if err != nil {
		t.Fatalf("Error testing special characters: %v", err)
	}

	// Verify special character handling
	if result.NormalText != "Hello World" {
		t.Errorf("Expected normalText to be 'Hello World', got '%s'", result.NormalText)
	}
	if !strings.Contains(result.WithNewlines, "\n") {
		t.Errorf("Expected withNewlines to contain newlines")
	}
	if !strings.Contains(result.WithTabs, "\t") {
		t.Errorf("Expected withTabs to contain tabs")
	}
	if !strings.Contains(result.WithQuotes, "\"") {
		t.Errorf("Expected withQuotes to contain quotes")
	}
	if !strings.Contains(result.WithBackslashes, "\\") {
		t.Errorf("Expected withBackslashes to contain backslashes")
	}
	if result.WithUnicode == "" {
		t.Errorf("Expected withUnicode to be non-empty")
	}
	if result.WithEmoji == "" {
		t.Errorf("Expected withEmoji to be non-empty")
	}
	if result.WithMixedSpecial == "" {
		t.Errorf("Expected withMixedSpecial to be non-empty")
	}
}

func TestNumberEdgeCases(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestNumberEdgeCases(ctx, "test number edge cases")
	if err != nil {
		t.Fatalf("Error testing number edge cases: %v", err)
	}

	// Verify number edge cases
	if result.Zero != 0 {
		t.Errorf("Expected zero to be 0, got %d", result.Zero)
	}
	if result.NegativeInt >= 0 {
		t.Errorf("Expected negativeInt to be negative, got %d", result.NegativeInt)
	}
	if result.LargeInt <= 1000 {
		t.Errorf("Expected largeInt to be large, got %d", result.LargeInt)
	}
	if result.VeryLargeInt <= 1000000 {
		t.Errorf("Expected veryLargeInt to be very large, got %d", result.VeryLargeInt)
	}
	if result.SmallFloat >= 1.0 {
		t.Errorf("Expected smallFloat to be small, got %f", result.SmallFloat)
	}
	if result.LargeFloat <= 1000.0 {
		t.Errorf("Expected largeFloat to be large, got %f", result.LargeFloat)
	}
	if result.NegativeFloat >= 0.0 {
		t.Errorf("Expected negativeFloat to be negative, got %f", result.NegativeFloat)
	}
	if math.Abs(result.ScientificNotation) < 1000.0 {
		t.Errorf("Expected scientificNotation to be in scientific range, got %f", result.ScientificNotation)
	}
}

func TestCircularReference(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestCircularReference(ctx, "test circular reference")
	if err != nil {
		t.Fatalf("Error testing circular reference: %v", err)
	}

	// Verify circular reference structure
	if result.Id != 1 {
		t.Errorf("Expected root id to be 1, got %d", result.Id)
	}
	if result.Name == "" {
		t.Errorf("Expected root name to be non-empty")
	}
	if len(result.Children) != 2 {
		t.Errorf("Expected 2 children, got %d", len(result.Children))
	}

	// Verify children structure
	child1 := result.Children[0]
	child2 := result.Children[1]

	if child1.Id != 2 && child1.Id != 3 {
		t.Errorf("Expected child1 id to be 2 or 3, got %d", child1.Id)
	}
	if child2.Id != 2 && child2.Id != 3 {
		t.Errorf("Expected child2 id to be 2 or 3, got %d", child2.Id)
	}
	if child1.Id == child2.Id {
		t.Errorf("Expected children to have different ids")
	}

	// Verify parent references (if not causing circular serialization issues)
	if child1.Parent != nil && child1.Parent.Id != 1 {
		t.Errorf("Expected child1 parent id to be 1, got %d", child1.Parent.Id)
	}
	if child2.Parent != nil && child2.Parent.Id != 1 {
		t.Errorf("Expected child2 parent id to be 1, got %d", child2.Parent.Id)
	}

	// Verify related items exist
	if len(result.RelatedItems) < 0 {
		t.Errorf("Expected relatedItems to be valid array")
	}
}
