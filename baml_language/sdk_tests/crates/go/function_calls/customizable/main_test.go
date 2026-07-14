package sdk_test

import (
	"context"
	"testing"

	"baml.local/sdk/baml_sdk"
)

func TestScalarFunctions(t *testing.T) {
	ctx := context.Background()

	hello, err := baml_sdk.HelloWorld(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if hello != "hello world" {
		t.Fatalf("HelloWorld() = %q, want %q", hello, "hello world")
	}

	echo, err := baml_sdk.SingleRequiredArg(ctx, "hi")
	if err != nil {
		t.Fatal(err)
	}
	if echo != "hi" {
		t.Fatalf("SingleRequiredArg() = %q, want %q", echo, "hi")
	}
}

func TestPersonRoundTrip(t *testing.T) {
	want := baml_sdk.Person{Person: "record", Name: "Ada", Age: 37}
	got, err := baml_sdk.RoundTripPerson(context.Background(), want)
	if err != nil {
		t.Fatal(err)
	}
	if got != want {
		t.Fatalf("RoundTripPerson() = %#v, want %#v", got, want)
	}
}
