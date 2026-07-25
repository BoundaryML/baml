package sdk_test

import (
	"context"
	"reflect"
	"testing"

	"baml.local/sdk/baml_sdk"
)

// Supported subset of the direct Python test_forward_refs.py port. Recursive
// aliases are deferred; the uninhabitable required self-reference remains
// compile-only in both languages.
func Test_round_trip_forward_ref_other(t *testing.T) {
	want := baml_sdk.ForwardRefsOther{V: 7}
	got, err := baml_sdk.ForwardRefsRoundTripOther(context.Background(), want)
	if err != nil || got != want {
		t.Fatalf("other = %#v, %v", got, err)
	}
}

var _ = baml_sdk.ForwardRefsNode{}

func Test_round_trip_forward_ref_g_node_int(t *testing.T) {
	want := baml_sdk.ForwardRefsGNode[int64]{
		Children: []baml_sdk.ForwardRefsGNode[int64]{{Children: []baml_sdk.ForwardRefsGNode[int64]{}}},
	}
	got, err := baml_sdk.ForwardRefsRoundTripGNodeInt(context.Background(), want)
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("generic node = %#v, %v, want %#v", got, err, want)
	}
}

// Python recursive-alias tests remain deferred with recursive unions.
