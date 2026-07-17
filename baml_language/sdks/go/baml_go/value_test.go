package baml_go

import (
	"testing"

	"github.com/boundaryml/baml-go/internal/cffi"
)

func TestFloat64AcceptsRuntimeAndLiteralWireForms(t *testing.T) {
	tests := []struct {
		name  string
		value *cffi.BamlOutboundValue
		want  float64
	}{
		{
			name: "runtime float",
			value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_FloatValue{
				FloatValue: 3.14,
			}},
			want: 3.14,
		},
		{
			name: "literal float source text",
			value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_LiteralValue{
				LiteralValue: &cffi.BamlLiteralValue{Literal: &cffi.BamlLiteralValue_FloatValue{
					FloatValue: "6.022e23",
				}},
			}},
			want: 6.022e23,
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			got, err := (Value{value: test.value}).Float64()
			if err != nil {
				t.Fatal(err)
			}
			if got != test.want {
				t.Fatalf("Float64() = %v, want %v", got, test.want)
			}
		})
	}
}

func TestFloat64RejectsInvalidLiteralSourceText(t *testing.T) {
	value := Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_LiteralValue{
		LiteralValue: &cffi.BamlLiteralValue{Literal: &cffi.BamlLiteralValue_FloatValue{
			FloatValue: "not-a-float",
		}},
	}}}

	if _, err := value.Float64(); err == nil {
		t.Fatal("invalid float literal unexpectedly decoded")
	}
}
