package sdk_test

import (
	"context"
	"testing"

	"baml.local/sdk/baml_sdk"
)

// Supported subset of the direct Python test_unions.py port. These two cases
// normalize to ordinary Go types; multi-arm unions remain deferred.
func TestRoundTripSingletonUnwrap(t *testing.T) {
	if got, err := baml_sdk.UnionsRoundTripSingletonUnwrap(context.Background(), 7); err != nil || got != 7 {
		t.Fatalf("singleton union = %v, %v", got, err)
	}
}

func TestRoundTripUnionT(t *testing.T) {
	want := baml_sdk.UnionsT{V: 4}
	if got, err := baml_sdk.UnionsRoundTripT(context.Background(), want); err != nil || got != want {
		t.Fatalf("T = %#v, %v", got, err)
	}
}

// Remaining Python union tests are deferred with general unions.
