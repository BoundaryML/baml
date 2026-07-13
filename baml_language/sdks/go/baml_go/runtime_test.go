package baml_go

import (
	"testing"

	"github.com/boundaryml/baml/sdks/go/baml_go/internal/cffi"
)

func TestEncodeCallUsesNamedKwargs(t *testing.T) {
	payload, err := encodeCall(42, map[string]Input{
		"text":  String("hello"),
		"count": Int64(3),
	})
	if err != nil {
		t.Fatal(err)
	}
	if len(payload) == 0 {
		t.Fatal("encoded payload was empty")
	}
}

func TestScalarValueAccessors(t *testing.T) {
	value := Value{value: &cffi.BamlOutboundValue{
		Value: &cffi.BamlOutboundValue_StringValue{StringValue: "hello"},
	}}
	got, err := value.String()
	if err != nil {
		t.Fatal(err)
	}
	if got != "hello" {
		t.Fatalf("got %q, want hello", got)
	}
}
