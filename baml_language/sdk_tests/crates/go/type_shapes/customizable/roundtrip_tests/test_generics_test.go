package sdk_test

import (
	"context"
	"reflect"
	"testing"

	"baml.local/sdk/baml_sdk"
)

func TestRoundTripWrapperInt(t *testing.T) {
	want := baml_sdk.GenericsWrapper[int64]{Value: 5}
	got, err := baml_sdk.GenericsRoundTripWrapperInt(context.Background(), want)
	if err != nil || got != want {
		t.Fatalf("wrapper = %#v, %v, want %#v", got, err, want)
	}
}

func TestRoundTripGenericLinkedListInt(t *testing.T) {
	want := baml_sdk.GenericsGenericLinkedList[int64]{
		Value: 1,
		Next:  &baml_sdk.GenericsGenericLinkedList[int64]{Value: 2},
	}
	got, err := baml_sdk.GenericsRoundTripGenericLinkedListInt(context.Background(), want)
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("linked list = %#v, %v, want %#v", got, err, want)
	}
}

func TestRoundTripGenericBinaryTreeInt(t *testing.T) {
	want := baml_sdk.GenericsGenericBinaryTree[int64]{Value: 1}
	got, err := baml_sdk.GenericsRoundTripGenericBinaryTreeInt(context.Background(), want)
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("binary tree = %#v, %v, want %#v", got, err, want)
	}
}

func TestRoundTripBoxInt(t *testing.T) {
	want := baml_sdk.GenericsBox[int64]{
		Value:   3,
		Wrapped: baml_sdk.GenericsWrapper[int64]{Value: 4},
	}
	got, err := baml_sdk.GenericsRoundTripBoxInt(context.Background(), want)
	if err != nil || got != want {
		t.Fatalf("box = %#v, %v, want %#v", got, err, want)
	}
}

func TestRoundTripNestedGenerics(t *testing.T) {
	want := baml_sdk.GenericsNestedGenerics{
		Ww: baml_sdk.GenericsWrapper[baml_sdk.GenericsWrapper[int64]]{
			Value: baml_sdk.GenericsWrapper[int64]{Value: 1},
		},
		Wl: baml_sdk.GenericsWrapper[[]int64]{Value: []int64{1, 2}},
		Wr: baml_sdk.GenericsWrapper[baml_sdk.GenericsGenericLinkedList[int64]]{
			Value: baml_sdk.GenericsGenericLinkedList[int64]{Value: 9},
		},
	}
	got, err := baml_sdk.GenericsRoundTripNestedGenerics(context.Background(), want)
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("nested generics = %#v, %v, want %#v", got, err, want)
	}
}

func TestRoundTripDifferingInstantiation(t *testing.T) {
	want := baml_sdk.GenericsDifferingInstantiation{
		List: baml_sdk.GenericsGenericLinkedList[baml_sdk.GenericsWrapper[int64]]{
			Value: baml_sdk.GenericsWrapper[int64]{Value: 1},
		},
	}
	got, err := baml_sdk.GenericsRoundTripDifferingInstantiation(context.Background(), want)
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("differing instantiation = %#v, %v, want %#v", got, err, want)
	}
}
