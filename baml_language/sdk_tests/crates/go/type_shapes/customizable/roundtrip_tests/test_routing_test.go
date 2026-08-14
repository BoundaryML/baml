package sdk_test

import (
	"baml.local/sdk/baml_sdk"
	"context"
	"reflect"
	"testing"
)

func Test_make_foo(t *testing.T) {
	got, err := baml_sdk.MakeFoo(context.Background(), 3)
	if err != nil || got.V != 3 {
		t.Fatalf("got %#v, %v", got, err)
	}
}
func Test_round_trip_foo(t *testing.T) {
	want := baml_sdk.Foo{V: 10}
	got, err := baml_sdk.RoundTripFoo(context.Background(), want)
	if err != nil || got != want {
		t.Fatalf("got %#v, %v", got, err)
	}
}
func Test_round_trip_thing_from_ab(t *testing.T) {
	want := baml_sdk.ABThing{V: 1}
	got, err := baml_sdk.ABRoundTripThingFromAb(context.Background(), want)
	if err != nil || got != want {
		t.Fatalf("got %#v, %v", got, err)
	}
}
func Test_round_trip_root_foo_from_ab(t *testing.T) {
	want := baml_sdk.Foo{V: 2}
	got, err := baml_sdk.ABRoundTripRootFooFromAb(context.Background(), want)
	if err != nil || got != want {
		t.Fatalf("got %#v, %v", got, err)
	}
}
func Test_round_trip_deep_thing_from_a(t *testing.T) {
	want := baml_sdk.ABThing{V: 4}
	got, err := baml_sdk.ARoundTripDeepThingFromA(context.Background(), want)
	if err != nil || got != want {
		t.Fatalf("got %#v, %v", got, err)
	}
}
func Test_round_trip_deep_thing_from_lorem(t *testing.T) {
	want := baml_sdk.ABThing{V: 5}
	got, err := baml_sdk.LoremRoundTripDeepThingFromLorem(context.Background(), want)
	if err != nil || got != want {
		t.Fatalf("got %#v, %v", got, err)
	}
}
func Test_round_trip_lorem_resume(t *testing.T) {
	want := baml_sdk.LoremResume{Name: "ada"}
	got, err := baml_sdk.LoremRoundTripResume(context.Background(), want)
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("got %#v, %v", got, err)
	}
}
func Test_round_trip_root_foo_from_lorem(t *testing.T) {
	want := baml_sdk.Foo{V: 6}
	got, err := baml_sdk.LoremRoundTripRootFoo(context.Background(), want)
	if err != nil || got != want {
		t.Fatalf("got %#v, %v", got, err)
	}
}
func Test_round_trip_lorem_resume_from_ipsum(t *testing.T) {
	email := "g@x.com"
	want := baml_sdk.LoremResume{Name: "grace", Email: &email}
	got, err := baml_sdk.IpsumRoundTripLoremResumeFromIpsum(context.Background(), want)
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("got %#v, %v", got, err)
	}
}
