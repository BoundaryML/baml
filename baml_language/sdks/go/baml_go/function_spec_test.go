package baml_go

import (
	"testing"

	"github.com/boundaryml/baml-go/internal/cffi"
)

func TestEventCallbackInputUsesTheHostCallableABI(t *testing.T) {
	var received Value
	input := EventCallbackInput(func(value Value) { received = value })
	transaction := &inputTransaction{}
	encoded, err := input.encodeValue(transaction)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(transaction.rollback)
	handle := encoded.GetHandle()
	if handle == nil || handle.GetHandleType() != cffi.BamlHandleType_HOST_VALUE_CALLABLE {
		t.Fatalf("event callback encoded as %#v", encoded.GetValue())
	}
	callable, ok := lookupHostCallable(handle.GetKey())
	if !ok {
		t.Fatal("event callback was not registered")
	}
	event := Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_StringValue{StringValue: "event"}}}
	result, err := callable(HostCallArguments{required: []Value{event}})
	if err != nil {
		t.Fatal(err)
	}
	if received.value != event.value {
		t.Fatal("event callback did not receive the canonical Value")
	}
	encodedResult, err := result.encodeValue(&inputTransaction{})
	if err != nil || encodedResult.GetValue() != nil {
		t.Fatalf("event callback result = %#v, %v; want null", encodedResult, err)
	}
}
