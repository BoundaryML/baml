package main

import (
	"context"
	b "literal_types/baml_client"
	"testing"
)

func TestStringLiterals(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestStringLiterals(ctx, "test string literals")
	if err != nil {
		t.Fatalf("Error testing string literals: %v", err)
	}

	// Verify string literal values
	if !result.Status.IsKactive() {
		t.Errorf("Expected status to be 'active', got '%v'", result.Status)
	}
	if !result.Environment.IsKprod() {
		t.Errorf("Expected environment to be 'prod', got '%v'", result.Environment)
	}
	if !result.Method.IsKPOST() {
		t.Errorf("Expected method to be 'POST', got '%v'", result.Method)
	}
}

func TestIntegerLiterals(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestIntegerLiterals(ctx, "test integer literals")
	if err != nil {
		t.Fatalf("Error testing integer literals: %v", err)
	}

	// Verify integer literal values
	if !result.Priority.IsIntK3() {
		t.Errorf("Expected priority to be 3")
	}
	if !result.HttpStatus.IsIntK201() {
		t.Errorf("Expected httpStatus to be 201")
	}
	if !result.MaxRetries.IsIntK3() {
		t.Errorf("Expected maxRetries to be 3")
	}
}

func TestBooleanLiterals(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestBooleanLiterals(ctx, "test boolean literals")
	if err != nil {
		t.Fatalf("Error testing boolean literals: %v", err)
	}

	// Verify boolean literal values
	if !result.AlwaysTrue {
		t.Errorf("Expected alwaysTrue to be true, got false")
	}
	if result.AlwaysFalse {
		t.Errorf("Expected alwaysFalse to be false, got true")
	}
	if !result.EitherBool.IsBoolKTrue() {
		t.Errorf("Expected eitherBool to be true, got false")
	}
}

func TestMixedLiterals(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestMixedLiterals(ctx, "test mixed literals")
	if err != nil {
		t.Fatalf("Error testing mixed literals: %v", err)
	}

	// Verify mixed literal values
	if result.Id != 12345 {
		t.Errorf("Expected id to be 12345, got %d", result.Id)
	}
	if !result.Type.IsKadmin() {
		t.Errorf("Expected type to be 'admin'")
	}
	if !result.Level.IsIntK2() {
		t.Errorf("Expected level to be 2")
	}
	if !result.IsActive.IsBoolKTrue() {
		t.Errorf("Expected isActive to be true, got false")
	}
	if !result.ApiVersion.IsKv2() {
		t.Errorf("Expected apiVersion to be 'v2'")
	}
}

func TestComplexLiterals(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestComplexLiterals(ctx, "test complex literals")
	if err != nil {
		t.Fatalf("Error testing complex literals: %v", err)
	}

	// Verify complex literal values
	if !result.State.IsKpublished() {
		t.Errorf("Expected state to be 'published'")
	}
	if !result.RetryCount.IsIntK5() {
		t.Errorf("Expected retryCount to be 5")
	}
	if !result.Response.IsKsuccess() {
		t.Errorf("Expected response to be 'success'")
	}
	if len(result.Flags) != 3 {
		t.Errorf("Expected flags length 3, got %d", len(result.Flags))
	}
	if len(result.Codes) != 3 {
		t.Errorf("Expected codes length 3, got %d", len(result.Codes))
	}
}
