package sdk_test

import (
	"context"
	"math"
	"reflect"
	"strings"
	"testing"

	"baml.local/sdk/baml_sdk"
	baml_go "github.com/boundaryml/baml-go"
)

func canonicalJSONFixture() map[string]any {
	return map[string]any{
		"null":   nil,
		"bool":   true,
		"int":    int64(42),
		"float":  1.5,
		"string": "value",
		"list":   []any{nil, false, int64(-3), 2.25, "nested"},
		"map":    map[string]any{"child": []any{}},
	}
}

type nestedJSONMarshaler struct{}

func (nestedJSONMarshaler) BAMLInput() baml_go.Input {
	return baml_go.String("must not bypass the JSON codec")
}

func Test_canonical_json_round_trips_at_top_level_and_through_alias(t *testing.T) {
	ctx := context.Background()
	want := canonicalJSONFixture()
	for name, call := range map[string]func(context.Context, any) (any, error){
		"canonical": baml_sdk.GoJsonTestsRoundTripJson,
		"alias":     baml_sdk.GoJsonTestsRoundTripJsonAlias,
	} {
		t.Run(name, func(t *testing.T) {
			got, err := call(ctx, want)
			if err != nil || !reflect.DeepEqual(got, want) {
				t.Fatalf("JSON round trip = %#v, %v; want %#v", got, err, want)
			}
		})
	}
}

func Test_canonical_json_composes_through_containers_and_classes(t *testing.T) {
	ctx := context.Background()
	values := []any{nil, canonicalJSONFixture(), []any{}, map[string]any{}}
	gotList, err := baml_sdk.GoJsonTestsRoundTripJsonList(ctx, values)
	if err != nil || !reflect.DeepEqual(gotList, values) {
		t.Fatalf("JSON list = %#v, %v; want %#v", gotList, err, values)
	}

	mapping := map[string]any{"value": canonicalJSONFixture(), "null": nil}
	gotMap, err := baml_sdk.GoJsonTestsRoundTripJsonMap(ctx, mapping)
	if err != nil || !reflect.DeepEqual(gotMap, mapping) {
		t.Fatalf("JSON map = %#v, %v; want %#v", gotMap, err, mapping)
	}

	box := baml_sdk.GoJsonTestsJsonBox{
		Payload:  canonicalJSONFixture(),
		List:     values,
		Mapping:  mapping,
		Nullable: nil,
	}
	gotBox, err := baml_sdk.GoJsonTestsRoundTripJsonBox(ctx, box)
	if err != nil || !reflect.DeepEqual(gotBox, box) {
		t.Fatalf("JSON class = %#v, %v; want %#v", gotBox, err, box)
	}
}

func Test_canonical_json_defaults_and_callbacks(t *testing.T) {
	ctx := context.Background()
	got, err := baml_sdk.GoJsonTestsDefaultJson(ctx)
	if err != nil || got != nil {
		t.Fatalf("default JSON = %#v, %v; want nil", got, err)
	}

	want := canonicalJSONFixture()
	got, err = baml_sdk.GoJsonTestsDefaultJson(ctx, baml_sdk.WithGoJsonTestsDefaultJsonValue(want))
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("explicit JSON option = %#v, %v; want %#v", got, err, want)
	}

	got, err = baml_sdk.GoJsonTestsCallJsonCallback(ctx, func(value any) any {
		object := value.(map[string]any)
		object["callback"] = true
		return object
	}, map[string]any{"input": int64(1)})
	wantCallback := map[string]any{"input": int64(1), "callback": true}
	if err != nil || !reflect.DeepEqual(got, wantCallback) {
		t.Fatalf("JSON callback = %#v, %v; want %#v", got, err, wantCallback)
	}
}

func Test_canonical_json_dynamic_union(t *testing.T) {
	ctx := context.Background()
	for name, want := range map[string]any{
		"object": canonicalJSONFixture(),
		"null":   nil,
	} {
		t.Run(name, func(t *testing.T) {
			got, err := baml_sdk.GoJsonTestsRoundTripJsonOrImage(ctx, want)
			if err != nil || !reflect.DeepEqual(got, want) {
				t.Fatalf("JSON union = %#v, %v; want %#v", got, err, want)
			}
		})
	}

	const imageURL = "https://example.com/json-union.png"
	image, err := baml_go.NewImageFromUrl(imageURL, nil)
	if err != nil {
		t.Fatal(err)
	}
	got, err := baml_sdk.GoJsonTestsRoundTripJsonOrImage(ctx, image)
	if err != nil {
		t.Fatalf("image union: %v", err)
	}
	gotImage, ok := got.(baml_go.Image)
	if !ok {
		t.Fatalf("image union type = %T, want baml_go.Image", got)
	}
	gotURL, err := gotImage.Url()
	if err != nil || gotURL == nil || *gotURL != imageURL {
		t.Fatalf("image union URL = %v, %v; want %q", gotURL, err, imageURL)
	}
}

func Test_canonical_json_class_union_uses_declared_field_codecs(t *testing.T) {
	ctx := context.Background()
	var typedNilMap map[string]any
	box := baml_sdk.GoJsonTestsJsonBox{
		Payload: typedNilMap,
		List:    []any{},
		Mapping: map[string]any{},
	}

	typed := baml_sdk.NewImageOrGoJsonTestsJsonBoxFromGoJsonTestsJsonBox(box)
	typedResult, err := baml_sdk.GoJsonTestsRoundTripJsonBoxOrImage(ctx, typed)
	if err != nil {
		t.Fatal(err)
	}
	typedBox, ok := typedResult.AsGoJsonTestsJsonBox()
	if !ok || typedBox.Payload != nil {
		t.Fatalf("typed class union payload = %#v, %v; want JSON null", typedBox.Payload, ok)
	}

	dynamicResult, err := baml_sdk.GoJsonTestsRoundTripJsonBoxDynamic(ctx, box)
	if err != nil {
		t.Fatal(err)
	}
	dynamicBox, ok := dynamicResult.(baml_sdk.GoJsonTestsJsonBox)
	if !ok || dynamicBox.Payload != nil {
		t.Fatalf("dynamic class union payload = %#v (%T); want JSON null", dynamicResult, dynamicResult)
	}

	box.Payload = nestedJSONMarshaler{}
	for name, call := range map[string]func() error{
		"typed": func() error {
			_, err := baml_sdk.GoJsonTestsRoundTripJsonBoxOrImage(
				ctx,
				baml_sdk.NewImageOrGoJsonTestsJsonBoxFromGoJsonTestsJsonBox(box),
			)
			return err
		},
		"dynamic": func() error {
			_, err := baml_sdk.GoJsonTestsRoundTripJsonBoxDynamic(ctx, box)
			return err
		},
	} {
		t.Run(name, func(t *testing.T) {
			err := call()
			if err == nil || !strings.Contains(err.Error(), "generated BAML values are not JSON") {
				t.Fatalf("nested marshaler error = %v", err)
			}
		})
	}
}

func Test_host_supplied_json_supports_typed_narrowing(t *testing.T) {
	// Host-supplied json objects must materialize with `json` container
	// typing: a `match (j) { let m: map<string, json> => ... }` inside BAML
	// (and therefore `baml.json.path` / `path_or`) must treat them exactly
	// like BAML-born `baml.json.parse` values.
	ctx := context.Background()
	object := map[string]any{
		"type": "ok",
		"nested": map[string]any{
			"list": []any{int64(1), map[string]any{"deep": "found"}},
		},
	}

	kinds := map[string]any{
		"object": object,
		"array":  []any{int64(1)},
		"string": "text",
		"other":  int64(3),
	}
	for want, value := range kinds {
		got, err := baml_sdk.GoJsonTestsJsonKind(ctx, value)
		if err != nil || got != want {
			t.Fatalf("json_kind(%#v) = %q, %v; want %q", value, got, err, want)
		}
	}

	got, err := baml_sdk.GoJsonTestsJsonPathString(ctx, object, ".type")
	if err != nil || got != "ok" {
		t.Fatalf("json_path_string(.type) = %q, %v; want %q", got, err, "ok")
	}

	got, err = baml_sdk.GoJsonTestsJsonPathString(ctx, object, ".nested.list[1].deep")
	if err != nil || got != "found" {
		t.Fatalf("json_path_string(.nested.list[1].deep) = %q, %v; want %q", got, err, "found")
	}

	got, err = baml_sdk.GoJsonTestsJsonPathStringOr(ctx, object, ".missing", "fallback")
	if err != nil || got != "fallback" {
		t.Fatalf("json_path_string_or(.missing) = %q, %v; want %q", got, err, "fallback")
	}

	_, err = baml_sdk.GoJsonTestsJsonPathString(ctx, object, ".absent")
	if err == nil || !strings.Contains(err.Error(), "missing field") {
		t.Fatalf("json_path_string(.absent) error = %v; want missing-field JsonPathError", err)
	}
}

func Test_json_returned_from_host_callback_supports_typed_narrowing(t *testing.T) {
	// json returned from a host callback converts on the host-return path
	// (no argument coercion pass); it must narrow identically.
	ctx := context.Background()
	got, err := baml_sdk.GoJsonTestsJsonCallbackKind(ctx, func(value any) any {
		return map[string]any{"wrapped": value}
	}, "payload")
	if err != nil || got != "object" {
		t.Fatalf("json_callback_kind = %q, %v; want %q", got, err, "object")
	}
}

func Test_canonical_json_rejects_extensions_before_dispatch(t *testing.T) {
	ctx := context.Background()
	invalid := []struct {
		name  string
		value any
		want  string
	}{
		{"nan", math.NaN(), "non-finite"},
		{"infinity", math.Inf(1), "non-finite"},
		{"bytes", []byte("not json"), "byte slices"},
	}
	for _, test := range invalid {
		t.Run(test.name, func(t *testing.T) {
			_, err := baml_sdk.GoJsonTestsRoundTripJson(ctx, test.value)
			if err == nil || !strings.Contains(err.Error(), test.want) {
				t.Fatalf("invalid JSON error = %v, want %q", err, test.want)
			}

			_, err = baml_sdk.GoJsonTestsRoundTripJsonOrImage(ctx, test.value)
			if err == nil {
				t.Fatalf("dynamic JSON union unexpectedly accepted %T", test.value)
			}
		})
	}

	box := baml_sdk.GoJsonTestsJsonBox{Payload: math.NaN(), List: []any{}, Mapping: map[string]any{}}
	union := baml_sdk.NewImageOrGoJsonTestsJsonBoxFromGoJsonTestsJsonBox(box)
	_, err := baml_sdk.GoJsonTestsRoundTripJsonBoxOrImage(ctx, union)
	if err == nil || !strings.Contains(err.Error(), "non-finite") {
		t.Fatalf("union -> class -> JSON error = %v", err)
	}

	_, err = baml_sdk.GoJsonTestsRoundTripJsonBoxDynamic(ctx, box)
	if err == nil || !strings.Contains(err.Error(), "non-finite") {
		t.Fatalf("dynamic union -> class -> JSON error = %v", err)
	}
}
