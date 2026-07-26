package sdk_test

import (
	"baml.local/sdk/baml_sdk"
	"context"
	"testing"
)

func Test_return_literals(t *testing.T) {
	ctx := context.Background()
	if got, err := baml_sdk.LiteralsReturnLiteral42(ctx); err != nil || got != 42 {
		t.Fatalf("42 = %v, %v", got, err)
	}
	if got, err := baml_sdk.LiteralsReturnLiteralNegOne(ctx); err != nil || got != -1 {
		t.Fatalf("-1 = %v, %v", got, err)
	}
	if got, err := baml_sdk.LiteralsReturnLiteralDraft(ctx); err != nil || got != "draft" {
		t.Fatalf("draft = %q, %v", got, err)
	}
	if got, err := baml_sdk.LiteralsReturnLiteralEscaped(ctx); err != nil || got != `has "quotes"` {
		t.Fatalf("escaped = %q, %v", got, err)
	}
	if got, err := baml_sdk.LiteralsReturnLiteralTrue(ctx); err != nil || !got {
		t.Fatalf("true = %v, %v", got, err)
	}
	if got, err := baml_sdk.LiteralsReturnLiteralFalse(ctx); err != nil || got {
		t.Fatalf("false = %v, %v", got, err)
	}
}
func Test_round_trip_literal42(t *testing.T) {
	got, err := baml_sdk.LiteralsRoundTripLiteral42(context.Background(), 42)
	if err != nil || got != 42 {
		t.Fatalf("got %v, %v", got, err)
	}
}
func Test_round_trip_literal_draft(t *testing.T) {
	got, err := baml_sdk.LiteralsRoundTripLiteralDraft(context.Background(), "draft")
	if err != nil || got != "draft" {
		t.Fatalf("got %q, %v", got, err)
	}
}
func Test_round_trip_literal_escaped(t *testing.T) {
	got, err := baml_sdk.LiteralsRoundTripLiteralEscaped(context.Background(), `has "quotes"`)
	if err != nil || got != `has "quotes"` {
		t.Fatalf("got %q, %v", got, err)
	}
}
func Test_round_trip_literal_true(t *testing.T) {
	got, err := baml_sdk.LiteralsRoundTripLiteralTrue(context.Background(), true)
	if err != nil || !got {
		t.Fatalf("got %v, %v", got, err)
	}
}
func Test_round_trip_literal_false(t *testing.T) {
	got, err := baml_sdk.LiteralsRoundTripLiteralFalse(context.Background(), false)
	if err != nil || got {
		t.Fatalf("got %v, %v", got, err)
	}
}
func Test_round_trip_literals(t *testing.T) {
	want := baml_sdk.LiteralsLiterals{Literal42: 42, LiteralDraft: "draft", LiteralEscaped: `has "quotes"`, LiteralTrue: true, LiteralFalse: false}
	got, err := baml_sdk.LiteralsRoundTripLiterals(context.Background(), want)
	if err != nil || got != want {
		t.Fatalf("got %#v, %v", got, err)
	}
}
