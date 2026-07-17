package sdk_test

import (
	"context"
	"testing"

	"baml.local/sdk/baml_sdk"
)

// Supported subset of the direct Python test_forward_refs.py port. Recursive
// aliases and generic classes are deferred; the uninhabitable required
// self-reference remains compile-only in both languages.
func TestRoundTripForwardRefOther(t *testing.T) {
	want := baml_sdk.ForwardRefsOther{V: 7}
	got, err := baml_sdk.ForwardRefsRoundTripOther(context.Background(), want)
	if err != nil || got != want {
		t.Fatalf("other = %#v, %v", got, err)
	}
}

var _ = baml_sdk.ForwardRefsNode{}

// Python recursive-alias and generic forward-reference tests are deferred.
