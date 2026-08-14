package sdk_test

import (
	"context"
	"testing"

	"baml.local/sdk/baml_sdk"
)

func Test_round_trip_null_to_end(t *testing.T) {
	ctx := context.Background()
	integer := baml_sdk.NewStringOrIntFromInt(1)
	gotInteger, err := baml_sdk.UnionsRoundTripNullToEnd(ctx, &integer)
	if err != nil || gotInteger == nil {
		t.Fatalf("integer union = %#v, %v", gotInteger, err)
	}
	if value, ok := gotInteger.AsInt(); !ok || value != 1 {
		t.Fatalf("integer arm = %v, %v", value, ok)
	}
	text := baml_sdk.NewStringOrIntFromString("s")
	gotText, err := baml_sdk.UnionsRoundTripNullToEnd(ctx, &text)
	if err != nil || gotText == nil {
		t.Fatalf("string union = %#v, %v", gotText, err)
	}
	if value, ok := gotText.AsString(); !ok || value != "s" {
		t.Fatalf("string arm = %q, %v", value, ok)
	}
	if got, err := baml_sdk.UnionsRoundTripNullToEnd(ctx, nil); err != nil || got != nil {
		t.Fatalf("null union = %#v, %v", got, err)
	}
}

func Test_round_trip_dedup(t *testing.T) {
	ctx := context.Background()
	integer := baml_sdk.NewStringOrIntFromInt(2)
	if got, err := baml_sdk.UnionsRoundTripDedup(ctx, integer); err != nil {
		t.Fatal(err)
	} else if value, ok := got.AsInt(); !ok || value != 2 {
		t.Fatalf("integer arm = %v, %v", value, ok)
	}
	text := baml_sdk.NewStringOrIntFromString("x")
	if got, err := baml_sdk.UnionsRoundTripDedup(ctx, text); err != nil {
		t.Fatal(err)
	} else if value, ok := got.AsString(); !ok || value != "x" {
		t.Fatalf("string arm = %q, %v", value, ok)
	}
}

func Test_round_trip_singleton_unwrap(t *testing.T) {
	if got, err := baml_sdk.UnionsRoundTripSingletonUnwrap(context.Background(), 7); err != nil || got != 7 {
		t.Fatalf("singleton union = %v, %v", got, err)
	}
}

func Test_round_trip_optional_plus_null(t *testing.T) {
	ctx := context.Background()
	class := baml_sdk.NewStringOrUnionsTFromUnionsT(baml_sdk.UnionsT{V: 1})
	if got, err := baml_sdk.UnionsRoundTripOptionalPlusNull(ctx, &class); err != nil || got == nil {
		t.Fatalf("class union = %#v, %v", got, err)
	} else if value, ok := got.AsUnionsT(); !ok || value != (baml_sdk.UnionsT{V: 1}) {
		t.Fatalf("class arm = %#v, %v", value, ok)
	}
	text := baml_sdk.NewStringOrUnionsTFromString("s")
	if got, err := baml_sdk.UnionsRoundTripOptionalPlusNull(ctx, &text); err != nil || got == nil {
		t.Fatalf("string union = %#v, %v", got, err)
	} else if value, ok := got.AsString(); !ok || value != "s" {
		t.Fatalf("string arm = %q, %v", value, ok)
	}
	if got, err := baml_sdk.UnionsRoundTripOptionalPlusNull(ctx, nil); err != nil || got != nil {
		t.Fatalf("null union = %#v, %v", got, err)
	}
}

func Test_round_trip_union_t(t *testing.T) {
	want := baml_sdk.UnionsT{V: 4}
	if got, err := baml_sdk.UnionsRoundTripT(context.Background(), want); err != nil || got != want {
		t.Fatalf("T = %#v, %v", got, err)
	}
}

func Test_round_trip_union_container(t *testing.T) {
	nullToEnd := (*baml_sdk.StringOrInt)(nil)
	dedup := baml_sdk.NewStringOrIntFromString("d")
	optional := baml_sdk.NewStringOrUnionsTFromUnionsT(baml_sdk.UnionsT{V: 2})
	want := baml_sdk.UnionsUnionContainer{
		NullToEnd:        nullToEnd,
		Dedup:            dedup,
		SingletonUnwrap:  5,
		OptionalPlusNull: &optional,
	}
	got, err := baml_sdk.UnionsRoundTripUnionContainer(context.Background(), want)
	if err != nil {
		t.Fatal(err)
	}
	if got.NullToEnd != nil || got.SingletonUnwrap != 5 {
		t.Fatalf("container scalar fields = %#v", got)
	}
	if value, ok := got.Dedup.AsString(); !ok || value != "d" {
		t.Fatalf("container dedup = %q, %v", value, ok)
	}
	if got.OptionalPlusNull == nil {
		t.Fatal("container optional union is nil")
	}
	if value, ok := got.OptionalPlusNull.AsUnionsT(); !ok || value != (baml_sdk.UnionsT{V: 2}) {
		t.Fatalf("container optional arm = %#v, %v", value, ok)
	}
}
