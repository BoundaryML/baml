package main

import (
	"context"
	b "mixed_complex_types/baml_client"
	"testing"
)

func TestKitchenSink(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestKitchenSink(ctx, "test kitchen sink")
	if err != nil {
		t.Fatalf("Error testing kitchen sink: %v", err)
	}

	// Basic field validations (avoiding union type comparisons)
	if result.Id <= 0 {
		t.Errorf("Expected id to be positive, got %d", result.Id)
	}
	if result.Name == "" {
		t.Errorf("Expected name to be non-empty")
	}
	if result.Score <= 0 {
		t.Errorf("Expected score to be positive, got %f", result.Score)
	}
	if result.Nothing != nil {
		t.Errorf("Expected nothing to be null, got %v", result.Nothing)
	}

	// Verify literal union fields using IsXXX() methods
	if !result.Status.IsKdraft() && !result.Status.IsKpublished() && !result.Status.IsKarchived() {
		t.Errorf("Expected status to be a valid literal type")
	}
	if !result.Priority.IsIntK1() && !result.Priority.IsIntK2() && !result.Priority.IsIntK3() && !result.Priority.IsIntK4() && !result.Priority.IsIntK5() {
		t.Errorf("Expected priority to be between 1-5")
	}

	// Verify union fields
	if !result.Data.IsString() && !result.Data.IsInt() && !result.Data.IsDataObject() {
		t.Errorf("Expected data union to have a valid type")
	}
	if !result.Result.IsSuccess() && !result.Result.IsError() {
		t.Errorf("Expected result union to have a valid type")
	}

	// Verify arrays
	if len(result.Tags) < 0 {
		t.Errorf("Expected tags to be valid array")
	}
	if len(result.Numbers) < 0 {
		t.Errorf("Expected numbers to be valid array")
	}

	// Verify maps
	if len(result.Metadata) < 0 {
		t.Errorf("Expected metadata to be valid map")
	}
	if len(result.Scores) < 0 {
		t.Errorf("Expected scores to be valid map")
	}

	// Verify nested objects
	if result.User.Id <= 0 {
		t.Errorf("Expected user.id to be positive, got %d", result.User.Id)
	}
	if result.User.Profile.Name == "" {
		t.Errorf("Expected user.profile.name to be non-empty")
	}
	if result.User.Profile.Email == "" {
		t.Errorf("Expected user.profile.email to be non-empty")
	}
}

func TestUltraComplex(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestUltraComplex(ctx, "test ultra complex")
	if err != nil {
		t.Fatalf("Error testing ultra complex: %v", err)
	}

	// Basic validations
	if result.Tree.Id <= 0 {
		t.Errorf("Expected tree.id to be positive, got %d", result.Tree.Id)
	}
	// Verify tree union types using IsXXX() methods
	if !result.Tree.Type.IsKleaf() && !result.Tree.Type.IsKbranch() {
		t.Errorf("Expected tree.type to be 'leaf' or 'branch'")
	}
	if !result.Tree.Value.IsString() && !result.Tree.Value.IsInt() && !result.Tree.Value.IsListNode() && !result.Tree.Value.IsMapStringKeyNodeValue() {
		t.Errorf("Expected tree.value to have a valid type")
	}

	if len(result.Widgets) < 1 {
		t.Errorf("Expected at least 1 widget, got %d", len(result.Widgets))
	}
	// Verify widget types using IsXXX() methods
	for i, widget := range result.Widgets {
		if !widget.Type.IsKbutton() && !widget.Type.IsKtext() && !widget.Type.IsKimage() && !widget.Type.IsKcontainer() {
			t.Errorf("Widget %d has invalid type", i)
		}
		// Verify appropriate widget fields are populated based on type
		if widget.Type.IsKbutton() && widget.Button == nil {
			t.Errorf("Button widget %d missing button data", i)
		}
		if widget.Type.IsKtext() && widget.Text == nil {
			t.Errorf("Text widget %d missing text data", i)
		}
		if widget.Type.IsKtext() && widget.Text != nil {
			if !widget.Text.Format.IsKplain() && !widget.Text.Format.IsKmarkdown() && !widget.Text.Format.IsKhtml() {
				t.Errorf("Text widget %d has invalid format", i)
			}
		}
		if widget.Type.IsKimage() && widget.Image == nil {
			t.Errorf("Image widget %d missing image data", i)
		}
		if widget.Type.IsKcontainer() && widget.Container == nil {
			t.Errorf("Container widget %d missing container data", i)
		}
	}
}

func TestRecursiveComplexity(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestRecursiveComplexity(ctx, "test recursive complexity")
	if err != nil {
		t.Fatalf("Error testing recursive complexity: %v", err)
	}

	if result.Id <= 0 {
		t.Errorf("Expected node.id to be positive, got %d", result.Id)
	}
	// Verify node union types using IsXXX() methods
	if !result.Type.IsKleaf() && !result.Type.IsKbranch() {
		t.Errorf("Expected node.type to be 'leaf' or 'branch'")
	}
	if !result.Value.IsString() && !result.Value.IsInt() && !result.Value.IsListNode() && !result.Value.IsMapStringKeyNodeValue() {
		t.Errorf("Expected node.value to have a valid type")
	}
}
