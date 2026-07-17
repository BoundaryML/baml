package baml_go

import (
	"context"
	"errors"
	"testing"

	"github.com/boundaryml/baml-go/internal/cffi"
	"google.golang.org/protobuf/proto"
)

func TestCallRejectsNULInFunctionNameBeforeRuntimeInitialization(t *testing.T) {
	_, err := Call(context.Background(), "user.foo\x00bar", nil)
	if err == nil || err.Error() != "baml_go.Call: function name contains a NUL byte" {
		t.Fatalf("got error %v, want embedded-NUL diagnostic", err)
	}
}

func TestRuntimeInitializationWaitHonorsCancellation(t *testing.T) {
	state := newNativeRuntimeState()
	if err := state.acquire(context.Background()); err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if err := state.acquire(ctx); !errors.Is(err, context.Canceled) {
		t.Fatalf("got error %v, want context cancellation", err)
	}
	state.release()
}

func TestReservePendingCallSkipsOccupiedIDAfterWraparound(t *testing.T) {
	previous := nextCallbackID.Load()
	nextCallbackID.Store(^uint32(0))
	occupied := &pendingCall{result: make(chan []byte, 1)}
	pendingCalls.Store(uint32(1), occupied)
	t.Cleanup(func() {
		pendingCalls.Delete(uint32(1))
		pendingCalls.Delete(uint32(2))
		nextCallbackID.Store(previous)
	})

	call := &pendingCall{result: make(chan []byte, 1)}
	if id := reservePendingCall(call); id != 2 {
		t.Fatalf("reserved callback ID %d, want 2", id)
	}
	if got, _ := pendingCalls.Load(uint32(1)); got != occupied {
		t.Fatal("occupied callback ID was overwritten")
	}
}

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

func TestScalarValueAccessorsRejectNilLiteralPayloads(t *testing.T) {
	value := Value{value: &cffi.BamlOutboundValue{
		Value: &cffi.BamlOutboundValue_LiteralValue{},
	}}
	for name, decode := range map[string]func() error{
		"string": func() error { _, err := value.String(); return err },
		"int":    func() error { _, err := value.Int64(); return err },
		"bigint": func() error { _, err := value.BigInt(); return err },
		"float":  func() error { _, err := value.Float64(); return err },
		"bool":   func() error { _, err := value.Bool(); return err },
	} {
		t.Run(name, func(t *testing.T) {
			if err := decode(); err == nil {
				t.Fatal("nil literal payload unexpectedly decoded")
			}
		})
	}
}

func TestAbsentOutboundOneofIsNull(t *testing.T) {
	value := Value{value: &cffi.BamlOutboundValue{}}
	if isNull, err := value.isNull(); err != nil || !isNull {
		t.Fatalf("isNull returned (%v, %v), want (true, nil)", isNull, err)
	}
	if _, err := value.Null(); err != nil {
		t.Fatalf("absent outbound oneof did not decode as null: %v", err)
	}
}

func TestEncodeCallUsesExactClassAndFieldWireNames(t *testing.T) {
	payload, err := encodeCall(42, map[string]Input{
		"person_arg": Class("user.people.Person", map[string]Input{
			"age_years": Int64(37),
			"full_name": String("Ada"),
		}),
	})
	if err != nil {
		t.Fatal(err)
	}

	var call cffi.CallFunctionArgs
	if err := proto.Unmarshal(payload, &call); err != nil {
		t.Fatal(err)
	}
	if len(call.Kwargs) != 1 || call.Kwargs[0].GetStringKey() != "person_arg" {
		t.Fatalf("unexpected kwargs: %#v", call.Kwargs)
	}
	class := call.Kwargs[0].Value.GetClassValue()
	if class == nil || class.ClassTy.GetName() != "user.people.Person" {
		t.Fatalf("unexpected class: %#v", class)
	}
	if len(class.Fields) != 2 {
		t.Fatalf("got %d fields, want 2", len(class.Fields))
	}
	if class.Fields[0].GetStringKey() != "age_years" || class.Fields[1].GetStringKey() != "full_name" {
		t.Fatalf("fields are not sorted by exact wire name: %#v", class.Fields)
	}
}

func TestClassValueValidatesNameAndDecodesFields(t *testing.T) {
	value := Value{value: &cffi.BamlOutboundValue{
		Value: &cffi.BamlOutboundValue_ClassValue{ClassValue: &cffi.BamlValueClass{
			Name: "user.Person",
			Fields: []*cffi.BamlOutboundMapEntry{
				{Key: "name", Value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_StringValue{StringValue: "Ada"}}},
				{Key: "age", Value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_IntValue{IntValue: 37}}},
			},
		}},
	}}

	class, err := value.Class("user.Person")
	if err != nil {
		t.Fatal(err)
	}
	name, err := class.String("name")
	if err != nil {
		t.Fatal(err)
	}
	age, err := class.Int64("age")
	if err != nil {
		t.Fatal(err)
	}
	if name != "Ada" || age != 37 {
		t.Fatalf("got (%q, %d), want (%q, %d)", name, age, "Ada", 37)
	}
	if _, err := value.Class("user.Other"); err == nil {
		t.Fatal("wrong class name unexpectedly succeeded")
	}
	if _, err := class.Bool("missing"); err == nil {
		t.Fatal("missing field unexpectedly succeeded")
	}
}

func TestClassValueDecodesNestedClass(t *testing.T) {
	value := Value{value: &cffi.BamlOutboundValue{
		Value: &cffi.BamlOutboundValue_ClassValue{ClassValue: &cffi.BamlValueClass{
			Name: "user.Outer",
			Fields: []*cffi.BamlOutboundMapEntry{{
				Key: "inner",
				Value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ClassValue{
					ClassValue: &cffi.BamlValueClass{
						Name: "user.Inner",
						Fields: []*cffi.BamlOutboundMapEntry{{
							Key: "value",
							Value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_IntValue{
								IntValue: 42,
							}},
						}},
					},
				}},
			}},
		}},
	}}

	outer, err := value.Class("user.Outer")
	if err != nil {
		t.Fatal(err)
	}
	inner, err := outer.Class("inner", "user.Inner")
	if err != nil {
		t.Fatal(err)
	}
	got, err := inner.Int64("value")
	if err != nil {
		t.Fatal(err)
	}
	if got != 42 {
		t.Fatalf("got %d, want 42", got)
	}
	if _, err := outer.Class("inner", "user.Other"); err == nil {
		t.Fatal("wrong nested class name unexpectedly succeeded")
	}
}
