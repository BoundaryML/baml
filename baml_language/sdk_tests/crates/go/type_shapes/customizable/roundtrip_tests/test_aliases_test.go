package sdk_test

import (
	"context"
	"reflect"
	"testing"

	"baml.local/sdk/baml_sdk"
)

// Supported subset of the direct Python test_aliases.py port. Recursive aliases
// and their containing class remain explicitly deferred with general unions.
func Test_round_trip_string_list(t *testing.T) {
	want := baml_sdk.AliasesStringList{"a", "b"}
	got, err := baml_sdk.AliasesRoundTripStringList(context.Background(), want)
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("string list = %#v, %v, want %#v", got, err, want)
	}
}

// Python recursive-alias tests are deferred with recursive unions.
