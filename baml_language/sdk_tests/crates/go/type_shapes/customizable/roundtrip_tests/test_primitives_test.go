package sdk_test

import (
	"bytes"
	"context"
	"reflect"
	"testing"

	"baml.local/sdk/baml_sdk"
	"github.com/boundaryml/baml-go"
)

func Test_return_int(t *testing.T) {
	got, err := baml_sdk.PrimitivesReturnInt(context.Background())
	if err != nil || got != 42 {
		t.Fatalf("got %v, %v", got, err)
	}
}
func Test_return_float(t *testing.T) {
	got, err := baml_sdk.PrimitivesReturnFloat(context.Background())
	if err != nil || got != 3.14 {
		t.Fatalf("got %v, %v", got, err)
	}
}
func Test_return_string(t *testing.T) {
	got, err := baml_sdk.PrimitivesReturnString(context.Background())
	if err != nil || got != "hello" {
		t.Fatalf("got %q, %v", got, err)
	}
}
func Test_return_bool(t *testing.T) {
	got, err := baml_sdk.PrimitivesReturnBool(context.Background())
	if err != nil || !got {
		t.Fatalf("got %v, %v", got, err)
	}
}
func Test_return_null(t *testing.T) {
	if _, err := baml_sdk.PrimitivesReturnNull(context.Background()); err != nil {
		t.Fatal(err)
	}
}

func Test_round_trip_int(t *testing.T) {
	got, err := baml_sdk.PrimitivesRoundTripInt(context.Background(), 7)
	if err != nil || got != 7 {
		t.Fatalf("got %v, %v", got, err)
	}
}
func Test_round_trip_float(t *testing.T) {
	got, err := baml_sdk.PrimitivesRoundTripFloat(context.Background(), 2.5)
	if err != nil || got != 2.5 {
		t.Fatalf("got %v, %v", got, err)
	}
}
func Test_round_trip_float_accepts_int_constant(t *testing.T) {
	got, err := baml_sdk.PrimitivesRoundTripFloat(context.Background(), 7)
	if err != nil || got != float64(7) {
		t.Fatalf("got %T(%v), %v", got, got, err)
	}
}
func Test_round_trip_string(t *testing.T) {
	got, err := baml_sdk.PrimitivesRoundTripString(context.Background(), "hi")
	if err != nil || got != "hi" {
		t.Fatalf("got %q, %v", got, err)
	}
}
func Test_round_trip_bool(t *testing.T) {
	got, err := baml_sdk.PrimitivesRoundTripBool(context.Background(), false)
	if err != nil || got {
		t.Fatalf("got %v, %v", got, err)
	}
}
func Test_round_trip_null(t *testing.T) {
	if _, err := baml_sdk.PrimitivesRoundTripNull(context.Background(), baml_go.Null{}); err != nil {
		t.Fatal(err)
	}
}
func Test_round_trip_uint8_array(t *testing.T) {
	want := []byte{0, 1, 2}
	got, err := baml_sdk.PrimitivesRoundTripUint8Array(context.Background(), want)
	if err != nil || !bytes.Equal(got, want) {
		t.Fatalf("got %v, %v", got, err)
	}
}

func Test_round_trip_primitives(t *testing.T) {
	want := baml_sdk.PrimitivesPrimitives{IntField: 1, FloatField: 1.5, StringField: "s", BoolField: true, NullField: baml_go.Null{}, Uint8arrayField: []byte("ab")}
	got, err := baml_sdk.PrimitivesRoundTripPrimitives(context.Background(), want)
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("got %#v, %v, want %#v", got, err, want)
	}
}

func Test_round_trip_primitives_float_field_accepts_int_constant(t *testing.T) {
	want := baml_sdk.PrimitivesPrimitives{IntField: 1, FloatField: 2, StringField: "s", BoolField: true, NullField: baml_go.Null{}, Uint8arrayField: []byte("ab")}
	got, err := baml_sdk.PrimitivesRoundTripPrimitives(context.Background(), want)
	if err != nil || got.FloatField != float64(2) {
		t.Fatalf("got %T(%v), %v", got.FloatField, got.FloatField, err)
	}
}
