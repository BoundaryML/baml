package main

import (
	"context"
	"fmt"
	b "literal_types/baml_client"
	"os"
)

func main() {
	ctx := context.Background()

	// Test string literals
	fmt.Println("Testing StringLiterals...")
	stringResult, err := b.TestStringLiterals(ctx, "test string literals")
	if err != nil {
		fmt.Printf("Error testing string literals: %v\n", err)
		os.Exit(1)
	}

	// Verify string literal values
	if !stringResult.Status.IsKactive() {
		fmt.Printf("Expected status to be 'active', got '%s'\n", stringResult.Status.Kactive())
		os.Exit(1)
	}
	if !stringResult.Environment.IsKprod() {
		fmt.Printf("Expected environment to be 'prod', got '%s'\n", stringResult.Environment.Kprod())
		os.Exit(1)
	}
	if !stringResult.Method.IsKPOST() {
		fmt.Printf("Expected method to be 'POST', got '%s'\n", stringResult.Method.KPOST())
		os.Exit(1)
	}
	fmt.Println("✓ StringLiterals test passed")

	// Test integer literals
	fmt.Println("\nTesting IntegerLiterals...")
	intResult, err := b.TestIntegerLiterals(ctx, "test integer literals")
	if err != nil {
		fmt.Printf("Error testing integer literals: %v\n", err)
		os.Exit(1)
	}

	// Verify integer literal values
	if !intResult.Priority.IsIntK3() {
		fmt.Printf("Expected priority to be 3, got %d\n", intResult.Priority.IntK3())
		os.Exit(1)
	}
	if !intResult.HttpStatus.IsIntK201() {
		fmt.Printf("Expected httpStatus to be 201, got %d\n", intResult.HttpStatus.IntK201())
		os.Exit(1)
	}
	if !intResult.MaxRetries.IsIntK3() {
		fmt.Printf("Expected maxRetries to be 3, got %d\n", intResult.MaxRetries.IntK3())
		os.Exit(1)
	}
	fmt.Println("✓ IntegerLiterals test passed")

	// Test boolean literals
	fmt.Println("\nTesting BooleanLiterals...")
	boolResult, err := b.TestBooleanLiterals(ctx, "test boolean literals")
	if err != nil {
		fmt.Printf("Error testing boolean literals: %v\n", err)
		os.Exit(1)
	}

	// Verify boolean literal values
	if !boolResult.AlwaysTrue {
		fmt.Printf("Expected alwaysTrue to be true, got false\n")
		os.Exit(1)
	}
	if boolResult.AlwaysFalse {
		fmt.Printf("Expected alwaysFalse to be false, got true\n")
		os.Exit(1)
	}
	if !boolResult.EitherBool.IsBoolKTrue() {
		fmt.Printf("Expected eitherBool to be true, got false\n")
		os.Exit(1)
	}
	fmt.Println("✓ BooleanLiterals test passed")

	// Test mixed literals
	fmt.Println("\nTesting MixedLiterals...")
	mixedResult, err := b.TestMixedLiterals(ctx, "test mixed literals")
	if err != nil {
		fmt.Printf("Error testing mixed literals: %v\n", err)
		os.Exit(1)
	}

	// Verify mixed literal values
	if mixedResult.Id != 12345 {
		fmt.Printf("Expected id to be 12345, got %d\n", mixedResult.Id)
		os.Exit(1)
	}
	if !mixedResult.Type.IsKadmin() {
		fmt.Printf("Expected type to be 'admin', got '%s'\n", mixedResult.Type.Kadmin())
		os.Exit(1)
	}
	if !mixedResult.Level.IsIntK2() {
		fmt.Printf("Expected level to be 2, got %d\n", mixedResult.Level.IntK2())
		os.Exit(1)
	}
	if !mixedResult.IsActive.IsBoolKTrue() {
		fmt.Printf("Expected isActive to be true, got false\n")
		os.Exit(1)
	}
	if !mixedResult.ApiVersion.IsKv2() {
		fmt.Printf("Expected apiVersion to be 'v2', got '%s'\n", mixedResult.ApiVersion.Kv2())
		os.Exit(1)
	}
	fmt.Println("✓ MixedLiterals test passed")

	// Test complex literals
	fmt.Println("\nTesting ComplexLiterals...")
	complexResult, err := b.TestComplexLiterals(ctx, "test complex literals")
	if err != nil {
		fmt.Printf("Error testing complex literals: %v\n", err)
		os.Exit(1)
	}

	// Verify complex literal values
	if !complexResult.State.IsKpublished() {
		fmt.Printf("Expected state to be 'published', got '%s'\n", complexResult.State.Kpublished())
		os.Exit(1)
	}
	if !complexResult.RetryCount.IsIntK5() {
		fmt.Printf("Expected retryCount to be 5, got %d\n", complexResult.RetryCount.IntK5())
		os.Exit(1)
	}
	if !complexResult.Response.IsKsuccess() {
		fmt.Printf("Expected response to be 'success', got '%s'\n", complexResult.Response.Ksuccess())
		os.Exit(1)
	}
	if len(complexResult.Flags) != 3 {
		fmt.Printf("Expected flags length 3, got %d\n", len(complexResult.Flags))
		os.Exit(1)
	}
	if len(complexResult.Codes) != 3 {
		fmt.Printf("Expected codes length 3, got %d\n", len(complexResult.Codes))
		os.Exit(1)
	}
	fmt.Println("✓ ComplexLiterals test passed")

	fmt.Println("\n✅ All literal type tests passed!")
}
