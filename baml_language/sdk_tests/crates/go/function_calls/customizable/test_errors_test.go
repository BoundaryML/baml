package sdk_test

import (
	"baml.local/sdk/baml_sdk"
	"context"
	"strings"
	"testing"
)

// Supported semantic boundary of Python test_str_is_non_empty.
func TestErrorStringIsNonEmpty(t *testing.T) {
	_, err := baml_sdk.ThrowsTestThrowMyError(context.Background())
	if err == nil || err.Error() == "" {
		t.Fatalf("error = %v", err)
	}
}

// Supported semantic boundary of Python test_baml_error_carries_baml_trace.
func TestBAMLErrorCarriesBAMLTrace(t *testing.T) {
	_, err := baml_sdk.ThrowsTestThrowMyError(context.Background())
	if err == nil {
		t.Fatal("ThrowMyError returned no error")
	}
	if !strings.Contains(err.Error(), "user.throws_test.ThrowMyError") || !strings.Contains(err.Error(), "types.baml") {
		t.Fatalf("error missing BAML trace: %v", err)
	}
}

// Typed values, class_name, traceback splicing, panics, and process exit are deferred.
