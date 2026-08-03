package sdk_test

import (
	"context"
	"errors"
	"reflect"
	"strings"
	"testing"
	"time"

	"baml.local/sdk/baml_sdk"
	"github.com/boundaryml/baml-go"
)

func Test_static_method_required_default_and_structured_round_trips(t *testing.T) {
	ctx := context.Background()
	if got, err := baml_sdk.StaticMethodEdgesEdgeRequired(ctx, "required"); err != nil || got != "required" {
		t.Fatalf("required = %q, %v", got, err)
	}
	if got, err := baml_sdk.StaticMethodEdgesEdgeDefaulted(ctx, "required"); err != nil || !reflect.DeepEqual(got, []string{"required", "default"}) {
		t.Fatalf("defaulted = %#v, %v", got, err)
	}
	if got, err := baml_sdk.StaticMethodEdgesEdgeDefaulted(ctx, "required", baml_sdk.WithStaticMethodEdgesEdgeDefaultedOptional("override")); err != nil || !reflect.DeepEqual(got, []string{"required", "override"}) {
		t.Fatalf("default override = %#v, %v", got, err)
	}

	integer := int64(42)
	if got, err := baml_sdk.StaticMethodEdgesEdgeNullable(ctx, &integer); err != nil || got == nil || *got != integer {
		t.Fatalf("nullable = %#v, %v", got, err)
	}
	if got, err := baml_sdk.StaticMethodEdgesEdgeNullable(ctx, nil); err != nil || got != nil {
		t.Fatalf("nullable nil = %#v, %v", got, err)
	}
	if got, err := baml_sdk.StaticMethodEdgesEdgeList(ctx, []string{"a", "b"}); err != nil || !reflect.DeepEqual(got, []string{"a", "b"}) {
		t.Fatalf("list = %#v, %v", got, err)
	}
	if got, err := baml_sdk.StaticMethodEdgesEdgeMapping(ctx, map[string]int64{"a": 1}); err != nil || !reflect.DeepEqual(got, map[string]int64{"a": 1}) {
		t.Fatalf("map = %#v, %v", got, err)
	}
	protected := []string{"ctx", "err", "result", "zero", "bootstrap", "receiver", "arguments"}
	if got, err := baml_sdk.StaticMethodEdgesEdgeProtectedLocals(ctx, protected[0], protected[1], protected[2], protected[3], protected[4], protected[5], protected[6]); err != nil || !reflect.DeepEqual(got, protected) {
		t.Fatalf("protected locals = %#v, %v", got, err)
	}

	payload := baml_sdk.StaticMethodEdgesStaticPayload{Value: "payload"}
	if got, err := baml_sdk.StaticMethodEdgesEdgeClassValue(ctx, payload); err != nil || got != payload {
		t.Fatalf("class = %#v, %v", got, err)
	}
	if got, err := baml_sdk.StaticMethodEdgesEdgeAliasValue(ctx, payload); err != nil || got != payload {
		t.Fatalf("alias = %#v, %v", got, err)
	}
	if got, err := baml_sdk.StaticMethodEdgesEdgeEnumValue(ctx, baml_sdk.StaticMethodEdgesStaticMoodHAPPY); err != nil || got != baml_sdk.StaticMethodEdgesStaticMoodHAPPY {
		t.Fatalf("enum = %#v, %v", got, err)
	}

	union := baml_sdk.NewStringOrIntFromInt(7)
	gotUnion, err := baml_sdk.StaticMethodEdgesEdgeUnionValue(ctx, union)
	if err != nil {
		t.Fatal(err)
	}
	if got, ok := gotUnion.AsInt(); !ok || got != 7 {
		t.Fatalf("union = %#v, %v", got, ok)
	}
}

func Test_static_method_media_json_type_and_rust_type_round_trips(t *testing.T) {
	ctx := context.Background()
	image, err := baml_go.NewImageFromUrl("https://example.com/static.png", nil)
	if err != nil {
		t.Fatal(err)
	}
	gotImage, err := baml_sdk.StaticMethodEdgesEdgeImageValue(ctx, image)
	if err != nil {
		t.Fatal(err)
	}
	url, err := gotImage.Url()
	if err != nil || url == nil || *url != "https://example.com/static.png" {
		t.Fatalf("image URL = %#v, %v", url, err)
	}

	jsonValue := map[string]any{"nested": []any{true, "value", int64(3)}}
	if got, err := baml_sdk.StaticMethodEdgesEdgeJsonValue(ctx, jsonValue); err != nil || !reflect.DeepEqual(got, jsonValue) {
		t.Fatalf("json = %#v, %v", got, err)
	}
	typeValue := baml_go.MapBAMLType(
		baml_go.PrimitiveBAMLType(baml_go.StringType),
		baml_go.ListBAMLType(baml_go.PrimitiveBAMLType(baml_go.IntType)),
	)
	gotType, err := baml_sdk.StaticMethodEdgesEdgeTypeValue(ctx, typeValue)
	if err != nil || !gotType.Equal(typeValue) {
		t.Fatalf("type = %#v, %v", gotType, err)
	}

	response, err := baml_sdk.StaticMethodEdgesEdgeMakeResponse(ctx, "opaque static body")
	if err != nil {
		t.Fatal(err)
	}
	response, err = baml_sdk.StaticMethodEdgesEdgeRustTypeValue(ctx, response)
	if err != nil {
		t.Fatal(err)
	}
	body, err := baml_sdk.StaticMethodEdgesEdgeReadResponse(ctx, response)
	if err != nil || body != "opaque static body" {
		t.Fatalf("opaque response = %q, %v", body, err)
	}
}

func Test_static_method_errors_never_cancellation_and_collision_names(t *testing.T) {
	ctx := context.Background()
	if got, err := baml_sdk.StaticMethodEdgesEdgeRoundTrip_9a9648a9(ctx, "method"); err != nil || got != "method" {
		t.Fatalf("Edge.round_trip = %q, %v", got, err)
	}
	if got, err := baml_sdk.StaticMethodEdgesEdgeRoundTrip_a8f3b1fb(ctx, "other helper"); err != nil || got != "other helper" {
		t.Fatalf("EdgeRound.trip = %q, %v", got, err)
	}
	if got, err := baml_sdk.StaticMethodEdgesEdgeRoundTrip_0fc5c27f(ctx, "free"); err != nil || got != "free" {
		t.Fatalf("edge_round_trip = %q, %v", got, err)
	}

	got, err := baml_sdk.StaticMethodEdgesEdgeThrowError(ctx, "static boom")
	if got != "" || err == nil || !strings.Contains(err.Error(), "user.static_method_edges.StaticMethodError: static boom") {
		t.Fatalf("throw = %q, %v", got, err)
	}
	err = baml_sdk.StaticMethodEdgesEdgePanic(ctx, "static panic")
	if err == nil || !strings.Contains(err.Error(), "BAML panic:") || !strings.Contains(err.Error(), "static panic") {
		t.Fatalf("never = %v", err)
	}

	timed, cancel := context.WithTimeout(ctx, 150*time.Millisecond)
	defer cancel()
	start := time.Now()
	_, err = baml_sdk.StaticMethodEdgesEdgeSleepMs(timed, 2000)
	if err != timed.Err() || !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("cancellation = %v, want exact %v", err, timed.Err())
	}
	if elapsed := time.Since(start); elapsed >= time.Second {
		t.Fatalf("cancellation took %s", elapsed)
	}
}
