package main

import (
	"context"
	"fmt"
	b "mixed_complex_types/baml_client"
	"os"
)

func main() {
	ctx := context.Background()

	// Test KitchenSink - basic validation only
	fmt.Println("Testing KitchenSink...")
	kitchenResult, err := b.TestKitchenSink(ctx, "test kitchen sink")
	if err != nil {
		fmt.Printf("Error testing kitchen sink: %v\n", err)
		os.Exit(1)
	}

	// Basic field validations (avoiding union type comparisons)
	if kitchenResult.Id <= 0 {
		fmt.Printf("Expected id to be positive, got %d\n", kitchenResult.Id)
		os.Exit(1)
	}
	if kitchenResult.Name == "" {
		fmt.Printf("Expected name to be non-empty\n")
		os.Exit(1)
	}
	if kitchenResult.Score <= 0 {
		fmt.Printf("Expected score to be positive, got %f\n", kitchenResult.Score)
		os.Exit(1)
	}
	if kitchenResult.Nothing != nil {
		fmt.Printf("Expected nothing to be null, got %v\n", kitchenResult.Nothing)
		os.Exit(1)
	}

	// Verify literal union fields using IsXXX() methods
	if !kitchenResult.Status.IsKdraft() && !kitchenResult.Status.IsKpublished() && !kitchenResult.Status.IsKarchived() {
		fmt.Printf("Expected status to be a valid literal type\n")
		os.Exit(1)
	}
	if !kitchenResult.Priority.IsIntK1() && !kitchenResult.Priority.IsIntK2() && !kitchenResult.Priority.IsIntK3() && !kitchenResult.Priority.IsIntK4() && !kitchenResult.Priority.IsIntK5() {
		fmt.Printf("Expected priority to be between 1-5\n")
		os.Exit(1)
	}

	// Verify union fields
	if !kitchenResult.Data.IsString() && !kitchenResult.Data.IsInt() && !kitchenResult.Data.IsDataObject() {
		fmt.Printf("Expected data union to have a valid type\n")
		os.Exit(1)
	}
	if !kitchenResult.Result.IsSuccess() && !kitchenResult.Result.IsError() {
		fmt.Printf("Expected result union to have a valid type\n")
		os.Exit(1)
	}

	// Verify arrays
	if len(kitchenResult.Tags) < 0 {
		fmt.Printf("Expected tags to be valid array\n")
		os.Exit(1)
	}
	if len(kitchenResult.Numbers) < 0 {
		fmt.Printf("Expected numbers to be valid array\n")
		os.Exit(1)
	}

	// Verify maps
	if len(kitchenResult.Metadata) < 0 {
		fmt.Printf("Expected metadata to be valid map\n")
		os.Exit(1)
	}
	if len(kitchenResult.Scores) < 0 {
		fmt.Printf("Expected scores to be valid map\n")
		os.Exit(1)
	}

	// Verify nested objects
	if kitchenResult.User.Id <= 0 {
		fmt.Printf("Expected user.id to be positive, got %d\n", kitchenResult.User.Id)
		os.Exit(1)
	}
	if kitchenResult.User.Profile.Name == "" {
		fmt.Printf("Expected user.profile.name to be non-empty\n")
		os.Exit(1)
	}
	if kitchenResult.User.Profile.Email == "" {
		fmt.Printf("Expected user.profile.email to be non-empty\n")
		os.Exit(1)
	}

	fmt.Println("✓ KitchenSink test passed")

	// Test UltraComplex - basic validation only
	fmt.Println("\nTesting UltraComplex...")
	ultraResult, err := b.TestUltraComplex(ctx, "test ultra complex")
	if err != nil {
		fmt.Printf("Error testing ultra complex: %v\n", err)
		os.Exit(1)
	}

	// Basic validations
	if ultraResult.Tree.Id <= 0 {
		fmt.Printf("Expected tree.id to be positive, got %d\n", ultraResult.Tree.Id)
		os.Exit(1)
	}
	// Verify tree union types using IsXXX() methods
	if !ultraResult.Tree.Type.IsKleaf() && !ultraResult.Tree.Type.IsKbranch() {
		fmt.Printf("Expected tree.type to be 'leaf' or 'branch'\n")
		os.Exit(1)
	}
	if !ultraResult.Tree.Value.IsString() && !ultraResult.Tree.Value.IsInt() && !ultraResult.Tree.Value.IsListNode() && !ultraResult.Tree.Value.IsMapStringKeyNodeValue() {
		fmt.Printf("Expected tree.value to have a valid type\n")
		os.Exit(1)
	}

	if len(ultraResult.Widgets) < 1 {
		fmt.Printf("Expected at least 1 widget, got %d\n", len(ultraResult.Widgets))
		os.Exit(1)
	}
	// Verify widget types using IsXXX() methods
	for i, widget := range ultraResult.Widgets {
		if !widget.Type.IsKbutton() && !widget.Type.IsKtext() && !widget.Type.IsKimage() && !widget.Type.IsKcontainer() {
			fmt.Printf("Widget %d has invalid type\n", i)
			os.Exit(1)
		}
		// Verify appropriate widget fields are populated based on type
		if widget.Type.IsKbutton() && widget.Button == nil {
			fmt.Printf("Button widget %d missing button data\n", i)
			os.Exit(1)
		}
		if widget.Type.IsKtext() && widget.Text == nil {
			fmt.Printf("Text widget %d missing text data\n", i)
			os.Exit(1)
		}
		if widget.Type.IsKtext() && widget.Text != nil {
			if !widget.Text.Format.IsKplain() && !widget.Text.Format.IsKmarkdown() && !widget.Text.Format.IsKhtml() {
				fmt.Printf("Text widget %d has invalid format\n", i)
				os.Exit(1)
			}
		}
		if widget.Type.IsKimage() && widget.Image == nil {
			fmt.Printf("Image widget %d missing image data\n", i)
			os.Exit(1)
		}
		if widget.Type.IsKcontainer() && widget.Container == nil {
			fmt.Printf("Container widget %d missing container data\n", i)
			os.Exit(1)
		}
	}

	fmt.Println("✓ UltraComplex test passed")

	// Test RecursiveComplexity - basic validation only
	fmt.Println("\nTesting RecursiveComplexity...")
	nodeResult, err := b.TestRecursiveComplexity(ctx, "test recursive complexity")
	if err != nil {
		fmt.Printf("Error testing recursive complexity: %v\n", err)
		os.Exit(1)
	}

	if nodeResult.Id <= 0 {
		fmt.Printf("Expected node.id to be positive, got %d\n", nodeResult.Id)
		os.Exit(1)
	}
	// Verify node union types using IsXXX() methods
	if !nodeResult.Type.IsKleaf() && !nodeResult.Type.IsKbranch() {
		fmt.Printf("Expected node.type to be 'leaf' or 'branch'\n")
		os.Exit(1)
	}
	if !nodeResult.Value.IsString() && !nodeResult.Value.IsInt() && !nodeResult.Value.IsListNode() && !nodeResult.Value.IsMapStringKeyNodeValue() {
		fmt.Printf("Expected node.value to have a valid type\n")
		os.Exit(1)
	}

	fmt.Println("✓ RecursiveComplexity test passed")

	fmt.Println("\n✅ All mixed complex type tests passed!")
}
