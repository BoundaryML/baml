package sdk_test

import (
	"baml.local/sdk/baml_sdk"
	"context"
	"reflect"
	"testing"
)

func TestRoundTripInts(t *testing.T) {
	want := []int64{1, 2, 3}
	got, err := baml_sdk.ListsRoundTripInts(context.Background(), want)
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("got %#v, %v", got, err)
	}
}
func TestRoundTripEmptyList(t *testing.T) {
	want := []int64{}
	got, err := baml_sdk.ListsRoundTripInts(context.Background(), want)
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("got %#v, %v", got, err)
	}
}
func TestRoundTripOptionalStrings(t *testing.T) {
	a, b := "a", "b"
	want := []*string{&a, nil, &b}
	got, err := baml_sdk.ListsRoundTripOptionalStrings(context.Background(), want)
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("got %#v, %v", got, err)
	}
}

// Python test_round_trip_union_list and test_round_trip_list_container are deferred with general unions.
