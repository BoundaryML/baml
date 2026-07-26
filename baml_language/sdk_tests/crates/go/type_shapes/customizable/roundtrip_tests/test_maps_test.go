package sdk_test

import (
	"baml.local/sdk/baml_sdk"
	"context"
	"reflect"
	"testing"
)

func Test_round_trip_simple_map(t *testing.T) {
	want := map[string]int64{"a": 1, "b": 2}
	got, err := baml_sdk.MapsRoundTripSimpleMap(context.Background(), want)
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("got %#v, %v", got, err)
	}
}
func Test_round_trip_list_valued_map(t *testing.T) {
	want := map[string][]int64{"k": {1, 2}}
	got, err := baml_sdk.MapsRoundTripListValuedMap(context.Background(), want)
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("got %#v, %v", got, err)
	}
}
func Test_round_trip_map_sentiment(t *testing.T) {
	got, err := baml_sdk.MapsRoundTripSentiment(context.Background(), baml_sdk.MapsSentimentPositive)
	if err != nil || got != baml_sdk.MapsSentimentPositive {
		t.Fatalf("got %q, %v", got, err)
	}
}
func Test_round_trip_map_resume(t *testing.T) {
	want := baml_sdk.MapsResume{Name: "n"}
	got, err := baml_sdk.MapsRoundTripResume(context.Background(), want)
	if err != nil || got != want {
		t.Fatalf("got %#v, %v", got, err)
	}
}
