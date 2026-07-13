package sdk_test

import (
	"context"
	"math/big"

	"baml.local/sdk/baml_sdk/go_codegen/left"
	"baml.local/sdk/baml_sdk/go_codegen/nested"
	"baml.local/sdk/baml_sdk/go_codegen/right"
	"baml.local/sdk/baml_sdk/literals"
	"baml.local/sdk/baml_sdk/primitives"
	voidsdk "baml.local/sdk/baml_sdk/void"
	"github.com/boundaryml/baml/sdks/go/baml_go"
)

// These assignments are intentionally compile-only. They pin the exact public
// Go signature generated for every primitive and primitive literal currently
// in scope, independently of runtime round-trip coverage.
var (
	_ func(context.Context) (int64, error)                      = primitives.ReturnInt
	_ func(context.Context) (*big.Int, error)                   = primitives.ReturnBigint
	_ func(context.Context) (float64, error)                    = primitives.ReturnFloat
	_ func(context.Context) (string, error)                     = primitives.ReturnString
	_ func(context.Context) (bool, error)                       = primitives.ReturnBool
	_ func(context.Context) (baml_go.Null, error)               = primitives.ReturnNull
	_ func(context.Context, int64) (int64, error)               = primitives.RoundTripInt
	_ func(context.Context, *big.Int) (*big.Int, error)         = primitives.RoundTripBigint
	_ func(context.Context, float64) (float64, error)           = primitives.RoundTripFloat
	_ func(context.Context, string) (string, error)             = primitives.RoundTripString
	_ func(context.Context, bool) (bool, error)                 = primitives.RoundTripBool
	_ func(context.Context, baml_go.Null) (baml_go.Null, error) = primitives.RoundTripNull
	_ func(context.Context, []byte) ([]byte, error)             = primitives.RoundTripUint8Array

	_ func(context.Context) (int64, error)          = literals.ReturnLiteral42
	_ func(context.Context) (int64, error)          = literals.ReturnLiteralNegOne
	_ func(context.Context) (string, error)         = literals.ReturnLiteralDraft
	_ func(context.Context) (string, error)         = literals.ReturnLiteralEscaped
	_ func(context.Context) (bool, error)           = literals.ReturnLiteralTrue
	_ func(context.Context) (bool, error)           = literals.ReturnLiteralFalse
	_ func(context.Context, int64) (int64, error)   = literals.RoundTripLiteral42
	_ func(context.Context, string) (string, error) = literals.RoundTripLiteralDraft
	_ func(context.Context, string) (string, error) = literals.RoundTripLiteralEscaped
	_ func(context.Context, bool) (bool, error)     = literals.RoundTripLiteralTrue
	_ func(context.Context, bool) (bool, error)     = literals.RoundTripLiteralFalse

	_ func(context.Context) error = voidsdk.NoOp

	_ func(context.Context, string) (string, error)              = left.Echo
	_ func(context.Context, string) (string, error)              = right.Echo
	_ func(context.Context, string, int64, bool) (string, error) = nested.ReservedArgs
)
