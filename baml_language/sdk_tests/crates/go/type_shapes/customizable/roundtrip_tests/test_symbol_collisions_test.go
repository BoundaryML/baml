package sdk_test

import (
	"baml.local/sdk/baml_sdk"
	"context"
	"testing"
)

func makeCollisionBars(t *testing.T) (baml_sdk.SymbolCollisionsFooBar, baml_sdk.SymbolCollisionsFizzFooBar, baml_sdk.SymbolCollisionsFizzBuzzFooBar) {
	t.Helper()
	ctx := context.Background()
	a, err := baml_sdk.SymbolCollisionsFooMakeFooBar(ctx, "a", 1)
	if err != nil {
		t.Fatal(err)
	}
	b, err := baml_sdk.SymbolCollisionsFizzFooMakeFizzFooBar(ctx, "b", 2)
	if err != nil {
		t.Fatal(err)
	}
	c, err := baml_sdk.SymbolCollisionsFizzBuzzFooMakeFizzBuzzFooBar(ctx, "c", 3, false)
	if err != nil {
		t.Fatal(err)
	}
	return a, b, c
}

func Test_round_trip_foo_bar(t *testing.T) {
	got, err := baml_sdk.SymbolCollisionsFooMakeFooBar(context.Background(), "hi", 2)
	if err != nil {
		t.Fatal(err)
	}
	round, err := baml_sdk.SymbolCollisionsFooRoundTripFooBar(context.Background(), got)
	if err != nil || round != got {
		t.Fatalf("got %#v, %v", round, err)
	}
}
func Test_round_trip_fizz_foo_bar(t *testing.T) {
	got, err := baml_sdk.SymbolCollisionsFizzFooMakeFizzFooBar(context.Background(), "t", 1.5)
	if err != nil {
		t.Fatal(err)
	}
	round, err := baml_sdk.SymbolCollisionsFizzFooRoundTripFizzFooBar(context.Background(), got)
	if err != nil || round != got {
		t.Fatalf("got %#v, %v", round, err)
	}
}
func Test_round_trip_fizz_buzz_foo_bar(t *testing.T) {
	got, err := baml_sdk.SymbolCollisionsFizzBuzzFooMakeFizzBuzzFooBar(context.Background(), "f", 2.5, true)
	if err != nil {
		t.Fatal(err)
	}
	round, err := baml_sdk.SymbolCollisionsFizzBuzzFooRoundTripFizzBuzzFooBar(context.Background(), got)
	if err != nil || round != got {
		t.Fatalf("got %#v, %v", round, err)
	}
}
func Test_round_trip_ipsum(t *testing.T) {
	a, b, c := makeCollisionBars(t)
	want, err := baml_sdk.SymbolCollisionsLoremMakeIpsum(context.Background(), a, b, c)
	if err != nil {
		t.Fatal(err)
	}
	got, err := baml_sdk.SymbolCollisionsLoremRoundTripIpsum(context.Background(), want)
	if err != nil || got != want {
		t.Fatalf("got %#v, %v", got, err)
	}
}
func Test_round_trip_deep(t *testing.T) {
	a, b, c := makeCollisionBars(t)
	nested, err := baml_sdk.SymbolCollisionsLoremMakeIpsum(context.Background(), a, b, c)
	if err != nil {
		t.Fatal(err)
	}
	want, err := baml_sdk.SymbolCollisionsABCDMakeDeep(context.Background(), a, b, c, nested)
	if err != nil {
		t.Fatal(err)
	}
	got, err := baml_sdk.SymbolCollisionsABCDRoundTripDeep(context.Background(), want)
	if err != nil || got != want {
		t.Fatalf("got %#v, %v", got, err)
	}
}
