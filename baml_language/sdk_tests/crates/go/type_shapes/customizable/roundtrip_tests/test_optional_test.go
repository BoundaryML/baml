package sdk_test

import (
	"context"
	"testing"

	"baml.local/sdk/baml_sdk"
)

func Test_round_trip_optional_int(t *testing.T) {
	ctx := context.Background()
	value := int64(5)
	if got, err := baml_sdk.OptionalRoundTripOptionalInt(ctx, &value); err != nil || got == nil || *got != value {
		t.Fatalf("present int = %#v, %v", got, err)
	}
	if got, err := baml_sdk.OptionalRoundTripOptionalInt(ctx, nil); err != nil || got != nil {
		t.Fatalf("null int = %#v, %v", got, err)
	}
}

func Test_round_trip_optional_union(t *testing.T) {
	ctx := context.Background()
	integer := baml_sdk.NewStringOrIntFromInt(3)
	if got, err := baml_sdk.OptionalRoundTripOptionalUnion(ctx, &integer); err != nil || got == nil {
		t.Fatalf("integer = %#v, %v", got, err)
	} else if value, ok := got.AsInt(); !ok || value != 3 {
		t.Fatalf("integer arm = %v, %v", value, ok)
	}
	text := baml_sdk.NewStringOrIntFromString("s")
	if got, err := baml_sdk.OptionalRoundTripOptionalUnion(ctx, &text); err != nil || got == nil {
		t.Fatalf("string = %#v, %v", got, err)
	} else if value, ok := got.AsString(); !ok || value != "s" {
		t.Fatalf("string arm = %q, %v", value, ok)
	}
	if got, err := baml_sdk.OptionalRoundTripOptionalUnion(ctx, nil); err != nil || got != nil {
		t.Fatalf("null = %#v, %v", got, err)
	}
}

func Test_round_trip_optional_resume(t *testing.T) {
	ctx := context.Background()
	resume := baml_sdk.OptionalResume{Name: "ada"}
	if got, err := baml_sdk.OptionalRoundTripOptionalResume(ctx, &resume); err != nil || got == nil || *got != resume {
		t.Fatalf("present resume = %#v, %v", got, err)
	}
	if got, err := baml_sdk.OptionalRoundTripOptionalResume(ctx, nil); err != nil || got != nil {
		t.Fatalf("null resume = %#v, %v", got, err)
	}
}

func Test_round_trip_required_resume(t *testing.T) {
	want := baml_sdk.OptionalResume{Name: "grace"}
	got, err := baml_sdk.OptionalRoundTripResume(context.Background(), want)
	if err != nil || got != want {
		t.Fatalf("got %#v, %v", got, err)
	}
}

func Test_round_trip_optional_container(t *testing.T) {
	resume := baml_sdk.OptionalResume{Name: "x"}
	union := baml_sdk.NewStringOrIntFromString("y")
	want := baml_sdk.OptionalOptionalContainer{
		OptionalInt:   nil,
		OptionalClass: &resume,
		OptionalUnion: &union,
	}
	got, err := baml_sdk.OptionalRoundTripOptionalContainer(context.Background(), want)
	if err != nil || got.OptionalInt != nil || got.OptionalClass == nil || *got.OptionalClass != resume || got.OptionalUnion == nil {
		t.Fatalf("got %#v, %v", got, err)
	}
	if value, ok := got.OptionalUnion.AsString(); !ok || value != "y" {
		t.Fatalf("union arm = %q, %v", value, ok)
	}
}
