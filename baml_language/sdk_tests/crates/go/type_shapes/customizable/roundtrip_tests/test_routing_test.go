package sdk_test

import (
	"baml.local/sdk/baml_sdk"
	"context"
	"reflect"
	"testing"
)

func TestMakeFoo(t *testing.T) {
	got, err := baml_sdk.MakeFoo(context.Background(), 3)
	if err != nil || got.V != 3 {
		t.Fatalf("got %#v, %v", got, err)
	}
}
func TestRoundTripFoo(t *testing.T) {
	want := baml_sdk.Foo{V: 10}
	got, err := baml_sdk.RoundTripFoo(context.Background(), want)
	if err != nil || got != want {
		t.Fatalf("got %#v, %v", got, err)
	}
}
func TestRoundTripThingFromAB(t *testing.T) {
	want := baml_sdk.ABThing{V: 1}
	got, err := baml_sdk.ABRoundTripThingFromAb(context.Background(), want)
	if err != nil || got != want {
		t.Fatalf("got %#v, %v", got, err)
	}
}
func TestRoundTripRootFooFromAB(t *testing.T) {
	want := baml_sdk.Foo{V: 2}
	got, err := baml_sdk.ABRoundTripRootFooFromAb(context.Background(), want)
	if err != nil || got != want {
		t.Fatalf("got %#v, %v", got, err)
	}
}
func TestRoundTripDeepThingFromA(t *testing.T) {
	want := baml_sdk.ABThing{V: 4}
	got, err := baml_sdk.ARoundTripDeepThingFromA(context.Background(), want)
	if err != nil || got != want {
		t.Fatalf("got %#v, %v", got, err)
	}
}
func TestRoundTripDeepThingFromLorem(t *testing.T) {
	want := baml_sdk.ABThing{V: 5}
	got, err := baml_sdk.LoremRoundTripDeepThingFromLorem(context.Background(), want)
	if err != nil || got != want {
		t.Fatalf("got %#v, %v", got, err)
	}
}
func TestRoundTripLoremResume(t *testing.T) {
	want := baml_sdk.LoremResume{Name: "ada"}
	got, err := baml_sdk.LoremRoundTripResume(context.Background(), want)
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("got %#v, %v", got, err)
	}
}
func TestRoundTripRootFooFromLorem(t *testing.T) {
	want := baml_sdk.Foo{V: 6}
	got, err := baml_sdk.LoremRoundTripRootFoo(context.Background(), want)
	if err != nil || got != want {
		t.Fatalf("got %#v, %v", got, err)
	}
}
func TestRoundTripLoremResumeFromIpsum(t *testing.T) {
	email := "g@x.com"
	want := baml_sdk.LoremResume{Name: "grace", Email: &email}
	got, err := baml_sdk.IpsumRoundTripLoremResumeFromIpsum(context.Background(), want)
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("got %#v, %v", got, err)
	}
}
