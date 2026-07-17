package sdk_test

import (
	"context"
	"testing"

	"baml.local/sdk/baml_sdk"
)

// Supported subset of the direct Python test_optional.py port. The optional
// general-union function and its containing class are deferred.
func TestRoundTripOptionalInt(t *testing.T) {
	ctx := context.Background()
	value := int64(5)
	if got, err := baml_sdk.OptionalRoundTripOptionalInt(ctx, &value); err != nil || got == nil || *got != value {
		t.Fatalf("present int = %#v, %v", got, err)
	}
	if got, err := baml_sdk.OptionalRoundTripOptionalInt(ctx, nil); err != nil || got != nil {
		t.Fatalf("null int = %#v, %v", got, err)
	}
}

func TestRoundTripOptionalResume(t *testing.T) {
	ctx := context.Background()
	resume := baml_sdk.OptionalResume{Name: "ada"}
	if got, err := baml_sdk.OptionalRoundTripOptionalResume(ctx, &resume); err != nil || got == nil || *got != resume {
		t.Fatalf("present resume = %#v, %v", got, err)
	}
	if got, err := baml_sdk.OptionalRoundTripOptionalResume(ctx, nil); err != nil || got != nil {
		t.Fatalf("null resume = %#v, %v", got, err)
	}
}

func TestRoundTripRequiredResume(t *testing.T) {
	want := baml_sdk.OptionalResume{Name: "grace"}
	got, err := baml_sdk.OptionalRoundTripResume(context.Background(), want)
	if err != nil || got != want {
		t.Fatalf("got %#v, %v", got, err)
	}
}

// Python optional-union and optional-container tests are deferred with general unions.
