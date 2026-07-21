package sdk_test

import (
	"baml.local/sdk/baml_sdk"
	"context"
	"testing"
)

func Test_make_outer(t *testing.T) {
	got, err := baml_sdk.ClassRefsMakeOuter(context.Background(), 5)
	if err != nil || got.Inner.Value != 5 {
		t.Fatalf("got %#v, %v", got, err)
	}
}
func Test_round_trip_inner(t *testing.T) {
	want := baml_sdk.ClassRefsInner{Value: 3}
	got, err := baml_sdk.ClassRefsRoundTripInner(context.Background(), want)
	if err != nil || got != want {
		t.Fatalf("got %#v, %v", got, err)
	}
}
func Test_round_trip_outer(t *testing.T) {
	want := baml_sdk.ClassRefsOuter{Inner: baml_sdk.ClassRefsInner{Value: 9}}
	got, err := baml_sdk.ClassRefsRoundTripOuter(context.Background(), want)
	if err != nil || got != want {
		t.Fatalf("got %#v, %v", got, err)
	}
}
