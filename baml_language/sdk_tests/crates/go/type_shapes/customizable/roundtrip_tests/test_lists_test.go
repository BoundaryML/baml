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

func TestRoundTripUnionList(t *testing.T) {
	want := []baml_sdk.StringOrInt{
		baml_sdk.NewStringOrIntFromInt(1),
		baml_sdk.NewStringOrIntFromString("two"),
		baml_sdk.NewStringOrIntFromInt(3),
	}
	got, err := baml_sdk.ListsRoundTripUnionList(context.Background(), want)
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("got %#v, %v", got, err)
	}
}

func TestRoundTripListContainer(t *testing.T) {
	z := "z"
	want := baml_sdk.ListsListContainer{
		Ints:            []int64{1, 2},
		OptionalStrings: []*string{nil, &z},
		UnionList: []baml_sdk.StringOrInt{
			baml_sdk.NewStringOrIntFromInt(1),
			baml_sdk.NewStringOrIntFromString("x"),
		},
	}
	got, err := baml_sdk.ListsRoundTripListContainer(context.Background(), want)
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("got %#v, %v", got, err)
	}
}
