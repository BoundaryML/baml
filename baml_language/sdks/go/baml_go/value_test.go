package baml_go

import (
	"strings"
	"testing"

	"github.com/boundaryml/baml-go/internal/cffi"
)

func uint32Pointer(value uint32) *uint32 { return &value }

func outboundUnion(selfType BAMLType, index *uint32, payload *cffi.BamlOutboundValue) Value {
	return Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_UnionVariantValue{
		UnionVariantValue: &cffi.BamlValueUnionVariant{
			SelfType:            selfType.value,
			SelectedOptionIndex: index,
			Value:               payload,
		},
	}}}
}

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

func TestConcreteDecodersUnwrapUnionVariantEnvelopes(t *testing.T) {
	integer := PrimitiveBAMLType(IntType)
	wrapped := outboundUnion(
		UnionBAMLType(integer, PrimitiveBAMLType(StringType)),
		uint32Pointer(0),
		&cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_IntValue{IntValue: 7}},
	)
	got, err := wrapped.Int64()
	if err != nil || got != 7 {
		t.Fatalf("Int64() = %d, %v", got, err)
	}
}

func TestUnionVariantRejectsEnvelopeWithoutSelectedIndex(t *testing.T) {
	integer := PrimitiveBAMLType(IntType)
	wrapped := outboundUnion(
		UnionBAMLType(integer, PrimitiveBAMLType(StringType)),
		nil,
		&cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_IntValue{IntValue: 7}},
	)
	if _, _, err := wrapped.UnionVariant(); err == nil || !strings.Contains(err.Error(), "missing selected option index") {
		t.Fatalf("UnionVariant() error = %v", err)
	}
}

func TestUnionVariantRejectsInvalidCanonicalIndex(t *testing.T) {
	integer := PrimitiveBAMLType(IntType)
	stringType := PrimitiveBAMLType(StringType)
	payload := &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_IntValue{IntValue: 7}}
	tests := []struct {
		name    string
		value   Value
		message string
	}{
		{
			name:    "selected index out of range",
			value:   outboundUnion(UnionBAMLType(integer, stringType), uint32Pointer(2), payload),
			message: "outside self type",
		},
		{
			name:    "selected index cannot overflow host int",
			value:   outboundUnion(UnionBAMLType(integer, stringType), uint32Pointer(^uint32(0)), payload),
			message: "outside self type",
		},
		{
			name:    "non union self type",
			value:   outboundUnion(integer, uint32Pointer(0), payload),
			message: "not a union or optional",
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			if _, _, err := test.value.UnionVariant(); err == nil || !strings.Contains(err.Error(), test.message) {
				t.Fatalf("UnionVariant() error = %v, want %q", err, test.message)
			}
		})
	}
}

func TestUnionVariantRejectsMissingMetadata(t *testing.T) {
	integer := PrimitiveBAMLType(IntType)
	valid := func() *cffi.BamlValueUnionVariant {
		return &cffi.BamlValueUnionVariant{
			SelfType:            UnionBAMLType(integer, PrimitiveBAMLType(StringType)).value,
			SelectedOptionIndex: uint32Pointer(0),
			Value:               &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_IntValue{IntValue: 7}},
		}
	}
	tests := []struct {
		name    string
		mutate  func(*cffi.BamlValueUnionVariant)
		message string
	}{
		{"self type", func(value *cffi.BamlValueUnionVariant) { value.SelfType = nil }, "missing self type"},
		{"empty self descriptor", func(value *cffi.BamlValueUnionVariant) { value.SelfType = &cffi.BamlTy{} }, "missing self type"},
		{"selected index", func(value *cffi.BamlValueUnionVariant) { value.SelectedOptionIndex = nil }, "missing selected option index"},
		{"payload", func(value *cffi.BamlValueUnionVariant) { value.Value = nil }, "empty value"},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			variant := valid()
			test.mutate(variant)
			wrapped := Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_UnionVariantValue{UnionVariantValue: variant}}}
			if _, _, err := wrapped.UnionVariant(); err == nil || !strings.Contains(err.Error(), test.message) {
				t.Fatalf("UnionVariant() error = %v, want %q", err, test.message)
			}
		})
	}
}

func TestUnionVariantValidatesCanonicalOptionalAndExplicitUnionOrdering(t *testing.T) {
	integer := PrimitiveBAMLType(IntType)
	nullType := PrimitiveBAMLType(NullType)
	nullPayload := &cffi.BamlOutboundValue{}

	optional := outboundUnion(OptionalBAMLType(integer), uint32Pointer(1), nullPayload)
	if selected, _, err := optional.UnionVariant(); err != nil || !selected.Equal(nullType) {
		t.Fatalf("optional null arm = %#v, %v", selected, err)
	}
	inner := outboundUnion(OptionalBAMLType(integer), uint32Pointer(0), &cffi.BamlOutboundValue{
		Value: &cffi.BamlOutboundValue_IntValue{IntValue: 7},
	})
	if selected, _, err := inner.UnionVariant(); err != nil || !selected.Equal(integer) {
		t.Fatalf("optional inner arm = %#v, %v", selected, err)
	}

	explicitNullFirst := outboundUnion(UnionBAMLType(nullType, integer), uint32Pointer(0), nullPayload)
	if selected, _, err := explicitNullFirst.UnionVariant(); err != nil || !selected.Equal(nullType) {
		t.Fatalf("explicit null-first arm = %#v, %v", selected, err)
	}
}

func TestConcreteDecodersRejectEmptyUnionVariantEnvelope(t *testing.T) {
	wrapped := Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_UnionVariantValue{
		UnionVariantValue: &cffi.BamlValueUnionVariant{},
	}}}
	if _, err := wrapped.Int64(); err == nil {
		t.Fatal("Int64() unexpectedly accepted an empty union variant")
	}
}
