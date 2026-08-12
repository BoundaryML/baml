package sdk_test

import (
	"context"
	"reflect"
	"testing"

	"baml.local/sdk/baml_sdk"
)

// Direct port of python_pydantic2/type_shapes/roundtrip_tests/test_recursion.py.
func Test_round_trip_int_binary_tree(t *testing.T) {
	want := baml_sdk.RecursionIntBinaryTree{Value: 1, Left: &baml_sdk.RecursionIntBinaryTree{Value: 2}}
	got, err := baml_sdk.RecursionRoundTripIntBinaryTree(context.Background(), want)
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("tree = %#v, %v, want %#v", got, err, want)
	}
}

func Test_round_trip_mutual_recursion(t *testing.T) {
	ctx := context.Background()
	a := baml_sdk.RecursionA{B: &baml_sdk.RecursionB{}}
	b := baml_sdk.RecursionB{A: &baml_sdk.RecursionA{}}
	if got, err := baml_sdk.RecursionRoundTripA(ctx, a); err != nil || !reflect.DeepEqual(got, a) {
		t.Fatalf("A = %#v, %v", got, err)
	}
	if got, err := baml_sdk.RecursionRoundTripB(ctx, b); err != nil || !reflect.DeepEqual(got, b) {
		t.Fatalf("B = %#v, %v", got, err)
	}
}

func Test_round_trip_scct1_t2_t3(t *testing.T) {
	ctx := context.Background()
	t1 := baml_sdk.RecursionT1{Via2: &baml_sdk.RecursionT2{}}
	t2 := baml_sdk.RecursionT2{Via3: &baml_sdk.RecursionT3{}}
	t3 := baml_sdk.RecursionT3{}
	if got, err := baml_sdk.RecursionRoundTripT1(ctx, t1); err != nil || !reflect.DeepEqual(got, t1) {
		t.Fatalf("T1 = %#v, %v", got, err)
	}
	if got, err := baml_sdk.RecursionRoundTripT2(ctx, t2); err != nil || !reflect.DeepEqual(got, t2) {
		t.Fatalf("T2 = %#v, %v", got, err)
	}
	if got, err := baml_sdk.RecursionRoundTripT3(ctx, t3); err != nil || !reflect.DeepEqual(got, t3) {
		t.Fatalf("T3 = %#v, %v", got, err)
	}
}

func Test_round_trip_scct4_t5_t6(t *testing.T) {
	ctx := context.Background()
	t4 := baml_sdk.RecursionT4{Via5: &baml_sdk.RecursionT5{}}
	t5 := baml_sdk.RecursionT5{Via6: &baml_sdk.RecursionT6{}}
	t6 := baml_sdk.RecursionT6{}
	if got, err := baml_sdk.RecursionRoundTripT4(ctx, t4); err != nil || !reflect.DeepEqual(got, t4) {
		t.Fatalf("T4 = %#v, %v", got, err)
	}
	if got, err := baml_sdk.RecursionRoundTripT5(ctx, t5); err != nil || !reflect.DeepEqual(got, t5) {
		t.Fatalf("T5 = %#v, %v", got, err)
	}
	if got, err := baml_sdk.RecursionRoundTripT6(ctx, t6); err != nil || !reflect.DeepEqual(got, t6) {
		t.Fatalf("T6 = %#v, %v", got, err)
	}
}
