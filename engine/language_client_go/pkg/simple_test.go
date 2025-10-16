package baml

import (
	"testing"
)

func TestSimpleTypeBuilder(t *testing.T) {
	rt, err := CreateRuntime(".", map[string]string{}, map[string]string{})
	if err != nil {
		t.Fatalf("Failed to create runtime: %v", err)
	}

	// Test creating a type builder - this is where the error occurs
	t.Log("Attempting to create type builder...")
	typeBuilder, err := rt.NewTypeBuilder()
	if err != nil {
		t.Fatalf("Failed to create type builder: %v", err)
	}
	t.Log("Type builder created successfully")

	// Test basic string type creation
	t.Log("Attempting to create string type...")
	stringType, err := typeBuilder.String()
	if err != nil {
		t.Fatalf("Failed to create string type: %v", err)
	}
	t.Log("String type created successfully")

	// Test the String() method
	repr := stringType.Print()
	t.Logf("String type representation: %s", repr)
}