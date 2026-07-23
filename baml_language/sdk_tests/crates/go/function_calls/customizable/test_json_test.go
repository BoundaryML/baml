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

func TestCanonicalJSONRoundTripsAtTopLevelAndThroughAlias(t *testing.T) {
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

func TestCanonicalJSONComposesThroughContainersAndClasses(t *testing.T) {
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

func TestCanonicalJSONDefaultsAndCallbacks(t *testing.T) {
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

func TestCanonicalJSONDynamicUnion(t *testing.T) {
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

func TestCanonicalJSONClassUnionUsesDeclaredFieldCodecs(t *testing.T) {
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

func TestCanonicalJSONRejectsExtensionsBeforeDispatch(t *testing.T) {
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
