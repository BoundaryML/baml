package sdk_test

import (
	"bytes"
	"context"
	"math/big"
	"reflect"
	"testing"

	"baml.local/sdk/baml_sdk"
	"github.com/boundaryml/baml/sdks/go/baml_go"
)

// These assignments are intentionally compile-only. They pin the exact public
// Go signature generated for every primitive and primitive literal currently
// in scope, independently of runtime round-trip coverage.
var (
	_ func(context.Context) (int64, error)                      = baml_sdk.PrimitivesReturnInt
	_ func(context.Context) (*big.Int, error)                   = baml_sdk.PrimitivesReturnBigint
	_ func(context.Context) (float64, error)                    = baml_sdk.PrimitivesReturnFloat
	_ func(context.Context) (string, error)                     = baml_sdk.PrimitivesReturnString
	_ func(context.Context) (bool, error)                       = baml_sdk.PrimitivesReturnBool
	_ func(context.Context) (baml_go.Null, error)               = baml_sdk.PrimitivesReturnNull
	_ func(context.Context, int64) (int64, error)               = baml_sdk.PrimitivesRoundTripInt
	_ func(context.Context, *big.Int) (*big.Int, error)         = baml_sdk.PrimitivesRoundTripBigint
	_ func(context.Context, float64) (float64, error)           = baml_sdk.PrimitivesRoundTripFloat
	_ func(context.Context, string) (string, error)             = baml_sdk.PrimitivesRoundTripString
	_ func(context.Context, bool) (bool, error)                 = baml_sdk.PrimitivesRoundTripBool
	_ func(context.Context, baml_go.Null) (baml_go.Null, error) = baml_sdk.PrimitivesRoundTripNull
	_ func(context.Context, []byte) ([]byte, error)             = baml_sdk.PrimitivesRoundTripUint8Array

	_ func(context.Context) (int64, error)          = baml_sdk.LiteralsReturnLiteral42
	_ func(context.Context) (int64, error)          = baml_sdk.LiteralsReturnLiteralNegOne
	_ func(context.Context) (string, error)         = baml_sdk.LiteralsReturnLiteralDraft
	_ func(context.Context) (string, error)         = baml_sdk.LiteralsReturnLiteralEscaped
	_ func(context.Context) (bool, error)           = baml_sdk.LiteralsReturnLiteralTrue
	_ func(context.Context) (bool, error)           = baml_sdk.LiteralsReturnLiteralFalse
	_ func(context.Context, int64) (int64, error)   = baml_sdk.LiteralsRoundTripLiteral42
	_ func(context.Context, string) (string, error) = baml_sdk.LiteralsRoundTripLiteralDraft
	_ func(context.Context, string) (string, error) = baml_sdk.LiteralsRoundTripLiteralEscaped
	_ func(context.Context, bool) (bool, error)     = baml_sdk.LiteralsRoundTripLiteralTrue
	_ func(context.Context, bool) (bool, error)     = baml_sdk.LiteralsRoundTripLiteralFalse

	_ func(context.Context) error = baml_sdk.VoidNoOp

	_ func(context.Context, string) (string, error)                                                                                   = baml_sdk.GoCodegenLeftEcho
	_ func(context.Context, string) (string, error)                                                                                   = baml_sdk.GoCodegenRightEcho
	_ func(context.Context, string, int64, bool, string, string, string, string, string, string, string) (string, error)              = baml_sdk.GoCodegenNestedReservedArgs
	_ func(context.Context, *big.Int) (*big.Int, error)                                                                               = baml_sdk.GoCodegenPrimitiveEdgesRoundTripLiteralBigint
	_ func(context.Context, *big.Int, int64, float64, []byte, string, string) (*big.Int, error)                                       = baml_sdk.GoCodegenPrimitiveEdgesReservedTypeNames
	_ func(context.Context, baml_sdk.GoCodegenPrimitiveEdgesWirePrimitives) (baml_sdk.GoCodegenPrimitiveEdgesWirePrimitives, error)   = baml_sdk.GoCodegenPrimitiveEdgesRoundTripWirePrimitives
	_ func(context.Context, int64) (baml_sdk.ClassRefsOuter, error)                                                                   = baml_sdk.ClassRefsMakeOuter
	_ func(context.Context, baml_sdk.ClassRefsOuter) (baml_sdk.ClassRefsOuter, error)                                                 = baml_sdk.ClassRefsRoundTripOuter
	_ func(context.Context, *string) (*string, error)                                                                                 = baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalString
	_ func(context.Context, *int64) (*int64, error)                                                                                   = baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalInt
	_ func(context.Context, *big.Int) (*big.Int, error)                                                                               = baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalBigint
	_ func(context.Context, *float64) (*float64, error)                                                                               = baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalFloat
	_ func(context.Context, *bool) (*bool, error)                                                                                     = baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalBool
	_ func(context.Context, *[]byte) (*[]byte, error)                                                                                 = baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalBytes
	_ func(context.Context, *baml_sdk.GoCodegenPrimitiveEdgesWirePrimitives) (*baml_sdk.GoCodegenPrimitiveEdgesWirePrimitives, error) = baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalClass
	_ func(context.Context, baml_sdk.GoCodegenPrimitiveEdgesNullableWire) (baml_sdk.GoCodegenPrimitiveEdgesNullableWire, error)       = baml_sdk.GoCodegenPrimitiveEdgesRoundTripNullableWire
)

var (
	_ = baml_sdk.PrimitivesPrimitives{
		IntField:        1,
		FloatField:      1.5,
		StringField:     "value",
		BoolField:       true,
		NullField:       baml_go.Null{},
		Uint8arrayField: []byte{1},
	}
	_ = baml_sdk.GoCodegenPrimitiveEdgesPrimitiveHolder{
		BigValue: big.NewInt(1),
	}
	_ = baml_sdk.ClassRefsOuter{
		Inner: baml_sdk.ClassRefsInner{Value: 1},
	}
	_ = baml_sdk.RecursionIntBinaryTree{
		Left: &baml_sdk.RecursionIntBinaryTree{},
	}
	_ = baml_sdk.RecursionA{
		B: &baml_sdk.RecursionB{},
	}
)

func TestPrimitiveClassRoundTrip(t *testing.T) {
	want := baml_sdk.GoCodegenPrimitiveEdgesWirePrimitives{
		StringValue: "wire",
		IntValue:    42,
		BigintValue: new(big.Int).Lsh(big.NewInt(1), 80),
		FloatValue:  3.5,
		BoolValue:   true,
		NullValue:   baml_go.Null{},
		BytesValue:  []byte{0, 1, 127, 255},
	}
	got, err := baml_sdk.GoCodegenPrimitiveEdgesRoundTripWirePrimitives(context.Background(), want)
	if err != nil {
		t.Fatal(err)
	}
	if got.StringValue != want.StringValue ||
		got.IntValue != want.IntValue ||
		got.BigintValue.Cmp(want.BigintValue) != 0 ||
		got.FloatValue != want.FloatValue ||
		got.BoolValue != want.BoolValue ||
		!bytes.Equal(got.BytesValue, want.BytesValue) {
		t.Fatalf("round trip = %#v, want %#v", got, want)
	}
}

func TestNestedClassRoundTrip(t *testing.T) {
	want := baml_sdk.ClassRefsOuter{Inner: baml_sdk.ClassRefsInner{Value: 42}}
	got, err := baml_sdk.ClassRefsRoundTripOuter(context.Background(), want)
	if err != nil {
		t.Fatal(err)
	}
	if got != want {
		t.Fatalf("round trip = %#v, want %#v", got, want)
	}

	made, err := baml_sdk.ClassRefsMakeOuter(context.Background(), 73)
	if err != nil {
		t.Fatal(err)
	}
	if made.Inner.Value != 73 {
		t.Fatalf("MakeOuter() = %#v, want nested value 73", made)
	}
}

func TestOptionalTopLevelRoundTrips(t *testing.T) {
	stringValue := "value"
	intValue := int64(42)
	bigintValue := new(big.Int).Lsh(big.NewInt(1), 80)
	floatValue := 3.5
	boolValue := true
	bytesValue := []byte{0, 1, 255}
	classValue := baml_sdk.GoCodegenPrimitiveEdgesWirePrimitives{
		StringValue: "nested",
		IntValue:    7,
		BigintValue: big.NewInt(99),
		FloatValue:  1.5,
		BoolValue:   true,
		NullValue:   baml_go.Null{},
		BytesValue:  []byte{3, 2, 1},
	}

	assertOptionalRoundTrip(t, baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalString, &stringValue)
	assertOptionalRoundTrip(t, baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalInt, &intValue)
	assertOptionalRoundTrip(t, baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalBigint, bigintValue)
	assertOptionalRoundTrip(t, baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalFloat, &floatValue)
	assertOptionalRoundTrip(t, baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalBool, &boolValue)
	assertOptionalRoundTrip(t, baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalBytes, &bytesValue)
	assertOptionalRoundTrip(t, baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalClass, &classValue)
}

func TestNullableClassFieldsRoundTrip(t *testing.T) {
	empty, err := baml_sdk.GoCodegenPrimitiveEdgesRoundTripNullableWire(
		context.Background(),
		baml_sdk.GoCodegenPrimitiveEdgesNullableWire{},
	)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(empty, baml_sdk.GoCodegenPrimitiveEdgesNullableWire{}) {
		t.Fatalf("null fields round trip = %#v", empty)
	}

	stringValue := "value"
	intValue := int64(42)
	bigintValue := big.NewInt(123456789)
	floatValue := 3.5
	boolValue := true
	bytesValue := []byte{9, 8, 7}
	classValue := baml_sdk.GoCodegenPrimitiveEdgesWirePrimitives{
		StringValue: "nested",
		IntValue:    7,
		BigintValue: big.NewInt(99),
		FloatValue:  1.5,
		BoolValue:   true,
		NullValue:   baml_go.Null{},
		BytesValue:  []byte{3, 2, 1},
	}
	want := baml_sdk.GoCodegenPrimitiveEdgesNullableWire{
		StringValue: &stringValue,
		IntValue:    &intValue,
		BigintValue: bigintValue,
		FloatValue:  &floatValue,
		BoolValue:   &boolValue,
		BytesValue:  &bytesValue,
		ClassValue:  &classValue,
	}
	got, err := baml_sdk.GoCodegenPrimitiveEdgesRoundTripNullableWire(context.Background(), want)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("non-null fields round trip = %#v, want %#v", got, want)
	}
}

func TestNullableRecursiveClassesRoundTrip(t *testing.T) {
	tree := baml_sdk.RecursionIntBinaryTree{
		Value: 1,
		Left:  &baml_sdk.RecursionIntBinaryTree{Value: 2},
		Right: &baml_sdk.RecursionIntBinaryTree{
			Value: 3,
			Left:  &baml_sdk.RecursionIntBinaryTree{Value: 4},
		},
	}
	gotTree, err := baml_sdk.RecursionRoundTripIntBinaryTree(context.Background(), tree)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(gotTree, tree) {
		t.Fatalf("tree round trip = %#v, want %#v", gotTree, tree)
	}

	mutual := baml_sdk.RecursionA{B: &baml_sdk.RecursionB{}}
	gotMutual, err := baml_sdk.RecursionRoundTripA(context.Background(), mutual)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(gotMutual, mutual) {
		t.Fatalf("mutual round trip = %#v, want %#v", gotMutual, mutual)
	}
}

func assertOptionalRoundTrip[T any](
	t *testing.T,
	roundTrip func(context.Context, *T) (*T, error),
	want *T,
) {
	t.Helper()
	for _, value := range []*T{nil, want} {
		got, err := roundTrip(context.Background(), value)
		if err != nil {
			t.Fatal(err)
		}
		if !reflect.DeepEqual(got, value) {
			t.Fatalf("round trip = %#v, want %#v", got, value)
		}
	}
}
