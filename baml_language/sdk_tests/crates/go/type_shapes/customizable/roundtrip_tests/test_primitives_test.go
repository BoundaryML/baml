package sdk_test

import (
	"bytes"
	"context"
	"reflect"
	"testing"

	"baml.local/sdk/baml_sdk"
	"github.com/boundaryml/baml-go"
)

func TestReturnInt(t *testing.T) {
	got, err := baml_sdk.PrimitivesReturnInt(context.Background())
	if err != nil || got != 42 {
		t.Fatalf("got %v, %v", got, err)
	}
}
func TestReturnFloat(t *testing.T) {
	got, err := baml_sdk.PrimitivesReturnFloat(context.Background())
	if err != nil || got != 3.14 {
		t.Fatalf("got %v, %v", got, err)
	}
}
func TestReturnString(t *testing.T) {
	got, err := baml_sdk.PrimitivesReturnString(context.Background())
	if err != nil || got != "hello" {
		t.Fatalf("got %q, %v", got, err)
	}
}
func TestReturnBool(t *testing.T) {
	got, err := baml_sdk.PrimitivesReturnBool(context.Background())
	if err != nil || !got {
		t.Fatalf("got %v, %v", got, err)
	}
}
func TestReturnNull(t *testing.T) {
	if _, err := baml_sdk.PrimitivesReturnNull(context.Background()); err != nil {
		t.Fatal(err)
	}
}

func TestRoundTripInt(t *testing.T) {
	got, err := baml_sdk.PrimitivesRoundTripInt(context.Background(), 7)
	if err != nil || got != 7 {
		t.Fatalf("got %v, %v", got, err)
	}
}
func TestRoundTripFloat(t *testing.T) {
	got, err := baml_sdk.PrimitivesRoundTripFloat(context.Background(), 2.5)
	if err != nil || got != 2.5 {
		t.Fatalf("got %v, %v", got, err)
	}
}
func TestRoundTripFloatAcceptsIntConstant(t *testing.T) {
	got, err := baml_sdk.PrimitivesRoundTripFloat(context.Background(), 7)
	if err != nil || got != float64(7) {
		t.Fatalf("got %T(%v), %v", got, got, err)
	}
}
func TestRoundTripString(t *testing.T) {
	got, err := baml_sdk.PrimitivesRoundTripString(context.Background(), "hi")
	if err != nil || got != "hi" {
		t.Fatalf("got %q, %v", got, err)
	}
}
func TestRoundTripBool(t *testing.T) {
	got, err := baml_sdk.PrimitivesRoundTripBool(context.Background(), false)
	if err != nil || got {
		t.Fatalf("got %v, %v", got, err)
	}
}
func TestRoundTripNull(t *testing.T) {
	if _, err := baml_sdk.PrimitivesRoundTripNull(context.Background(), baml_go.Null{}); err != nil {
		t.Fatal(err)
	}
}
func TestRoundTripUint8Array(t *testing.T) {
	want := []byte{0, 1, 2}
	got, err := baml_sdk.PrimitivesRoundTripUint8Array(context.Background(), want)
	if err != nil || !bytes.Equal(got, want) {
		t.Fatalf("got %v, %v", got, err)
	}
}

func TestRoundTripPrimitives(t *testing.T) {
	want := baml_sdk.PrimitivesPrimitives{IntField: 1, FloatField: 1.5, StringField: "s", BoolField: true, NullField: baml_go.Null{}, Uint8arrayField: []byte("ab")}
	got, err := baml_sdk.PrimitivesRoundTripPrimitives(context.Background(), want)
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("got %#v, %v, want %#v", got, err, want)
	}
}

func TestRoundTripPrimitivesFloatFieldAcceptsIntConstant(t *testing.T) {
	want := baml_sdk.PrimitivesPrimitives{IntField: 1, FloatField: 2, StringField: "s", BoolField: true, NullField: baml_go.Null{}, Uint8arrayField: []byte("ab")}
	got, err := baml_sdk.PrimitivesRoundTripPrimitives(context.Background(), want)
	if err != nil || got.FloatField != float64(2) {
		t.Fatalf("got %T(%v), %v", got.FloatField, got.FloatField, err)
	}
}
