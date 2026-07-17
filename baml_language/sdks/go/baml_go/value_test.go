package baml_go

import (
	"strings"
	"testing"

	"github.com/boundaryml/baml-go/internal/cffi"
)

func uint32Pointer(value uint32) *uint32 { return &value }

func outboundUnion(selfType, selectedType BAMLType, index *uint32, payload *cffi.BamlOutboundValue) Value {
	return Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_UnionVariantValue{
		UnionVariantValue: &cffi.BamlValueUnionVariant{
			SelfType:            selfType.value,
			SelectedType:        selectedType.value,
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
		integer,
		uint32Pointer(0),
		&cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_IntValue{IntValue: 7}},
	)
	got, err := wrapped.Int64()
	if err != nil || got != 7 {
		t.Fatalf("Int64() = %d, %v", got, err)
	}
}

func TestUnionVariantAcceptsLegacyEnvelopeWithoutSelectedIndex(t *testing.T) {
	integer := PrimitiveBAMLType(IntType)
	wrapped := outboundUnion(
		UnionBAMLType(integer, PrimitiveBAMLType(StringType)),
		integer,
		nil,
		&cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_IntValue{IntValue: 7}},
	)
	selected, payload, err := wrapped.UnionVariant()
	if err != nil || !selected.Equal(integer) {
		t.Fatalf("UnionVariant() selected = %#v, %v", selected, err)
	}
	if got, err := payload.Int64(); err != nil || got != 7 {
		t.Fatalf("payload.Int64() = %d, %v", got, err)
	}
}

func TestUnionVariantRejectsContradictoryMetadata(t *testing.T) {
	integer := PrimitiveBAMLType(IntType)
	stringType := PrimitiveBAMLType(StringType)
	boolean := PrimitiveBAMLType(BoolType)
	payload := &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_IntValue{IntValue: 7}}
	tests := []struct {
		name    string
		value   Value
		message string
	}{
		{
			name:    "selected type absent from self type",
			value:   outboundUnion(UnionBAMLType(integer, stringType), boolean, uint32Pointer(0), payload),
			message: "not a member",
		},
		{
			name:    "selected index out of range",
			value:   outboundUnion(UnionBAMLType(integer, stringType), integer, uint32Pointer(2), payload),
			message: "outside self type",
		},
		{
			name:    "selected index cannot overflow host int",
			value:   outboundUnion(UnionBAMLType(integer, stringType), integer, uint32Pointer(^uint32(0)), payload),
			message: "outside self type",
		},
		{
			name:    "selected index disagrees with type",
			value:   outboundUnion(UnionBAMLType(integer, stringType), integer, uint32Pointer(1), payload),
			message: "disagrees with selected type",
		},
		{
			name:    "non union self type",
			value:   outboundUnion(integer, integer, uint32Pointer(0), payload),
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
			SelfType:     UnionBAMLType(integer, PrimitiveBAMLType(StringType)).value,
			SelectedType: integer.value,
			Value:        &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_IntValue{IntValue: 7}},
		}
	}
	tests := []struct {
		name    string
		mutate  func(*cffi.BamlValueUnionVariant)
		message string
	}{
		{"self type", func(value *cffi.BamlValueUnionVariant) { value.SelfType = nil }, "missing self type"},
		{"empty self descriptor", func(value *cffi.BamlValueUnionVariant) { value.SelfType = &cffi.BamlTy{} }, "missing self type"},
		{"selected type", func(value *cffi.BamlValueUnionVariant) { value.SelectedType = nil }, "missing selected type"},
		{"empty selected descriptor", func(value *cffi.BamlValueUnionVariant) { value.SelectedType = &cffi.BamlTy{} }, "missing selected type"},
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

	optional := outboundUnion(OptionalBAMLType(integer), nullType, uint32Pointer(1), nullPayload)
	if selected, _, err := optional.UnionVariant(); err != nil || !selected.Equal(nullType) {
		t.Fatalf("optional null arm = %#v, %v", selected, err)
	}
	wrongOptionalIndex := outboundUnion(OptionalBAMLType(integer), nullType, uint32Pointer(0), nullPayload)
	if _, _, err := wrongOptionalIndex.UnionVariant(); err == nil || !strings.Contains(err.Error(), "disagrees") {
		t.Fatalf("optional ordering mismatch error = %v", err)
	}

	explicitNullFirst := outboundUnion(UnionBAMLType(nullType, integer), nullType, uint32Pointer(0), nullPayload)
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
