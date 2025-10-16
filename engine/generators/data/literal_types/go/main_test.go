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

func TestStringLiteralsStream(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	channel, err := b.Stream.TestStringLiterals(ctx, "test string literals stream")
	if err != nil {
		t.Fatalf("Error starting TestStringLiterals stream: %v", err)
	}

	gotFinal := false
	streamCount := 0

	for result := range channel {
		if result.IsError {
			t.Fatalf("Error in stream: %v", result.Error)
		} else if result.IsFinal {
			gotFinal = true
			final := result.Final()
			if final == nil {
				t.Errorf("Expected non-nil final result")
			} else {
				// Verify string literal values
				if !final.Status.IsKactive() {
					t.Errorf("Expected status to be 'active', got '%v'", final.Status)
				}
				if !final.Environment.IsKprod() {
					t.Errorf("Expected environment to be 'prod', got '%v'", final.Environment)
				}
				if !final.Method.IsKPOST() {
					t.Errorf("Expected method to be 'POST', got '%v'", final.Method)
				}
			}
		} else {
			streamCount++
			stream := result.Stream()
			if stream == nil {
				t.Errorf("Expected non-nil stream result")
			}
		}
	}

	if !gotFinal {
		t.Errorf("Expected to receive a final result from stream")
	}

	t.Logf("Received %d stream chunks before final result", streamCount)
}

func TestIntegerLiteralsStream(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	channel, err := b.Stream.TestIntegerLiterals(ctx, "test integer literals stream")
	if err != nil {
		t.Fatalf("Error starting TestIntegerLiterals stream: %v", err)
	}

	gotFinal := false
	streamCount := 0

	for result := range channel {
		if result.IsError {
			t.Fatalf("Error in stream: %v", result.Error)
		} else if result.IsFinal {
			gotFinal = true
			final := result.Final()
			if final == nil {
				t.Errorf("Expected non-nil final result")
			} else {
				// Verify integer literal values
				if !final.Priority.IsIntK3() {
					t.Errorf("Expected priority to be 3")
				}
				if !final.HttpStatus.IsIntK201() {
					t.Errorf("Expected httpStatus to be 201")
				}
				if !final.MaxRetries.IsIntK3() {
					t.Errorf("Expected maxRetries to be 3")
				}
			}
		} else {
			streamCount++
			stream := result.Stream()
			if stream == nil {
				t.Errorf("Expected non-nil stream result")
			}
		}
	}

	if !gotFinal {
		t.Errorf("Expected to receive a final result from stream")
	}

	t.Logf("Received %d stream chunks before final result", streamCount)
}

func TestBooleanLiteralsStream(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	channel, err := b.Stream.TestBooleanLiterals(ctx, "test boolean literals stream")
	if err != nil {
		t.Fatalf("Error starting TestBooleanLiterals stream: %v", err)
	}

	gotFinal := false
	streamCount := 0

	for result := range channel {
		if result.IsError {
			t.Fatalf("Error in stream: %v", result.Error)
		} else if result.IsFinal {
			gotFinal = true
			final := result.Final()
			if final == nil {
				t.Errorf("Expected non-nil final result")
			} else {
				// Verify boolean literal values
				if !final.AlwaysTrue {
					t.Errorf("Expected alwaysTrue to be true, got false")
				}
				if final.AlwaysFalse {
					t.Errorf("Expected alwaysFalse to be false, got true")
				}
				if !final.EitherBool.IsBoolKTrue() {
					t.Errorf("Expected eitherBool to be true, got false")
				}
			}
		} else {
			streamCount++
			stream := result.Stream()
			if stream == nil {
				t.Errorf("Expected non-nil stream result")
			}
		}
	}

	if !gotFinal {
		t.Errorf("Expected to receive a final result from stream")
	}

	t.Logf("Received %d stream chunks before final result", streamCount)
}

func TestMixedLiteralsStream(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	channel, err := b.Stream.TestMixedLiterals(ctx, "test mixed literals stream")
	if err != nil {
		t.Fatalf("Error starting TestMixedLiterals stream: %v", err)
	}

	gotFinal := false
	streamCount := 0

	for result := range channel {
		if result.IsError {
			t.Fatalf("Error in stream: %v", result.Error)
		} else if result.IsFinal {
			gotFinal = true
			final := result.Final()
			if final == nil {
				t.Errorf("Expected non-nil final result")
			} else {
				// Verify mixed literal values
				if final.Id != 12345 {
					t.Errorf("Expected id to be 12345, got %d", final.Id)
				}
				if !final.Type.IsKadmin() {
					t.Errorf("Expected type to be 'admin'")
				}
				if !final.Level.IsIntK2() {
					t.Errorf("Expected level to be 2")
				}
				if !final.IsActive.IsBoolKTrue() {
					t.Errorf("Expected isActive to be true, got false")
				}
				if !final.ApiVersion.IsKv2() {
					t.Errorf("Expected apiVersion to be 'v2'")
				}
			}
		} else {
			streamCount++
			stream := result.Stream()
			if stream == nil {
				t.Errorf("Expected non-nil stream result")
			}
		}
	}

	if !gotFinal {
		t.Errorf("Expected to receive a final result from stream")
	}

	t.Logf("Received %d stream chunks before final result", streamCount)
}

func TestComplexLiteralsStream(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	channel, err := b.Stream.TestComplexLiterals(ctx, "test complex literals stream")
	if err != nil {
		t.Fatalf("Error starting TestComplexLiterals stream: %v", err)
	}

	gotFinal := false
	streamCount := 0

	for result := range channel {
		if result.IsError {
			t.Fatalf("Error in stream: %v", result.Error)
		} else if result.IsFinal {
			gotFinal = true
			final := result.Final()
			if final == nil {
				t.Errorf("Expected non-nil final result")
			} else {
				// Verify complex literal values
				if !final.State.IsKpublished() {
					t.Errorf("Expected state to be 'published'")
				}
				if !final.RetryCount.IsIntK5() {
					t.Errorf("Expected retryCount to be 5")
				}
				if !final.Response.IsKsuccess() {
					t.Errorf("Expected response to be 'success'")
				}
				if len(final.Flags) != 3 {
					t.Errorf("Expected flags length 3, got %d", len(final.Flags))
				}
				if len(final.Codes) != 3 {
					t.Errorf("Expected codes length 3, got %d", len(final.Codes))
				}
			}
		} else {
			streamCount++
			stream := result.Stream()
			if stream == nil {
				t.Errorf("Expected non-nil stream result")
			}
		}
	}

	if !gotFinal {
		t.Errorf("Expected to receive a final result from stream")
	}

	t.Logf("Received %d stream chunks before final result", streamCount)
}
