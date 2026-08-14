package sdk_test

import (
	"context"
	"reflect"
	"strings"
	"testing"

	"baml.local/sdk/baml_sdk"
)

func Test_generic_identity_inference_and_explicit_types(t *testing.T) {
	ctx := context.Background()
	integer, err := baml_sdk.GenericTestsIdentity(ctx, int64(5))
	if err != nil || integer != 5 {
		t.Fatalf("identity int = %d, %v", integer, err)
	}
	text, err := baml_sdk.GenericTestsIdentity(ctx, "hello")
	if err != nil || text != "hello" {
		t.Fatalf("identity string = %q, %v", text, err)
	}
	pair := baml_sdk.GenericTestsStringIntPair{MyString: "a", MyInt: 1}
	gotPair, err := baml_sdk.GenericTestsIdentity(ctx, pair)
	if err != nil || gotPair != pair {
		t.Fatalf("identity class = %#v, %v", gotPair, err)
	}
	nested := baml_sdk.GenericTestsGenericBox[baml_sdk.GenericTestsGenericBox[string]]{
		Value: baml_sdk.GenericTestsGenericBox[string]{Value: "nested"},
	}
	gotNested, err := baml_sdk.GenericTestsIdentity(ctx, nested)
	if err != nil || !reflect.DeepEqual(gotNested, nested) {
		t.Fatalf("identity nested = %#v, %v", gotNested, err)
	}
}

func Test_generic_return_only_type_arguments(t *testing.T) {
	ctx := context.Background()
	name, err := baml_sdk.GenericTestsOneTypeArg[int64](ctx)
	if err != nil || name != "int" {
		t.Fatalf("one type arg = %q, %v", name, err)
	}
	two, err := baml_sdk.GenericTestsTwoTypeArgs[int64, string](ctx)
	if err != nil || two != "int | string" {
		t.Fatalf("two type args = %q, %v", two, err)
	}
	parsed, err := baml_sdk.GenericTestsParseAs[baml_sdk.GenericTestsStringIntPair](ctx, `{"my_string":"x","my_int":3}`)
	if err != nil || parsed.MyString != "x" || parsed.MyInt != 3 {
		t.Fatalf("parse class = %#v, %v", parsed, err)
	}
}

func Test_generic_classes_containers_and_concrete_outputs(t *testing.T) {
	ctx := context.Background()
	triple, err := baml_sdk.GenericTestsMakeTriple(ctx, int64(1), []string{"a", "b"}, map[string]bool{"k": true})
	if err != nil || triple.First != 1 || !reflect.DeepEqual(triple.Second, []string{"a", "b"}) || !triple.Third["k"] {
		t.Fatalf("triple = %#v, %v", triple, err)
	}
	shape := baml_sdk.GenericTestsContainerShapes[int64]{
		Item: 1, Items: []int64{1, 2, 3}, ByKey: map[string]int64{"k": 4}, Mixed: int64(5),
	}
	items, err := baml_sdk.GenericTestsReadItems(ctx, shape)
	if err != nil || !reflect.DeepEqual(items, []int64{1, 2, 3}) {
		t.Fatalf("items = %#v, %v", items, err)
	}
	linked := baml_sdk.GenericTestsGenericRecursive[int64]{Value: 7, Next: &baml_sdk.GenericTestsGenericRecursive[int64]{Value: 8}}
	head, err := baml_sdk.GenericTestsListHead(ctx, linked)
	if err != nil || head != 7 {
		t.Fatalf("head = %d, %v", head, err)
	}
	box, err := baml_sdk.GenericTestsMakeIntBox(ctx)
	if err != nil || box.Value != 7 {
		t.Fatalf("int box = %#v, %v", box, err)
	}
	outer, err := baml_sdk.GenericTestsMakeNestedBox(ctx)
	if err != nil || outer.Value.Value != 9 {
		t.Fatalf("nested box = %#v, %v", outer, err)
	}
	container, err := baml_sdk.GenericTestsMakeIntContainer(ctx)
	if err != nil || container.Mixed != int64(5) || container.Maybe != nil {
		t.Fatalf("dynamic generic class field = %#v, %v", container, err)
	}
	closedUnion := baml_sdk.NewStringOrIntFromString("union")
	gotUnion, err := baml_sdk.GenericTestsIdentity(ctx, closedUnion)
	if err != nil {
		t.Fatal(err)
	}
	if value, ok := gotUnion.AsString(); !ok || value != "union" {
		t.Fatalf("generic closed union = %#v", gotUnion)
	}
}

func Test_generic_nullable_type_variables_preserve_every_pointer_boundary(t *testing.T) {
	ctx := context.Background()
	value := int64(5)
	present, err := baml_sdk.GenericTestsMaybeId(ctx, &value)
	if err != nil || present == nil || *present != 5 {
		t.Fatalf("maybe present = %#v, %v", present, err)
	}
	absent, err := baml_sdk.GenericTestsMaybeId[int64](ctx, nil)
	if err != nil || absent != nil {
		t.Fatalf("maybe absent = %#v, %v", absent, err)
	}

	// T itself is nullable here, so the BAML T? parameter and return become
	// **int64 in Go. The shape is Go-legal, but BAML canonicalizes repeated
	// nullability: outer nil and pointer-to-nil both cross the wire as null.
	optionalValue := &value
	doublePointer := &optionalValue
	nested, err := baml_sdk.GenericTestsMaybeId[*int64](ctx, doublePointer)
	if err != nil || nested == nil || *nested == nil || **nested != 5 {
		t.Fatalf("nested optional = %#v, %v", nested, err)
	}
	nestedAbsent, err := baml_sdk.GenericTestsMaybeId[*int64](ctx, nil)
	if err != nil || nestedAbsent != nil {
		t.Fatalf("nested outer nil = %#v, %v", nestedAbsent, err)
	}
	var nilInner *int64
	pointerToNil := &nilInner
	nestedAbsent, err = baml_sdk.GenericTestsMaybeId[*int64](ctx, pointerToNil)
	if err != nil || nestedAbsent != nil {
		t.Fatalf("nested inner nil = %#v, %v", nestedAbsent, err)
	}

	empty, err := baml_sdk.GenericTestsFirstOr[int64](ctx, nil)
	if err != nil || empty != nil {
		t.Fatalf("first empty = %#v, %v", empty, err)
	}
	nestedShape := baml_sdk.GenericTestsContainerShapes[*int64]{
		Item:  optionalValue,
		Items: []*int64{optionalValue},
		ByKey: map[string]*int64{"value": optionalValue},
		Maybe: doublePointer,
		Mixed: optionalValue,
	}
	items, err := baml_sdk.GenericTestsReadItems(ctx, nestedShape)
	if err != nil || len(items) != 1 || items[0] == nil || *items[0] != 5 {
		t.Fatalf("nullable class items = %#v, %v", items, err)
	}
}

func Test_generic_receiver_and_static_helpers(t *testing.T) {
	ctx := context.Background()
	box := baml_sdk.GenericTestsGenericBox[int64]{Value: 5}
	got, err := box.Get(ctx)
	if err != nil || got != "int" {
		t.Fatalf("get = %q, %v", got, err)
	}
	pair, err := baml_sdk.GenericTestsGenericBoxPairWith(ctx, box, "hello")
	if err != nil || pair != "int | string" {
		t.Fatalf("pair_with = %q, %v", pair, err)
	}
	created, err := baml_sdk.GenericTestsGenericBoxNew(ctx, int64(9))
	if err != nil || created.Value != 9 {
		t.Fatalf("static new = %#v, %v", created, err)
	}
	echoed, err := baml_sdk.GenericTestsGenericBoxStaticEcho(ctx, int64(11))
	if err != nil || echoed != 11 {
		t.Fatalf("static class TypeVar = %#v, %v", echoed, err)
	}
	pairStatic, err := baml_sdk.GenericTestsGenericBoxStaticPair(ctx, int64(11), "text")
	if err != nil || pairStatic != "int | string" {
		t.Fatalf("static class + method TypeVars = %#v, %v", pairStatic, err)
	}
	static, err := baml_sdk.GenericTestsNamedStaticMake(ctx, int64(1), "x")
	if err != nil || static != "int | string" {
		t.Fatalf("named static = %q, %v", static, err)
	}
}

func Test_generic_union_input_and_engine_validation(t *testing.T) {
	ctx := context.Background()
	tag, err := baml_sdk.GenericTestsTagOrValue[int64](ctx, int64(5))
	if err != nil || tag != "int" {
		t.Fatalf("tag = %q, %v", tag, err)
	}
	_, err = baml_sdk.GenericTestsChoose(ctx, int64(1), int64(2))
	if err != nil {
		t.Fatal(err)
	}
	// The engine, not generated Go reflection, remains authoritative for BAML
	// assignability after T is bound.
	_, err = baml_sdk.GenericTestsTagOrValue[int64](ctx, true)
	if err == nil || !strings.Contains(err.Error(), "BAML") {
		t.Fatalf("invalid generic union argument = %v", err)
	}
}
