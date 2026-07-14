package sdk_test

import (
	"bytes"
	"context"
	"math/big"
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

	_ func(context.Context, string) (string, error)              = baml_sdk.GoCodegenLeftEcho
	_ func(context.Context, string) (string, error)              = baml_sdk.GoCodegenRightEcho
	_ func(context.Context, string, int64, bool, string, string, string, string, string, string, string) (string, error) = baml_sdk.GoCodegenNestedReservedArgs
	_ func(context.Context, *big.Int) (*big.Int, error) = baml_sdk.GoCodegenPrimitiveEdgesRoundTripLiteralBigint
	_ func(context.Context, *big.Int, int64, float64, []byte, string, string) (*big.Int, error) = baml_sdk.GoCodegenPrimitiveEdgesReservedTypeNames
	_ func(context.Context, baml_sdk.GoCodegenPrimitiveEdgesWirePrimitives) (baml_sdk.GoCodegenPrimitiveEdgesWirePrimitives, error) = baml_sdk.GoCodegenPrimitiveEdgesRoundTripWirePrimitives
)

var (
	_ = baml_sdk.PrimitivesPrimitives{
		IntField: 1,
		FloatField: 1.5,
		StringField: "value",
		BoolField: true,
		NullField: baml_go.Null{},
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
		IntValue: 42,
		BigintValue: new(big.Int).Lsh(big.NewInt(1), 80),
		FloatValue: 3.5,
		BoolValue: true,
		NullValue: baml_go.Null{},
		BytesValue: []byte{0, 1, 127, 255},
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
