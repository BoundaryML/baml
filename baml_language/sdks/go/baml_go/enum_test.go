package baml_go

import (
	"strings"
	"testing"

	"github.com/boundaryml/baml-go/internal/cffi"
)

func TestEnumInputValidatesClosedVariantSet(t *testing.T) {
	input := Enum("user.Status", "ready", "ready", "done")
	if input.err != nil {
		t.Fatal(input.err)
	}
	encoded := input.value.GetEnumValue()
	if encoded.GetName() != "user.Status" || encoded.GetValue() != "ready" {
		t.Fatalf("encoded enum = %#v", encoded)
	}

	invalid := Enum("user.Status", "unknown", "ready", "done")
	if invalid.err == nil || !strings.Contains(invalid.err.Error(), `invalid BAML enum "user.Status" variant "unknown"`) {
		t.Fatalf("invalid enum error = %v", invalid.err)
	}
}

func TestEnumOutputValidatesNameAndClosedVariantSet(t *testing.T) {
	value := Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_EnumValue{
		EnumValue: &cffi.BamlValueEnum{Name: "user.Status", Value: "done"},
	}}}

	got, err := value.Enum("user.Status", "ready", "done")
	if err != nil || got != "done" {
		t.Fatalf("Enum() = %q, %v", got, err)
	}
	if _, err := value.Enum("user.Other", "done"); err == nil || !strings.Contains(err.Error(), `expected BAML enum "user.Other", got "user.Status"`) {
		t.Fatalf("wrong-name error = %v", err)
	}
	if _, err := value.Enum("user.Status", "ready"); err == nil || !strings.Contains(err.Error(), `unknown variant "done"`) {
		t.Fatalf("unknown-variant error = %v", err)
	}

	dynamic := Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_EnumValue{
		EnumValue: &cffi.BamlValueEnum{Name: "user.Status", Value: "ready", IsDynamic: true},
	}}}
	if _, err := dynamic.Enum("user.Status", "ready", "done"); err == nil || !strings.Contains(err.Error(), `dynamic variant "ready" for a closed enum`) {
		t.Fatalf("dynamic-variant error = %v", err)
	}
}

func TestEmptyEnumHasNoValidValues(t *testing.T) {
	if input := Enum("user.Empty", ""); input.err == nil {
		t.Fatal("empty enum unexpectedly accepted a value")
	}
}
