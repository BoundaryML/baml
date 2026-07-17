package sdk_test

import (
	"context"
	"testing"

	"baml.local/sdk/baml_sdk"
)

// Supported subset of the direct Python test_streams.py port. General unions,
// generic Box, and handle-backed HTTP responses remain deferred.
func TestRoundTripResumeStream(t *testing.T) {
	resume := baml_sdk.LoremResumeStream{}
	name := "ada"
	resume.Name = &name
	if got, err := baml_sdk.LoremRoundTripResumeStream(context.Background(), resume); err != nil || got.Name == nil || *got.Name != name || got.Email != nil {
		t.Fatalf("resume stream = %#v, %v", got, err)
	}
}

func TestRoundTripRootFooStream(t *testing.T) {
	value := int64(3)
	foo := baml_sdk.FooStream{V: &value}
	if got, err := baml_sdk.LoremRoundTripRootFooStream(context.Background(), foo); err != nil || got.V == nil || *got.V != value {
		t.Fatalf("foo stream = %#v, %v", got, err)
	}
}

// Remaining Python stream tests are deferred with generics, unions, and handles.
