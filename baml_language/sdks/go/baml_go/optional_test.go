package baml_go

import (
	"testing"

	"github.com/boundaryml/baml-go/internal/cffi"
)

func TestOptionalEncodesNilAsExplicitNull(t *testing.T) {
	input := Optional[string](nil, String)
	if input.value == nil {
		t.Fatal("optional nil produced an uninitialized Input")
	}
	if input.value.Value != nil {
		t.Fatalf("optional nil encoded as %T, want BAML null", input.value.Value)
	}

	value := "present"
	input = Optional(&value, String)
	if got := input.value.GetStringValue(); got != value {
		t.Fatalf("optional value encoded as %q, want %q", got, value)
	}
}

func TestDecodeOptionalDistinguishesNullAndValue(t *testing.T) {
	null := Value{value: &cffi.BamlOutboundValue{}}
	decoded, err := DecodeOptional(null, Value.String)
	if err != nil {
		t.Fatal(err)
	}
	if decoded != nil {
		t.Fatalf("decoded null as %#v", decoded)
	}

	present := Value{value: &cffi.BamlOutboundValue{
		Value: &cffi.BamlOutboundValue_StringValue{StringValue: "present"},
	}}
	decoded, err = DecodeOptional(present, Value.String)
	if err != nil {
		t.Fatal(err)
	}
	if decoded == nil || *decoded != "present" {
		t.Fatalf("decoded value as %#v", decoded)
	}
}

func TestOptionalClassFieldMustStillBePresent(t *testing.T) {
	class := ClassValue{name: "user.Container", fields: map[string]Value{}}
	if _, err := DecodeOptionalField(class, "value", Value.String); err == nil {
		t.Fatal("missing optional field unexpectedly succeeded")
	}

	class.fields["value"] = Value{value: &cffi.BamlOutboundValue{}}
	decoded, err := DecodeOptionalField(class, "value", Value.String)
	if err != nil {
		t.Fatal(err)
	}
	if decoded != nil {
		t.Fatalf("explicit null decoded as %#v", decoded)
	}
}
