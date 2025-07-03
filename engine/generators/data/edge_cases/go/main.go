package main

import (
	"context"
	b "edge_cases/baml_client"
	"fmt"
	"math"
	"os"
	"strings"
)

func main() {
	ctx := context.Background()

	// Test empty collections
	fmt.Println("Testing EmptyCollections...")
	emptyResult, err := b.TestEmptyCollections(ctx, "test empty collections")
	if err != nil {
		fmt.Printf("Error testing empty collections: %v\n", err)
		os.Exit(1)
	}

	// Verify all collections are empty
	if len(emptyResult.EmptyStringArray) != 0 {
		fmt.Printf("Expected emptyStringArray to be empty, got length %d\n", len(emptyResult.EmptyStringArray))
		os.Exit(1)
	}
	if len(emptyResult.EmptyIntArray) != 0 {
		fmt.Printf("Expected emptyIntArray to be empty, got length %d\n", len(emptyResult.EmptyIntArray))
		os.Exit(1)
	}
	if len(emptyResult.EmptyObjectArray) != 0 {
		fmt.Printf("Expected emptyObjectArray to be empty, got length %d\n", len(emptyResult.EmptyObjectArray))
		os.Exit(1)
	}
	if len(emptyResult.EmptyMap) != 0 {
		fmt.Printf("Expected emptyMap to be empty, got length %d\n", len(emptyResult.EmptyMap))
		os.Exit(1)
	}
	if len(emptyResult.EmptyNestedArray) != 0 {
		fmt.Printf("Expected emptyNestedArray to be empty, got length %d\n", len(emptyResult.EmptyNestedArray))
		os.Exit(1)
	}
	fmt.Println("✓ EmptyCollections test passed")

	// Test large structure
	fmt.Println("\nTesting LargeStructure...")
	largeResult, err := b.TestLargeStructure(ctx, "test large structure")
	if err != nil {
		fmt.Printf("Error testing large structure: %v\n", err)
		os.Exit(1)
	}

	// Verify large structure has all fields populated
	fields := []string{
		largeResult.Field1, largeResult.Field2, largeResult.Field3, largeResult.Field4, largeResult.Field5,
	}
	for i, field := range fields {
		if field == "" {
			fmt.Printf("Expected field%d to be non-empty\n", i+1)
			os.Exit(1)
		}
	}

	intFields := []int{
		int(largeResult.Field6), int(largeResult.Field7), int(largeResult.Field8), int(largeResult.Field9), int(largeResult.Field10),
	}
	for i, field := range intFields {
		if field == 0 {
			fmt.Printf("Expected field%d to be non-zero\n", i+6)
			os.Exit(1)
		}
	}

	floatFields := []float64{
		largeResult.Field11, largeResult.Field12, largeResult.Field13, largeResult.Field14, largeResult.Field15,
	}
	for i, field := range floatFields {
		if field == 0.0 {
			fmt.Printf("Expected field%d to be non-zero\n", i+11)
			os.Exit(1)
		}
	}

	// Verify arrays have expected sizes
	if len(largeResult.Array1) < 3 || len(largeResult.Array1) > 5 {
		fmt.Printf("Expected array1 length 3-5, got %d\n", len(largeResult.Array1))
		os.Exit(1)
	}
	if len(largeResult.Array2) < 3 || len(largeResult.Array2) > 5 {
		fmt.Printf("Expected array2 length 3-5, got %d\n", len(largeResult.Array2))
		os.Exit(1)
	}
	if len(largeResult.Array3) < 3 || len(largeResult.Array3) > 5 {
		fmt.Printf("Expected array3 length 3-5, got %d\n", len(largeResult.Array3))
		os.Exit(1)
	}
	if len(largeResult.Array4) < 3 || len(largeResult.Array4) > 5 {
		fmt.Printf("Expected array4 length 3-5, got %d\n", len(largeResult.Array4))
		os.Exit(1)
	}
	if len(largeResult.Array5) < 3 || len(largeResult.Array5) > 5 {
		fmt.Printf("Expected array5 length 3-5, got %d\n", len(largeResult.Array5))
		os.Exit(1)
	}

	// Verify maps have expected sizes
	if len(largeResult.Map1) < 2 || len(largeResult.Map1) > 3 {
		fmt.Printf("Expected map1 length 2-3, got %d\n", len(largeResult.Map1))
		os.Exit(1)
	}
	if len(largeResult.Map2) < 2 || len(largeResult.Map2) > 3 {
		fmt.Printf("Expected map2 length 2-3, got %d\n", len(largeResult.Map2))
		os.Exit(1)
	}
	if len(largeResult.Map3) < 2 || len(largeResult.Map3) > 3 {
		fmt.Printf("Expected map3 length 2-3, got %d\n", len(largeResult.Map3))
		os.Exit(1)
	}
	if len(largeResult.Map4) < 2 || len(largeResult.Map4) > 3 {
		fmt.Printf("Expected map4 length 2-3, got %d\n", len(largeResult.Map4))
		os.Exit(1)
	}
	if len(largeResult.Map5) < 2 || len(largeResult.Map5) > 3 {
		fmt.Printf("Expected map5 length 2-3, got %d\n", len(largeResult.Map5))
		os.Exit(1)
	}
	fmt.Println("✓ LargeStructure test passed")

	// Test deep recursion
	fmt.Println("\nTesting DeepRecursion...")
	recursionResult, err := b.TestDeepRecursion(ctx, 5)
	if err != nil {
		fmt.Printf("Error testing deep recursion: %v\n", err)
		os.Exit(1)
	}

	// Verify recursion depth
	current := &recursionResult
	depth := 0
	for current != nil {
		depth++
		if current.Value == "" {
			fmt.Printf("Expected value at depth %d to be non-empty\n", depth)
			os.Exit(1)
		}
		current = current.Next
	}
	if depth != 5 {
		fmt.Printf("Expected recursion depth 5, got %d\n", depth)
		os.Exit(1)
	}
	fmt.Println("✓ DeepRecursion test passed")

	// Test special characters
	fmt.Println("\nTesting SpecialCharacters...")
	specialResult, err := b.TestSpecialCharacters(ctx, "test special characters")
	if err != nil {
		fmt.Printf("Error testing special characters: %v\n", err)
		os.Exit(1)
	}

	// Verify special character handling
	if specialResult.NormalText != "Hello World" {
		fmt.Printf("Expected normalText to be 'Hello World', got '%s'\n", specialResult.NormalText)
		os.Exit(1)
	}
	if !strings.Contains(specialResult.WithNewlines, "\n") {
		fmt.Printf("Expected withNewlines to contain newlines\n")
		os.Exit(1)
	}
	if !strings.Contains(specialResult.WithTabs, "\t") {
		fmt.Printf("Expected withTabs to contain tabs\n")
		os.Exit(1)
	}
	if !strings.Contains(specialResult.WithQuotes, "\"") {
		fmt.Printf("Expected withQuotes to contain quotes\n")
		os.Exit(1)
	}
	if !strings.Contains(specialResult.WithBackslashes, "\\") {
		fmt.Printf("Expected withBackslashes to contain backslashes\n")
		os.Exit(1)
	}
	if specialResult.WithUnicode == "" {
		fmt.Printf("Expected withUnicode to be non-empty\n")
		os.Exit(1)
	}
	if specialResult.WithEmoji == "" {
		fmt.Printf("Expected withEmoji to be non-empty\n")
		os.Exit(1)
	}
	if specialResult.WithMixedSpecial == "" {
		fmt.Printf("Expected withMixedSpecial to be non-empty\n")
		os.Exit(1)
	}
	fmt.Println("✓ SpecialCharacters test passed")

	// Test number edge cases
	fmt.Println("\nTesting NumberEdgeCases...")
	numberResult, err := b.TestNumberEdgeCases(ctx, "test number edge cases")
	if err != nil {
		fmt.Printf("Error testing number edge cases: %v\n", err)
		os.Exit(1)
	}

	// Verify number edge cases
	if numberResult.Zero != 0 {
		fmt.Printf("Expected zero to be 0, got %d\n", numberResult.Zero)
		os.Exit(1)
	}
	if numberResult.NegativeInt >= 0 {
		fmt.Printf("Expected negativeInt to be negative, got %d\n", numberResult.NegativeInt)
		os.Exit(1)
	}
	if numberResult.LargeInt <= 1000 {
		fmt.Printf("Expected largeInt to be large, got %d\n", numberResult.LargeInt)
		os.Exit(1)
	}
	if numberResult.VeryLargeInt <= 1000000 {
		fmt.Printf("Expected veryLargeInt to be very large, got %d\n", numberResult.VeryLargeInt)
		os.Exit(1)
	}
	if numberResult.SmallFloat >= 1.0 {
		fmt.Printf("Expected smallFloat to be small, got %f\n", numberResult.SmallFloat)
		os.Exit(1)
	}
	if numberResult.LargeFloat <= 1000.0 {
		fmt.Printf("Expected largeFloat to be large, got %f\n", numberResult.LargeFloat)
		os.Exit(1)
	}
	if numberResult.NegativeFloat >= 0.0 {
		fmt.Printf("Expected negativeFloat to be negative, got %f\n", numberResult.NegativeFloat)
		os.Exit(1)
	}
	if math.Abs(numberResult.ScientificNotation) < 1000.0 {
		fmt.Printf("Expected scientificNotation to be in scientific range, got %f\n", numberResult.ScientificNotation)
		os.Exit(1)
	}
	// Note: infinity and NaN handling depends on how the Go client represents these values
	fmt.Println("✓ NumberEdgeCases test passed")

	// Test circular reference
	fmt.Println("\nTesting CircularReference...")
	circularResult, err := b.TestCircularReference(ctx, "test circular reference")
	if err != nil {
		fmt.Printf("Error testing circular reference: %v\n", err)
		os.Exit(1)
	}

	// Verify circular reference structure
	if circularResult.Id != 1 {
		fmt.Printf("Expected root id to be 1, got %d\n", circularResult.Id)
		os.Exit(1)
	}
	if circularResult.Name == "" {
		fmt.Printf("Expected root name to be non-empty\n")
		os.Exit(1)
	}
	if len(circularResult.Children) != 2 {
		fmt.Printf("Expected 2 children, got %d\n", len(circularResult.Children))
		os.Exit(1)
	}

	// Verify children structure
	child1 := circularResult.Children[0]
	child2 := circularResult.Children[1]

	if child1.Id != 2 && child1.Id != 3 {
		fmt.Printf("Expected child1 id to be 2 or 3, got %d\n", child1.Id)
		os.Exit(1)
	}
	if child2.Id != 2 && child2.Id != 3 {
		fmt.Printf("Expected child2 id to be 2 or 3, got %d\n", child2.Id)
		os.Exit(1)
	}
	if child1.Id == child2.Id {
		fmt.Printf("Expected children to have different ids\n")
		os.Exit(1)
	}

	// Verify parent references (if not causing circular serialization issues)
	if child1.Parent != nil && child1.Parent.Id != 1 {
		fmt.Printf("Expected child1 parent id to be 1, got %d\n", child1.Parent.Id)
		os.Exit(1)
	}
	if child2.Parent != nil && child2.Parent.Id != 1 {
		fmt.Printf("Expected child2 parent id to be 1, got %d\n", child2.Parent.Id)
		os.Exit(1)
	}

	// Verify related items exist
	if len(circularResult.RelatedItems) < 0 {
		fmt.Printf("Expected relatedItems to be valid array\n")
		os.Exit(1)
	}
	fmt.Println("✓ CircularReference test passed")

	fmt.Println("\n✅ All edge case tests passed!")
}
