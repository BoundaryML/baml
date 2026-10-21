package sdk_test

import (
	"context"
	"reflect"
	"testing"

	"baml.local/sdk/baml_sdk"
)

// A value in a type variable's slot crosses by Go reflection, which cannot see
// a declared type Go's surface erases: a `json` field is `any`, so its maps and
// lists would state `unknown` elements, which the engine refuses under `json`.
// Every generated class therefore encodes itself under its declared field
// types, wherever it sits.

func jsonBoxes() map[string]baml_sdk.GoJsonTestsJsonBox {
	return map[string]baml_sdk.GoJsonTestsJsonBox{
		"empty": {
			Payload: map[string]any{},
			List:    []any{},
			Mapping: map[string]any{},
		},
		"nested": {
			Payload:  map[string]any{"k": []any{int64(1), "two", map[string]any{"n": nil}}, "empty": []any{}},
			List:     []any{true, 1.5, []any{}, map[string]any{}},
			Mapping:  map[string]any{"a": []any{map[string]any{"b": "c"}}},
			Nullable: map[string]any{},
		},
	}
}

func Test_generic_class_with_a_json_field_round_trips(t *testing.T) {
	ctx := context.Background()
	for name, box := range jsonBoxes() {
		t.Run(name, func(t *testing.T) {
			// T is the class itself.
			gotBox, err := baml_sdk.GenericTestsIdentity(ctx, box)
			if err != nil || !reflect.DeepEqual(gotBox, box) {
				t.Fatalf("identity(JsonBox) = %#v, %v; want %#v", gotBox, err, box)
			}
			// The class fills a generic class's `T` field.
			generic := baml_sdk.GenericTestsGenericBox[baml_sdk.GoJsonTestsJsonBox]{Value: box}
			gotGeneric, err := baml_sdk.GenericTestsIdentity(ctx, generic)
			if err != nil || !reflect.DeepEqual(gotGeneric, generic) {
				t.Fatalf("identity(GenericBox<JsonBox>) = %#v, %v; want %#v", gotGeneric, err, generic)
			}
			// Every field shape of `T`, including the `T | string | null` union.
			shape := baml_sdk.GenericTestsContainerShapes[baml_sdk.GoJsonTestsJsonBox]{
				Item:  box,
				Items: []baml_sdk.GoJsonTestsJsonBox{box, box},
				ByKey: map[string]baml_sdk.GoJsonTestsJsonBox{"k": box},
				Maybe: &box,
				Mixed: box,
			}
			items, err := baml_sdk.GenericTestsReadItems(ctx, shape)
			if err != nil || !reflect.DeepEqual(items, shape.Items) {
				t.Fatalf("read_items(ContainerShapes<JsonBox>) = %#v, %v; want %#v", items, err, shape.Items)
			}
			shape.Mixed = "text"
			if _, err := baml_sdk.GenericTestsReadItems(ctx, shape); err != nil {
				t.Fatalf("read_items with a string `mixed`: %v", err)
			}
		})
	}
}

func Test_generic_class_instantiated_at_int(t *testing.T) {
	ctx := context.Background()
	got, err := baml_sdk.GenericTestsConsumeIntWrapper(ctx, baml_sdk.GenericTestsGenericBox[int64]{Value: 7})
	if err != nil || got != 7 {
		t.Fatalf("consume_int_wrapper(GenericBox<int>) = %d, %v", got, err)
	}
	box, err := baml_sdk.GenericTestsIdentity(ctx, baml_sdk.GenericTestsGenericBox[int64]{Value: 9})
	if err != nil || box.Value != 9 {
		t.Fatalf("identity(GenericBox<int>) = %#v, %v", box, err)
	}
}
