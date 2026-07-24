package sdk_test

import (
	"baml.local/sdk/baml_sdk"
	"context"
	"testing"
)

func TestPickSentiment(t *testing.T) {
	got, err := baml_sdk.EnumsPickSentiment(context.Background(), true)
	if err != nil || got != baml_sdk.EnumsSentimentPositive {
		t.Fatalf("true = %q, %v", got, err)
	}
	got, err = baml_sdk.EnumsPickSentiment(context.Background(), false)
	if err != nil || got != baml_sdk.EnumsSentimentNegative {
		t.Fatalf("false = %q, %v", got, err)
	}
}
func TestPickPositive(t *testing.T) {
	got, err := baml_sdk.EnumsPickPositive(context.Background())
	if err != nil || got != baml_sdk.EnumsSentimentPositive {
		t.Fatalf("got %q, %v", got, err)
	}
}
func TestRoundTripSentiment(t *testing.T) {
	got, err := baml_sdk.EnumsRoundTripSentiment(context.Background(), baml_sdk.EnumsSentimentNegative)
	if err != nil || got != baml_sdk.EnumsSentimentNegative {
		t.Fatalf("got %q, %v", got, err)
	}
}
func TestRoundTripSentimentPositive(t *testing.T) {
	got, err := baml_sdk.EnumsRoundTripSentimentPositive(context.Background(), baml_sdk.EnumsSentimentPositive)
	if err != nil || got != baml_sdk.EnumsSentimentPositive {
		t.Fatalf("got %q, %v", got, err)
	}
}
func TestRoundTripEnums(t *testing.T) {
	want := baml_sdk.EnumsEnums{BareEnum: baml_sdk.EnumsSentimentPositive, VariantAsType: baml_sdk.EnumsSentimentPositive}
	got, err := baml_sdk.EnumsRoundTripEnums(context.Background(), want)
	if err != nil || got != want {
		t.Fatalf("got %#v, %v", got, err)
	}
}
