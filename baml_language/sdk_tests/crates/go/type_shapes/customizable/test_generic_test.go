package sdk_test

import (
	"context"
	"testing"

	"baml.local/sdk/baml_sdk"
)

func Test_generic_wrapper_get_value(t *testing.T) {
	wrapper, err := baml_sdk.GenericsMakeWrapperMethods(context.Background(), "hello")
	if err != nil {
		t.Fatal(err)
	}
	got, err := wrapper.GetValue(context.Background())
	if err != nil || got != "hello" {
		t.Fatalf("GetValue() = %q, %v", got, err)
	}
}

// GetValueOrMarker remains deferred: its T | WrapperMarker result is emitted as
// a dynamic Go union, and decoding the class-level TypeVar arm is not supported.
