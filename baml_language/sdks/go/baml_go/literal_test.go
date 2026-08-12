package baml_go

import (
	"math"
	"testing"
)

func TestMustFloatLiteralPreservesSignedZeroAndDecimalSyntax(t *testing.T) {
	negativeZero := MustFloatLiteral("-0.0")
	if negativeZero != 0 || !math.Signbit(negativeZero) {
		t.Fatalf("MustFloatLiteral(-0.0) bits = %#x", math.Float64bits(negativeZero))
	}
	negativeUnderflow := MustFloatLiteral("-1e-9999")
	if negativeUnderflow != 0 || !math.Signbit(negativeUnderflow) {
		t.Fatalf("MustFloatLiteral(-1e-9999) bits = %#x", math.Float64bits(negativeUnderflow))
	}
	tests := []struct {
		source string
		want   float64
	}{
		{"6.022e23", 6.022e23},
		{"-2.5e-4", -2.5e-4},
		{"1.2345678901234567", 1.2345678901234567},
	}
	for _, test := range tests {
		if got := MustFloatLiteral(test.source); math.Float64bits(got) != math.Float64bits(test.want) {
			t.Fatalf("MustFloatLiteral(%q) = %.17g (%#x), want %.17g (%#x)", test.source, got, math.Float64bits(got), test.want, math.Float64bits(test.want))
		}
	}
}

func TestMustFloatLiteralRejectsNonBAMLNonFiniteSpellings(t *testing.T) {
	for _, source := range []string{"NaN", "+Inf", "-Inf", "1e9999"} {
		t.Run(source, func(t *testing.T) {
			defer func() {
				if recover() == nil {
					t.Fatalf("MustFloatLiteral(%q) did not panic", source)
				}
			}()
			_ = MustFloatLiteral(source)
		})
	}
}

func TestMustBigIntLiteralPreservesArbitraryPrecision(t *testing.T) {
	if got := MustBigIntLiteral("123456789012345678901234567890"); got.String() != "123456789012345678901234567890" {
		t.Fatalf("MustBigIntLiteral() = %s", got)
	}
}
