package baml_go

import (
	"reflect"
	"strings"
	"testing"

	"github.com/boundaryml/baml-go/internal/cffi"
)

func intStringTuple() BAMLType {
	return TupleBAMLType(PrimitiveBAMLType(IntType), PrimitiveBAMLType(StringType))
}

func outboundTuple(items ...*cffi.BamlOutboundValue) Value {
	return Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_TupleValue{
		TupleValue: &cffi.BamlValueTuple{Items: items},
	}}}
}

func outboundInt(value int64) *cffi.BamlOutboundValue {
	return &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_IntValue{IntValue: value}}
}

func TestTupleInputIsAListStatingItsTupleType(t *testing.T) {
	input := TupleInput(intStringTuple(), Int64(1), String("one"))
	if input.err != nil {
		t.Fatal(input.err)
	}
	if got, want := input.value.GetValueType(), intStringTuple().value; !reflect.DeepEqual(got, want) {
		t.Fatalf("tuple states %#v, want %#v", got, want)
	}
	values := input.value.GetListValue().GetValues()
	if len(values) != 2 || values[0].GetIntValue() != 1 || values[1].GetStringValue() != "one" {
		t.Fatalf("tuple slots encoded as %#v", values)
	}
}

func TestTupleInputRejectsWrongArityAndNonTupleTypes(t *testing.T) {
	if input := TupleInput(intStringTuple(), Int64(1)); input.err == nil || !strings.Contains(input.err.Error(), "arity 2 has 1 slots") {
		t.Fatalf("short tuple: %v", input.err)
	}
	if input := TupleListEncoder(intStringTuple())([]any{int64(1), "one", true}); input.err == nil {
		t.Fatal("a three-element []any unexpectedly encoded as a two-slot tuple")
	}
	if input := TupleInput(ListBAMLType(PrimitiveBAMLType(IntType)), Int64(1)); input.err == nil {
		t.Fatal("a list type unexpectedly stated a tuple")
	}
}

func TestTupleListEncoderStatesTheDeclaredTupleType(t *testing.T) {
	input := TupleListEncoder(intStringTuple())([]any{int64(1), "one"})
	if input.err != nil {
		t.Fatal(input.err)
	}
	if got, want := input.value.GetValueType(), intStringTuple().value; !reflect.DeepEqual(got, want) {
		t.Fatalf("fallback tuple states %#v, want %#v", got, want)
	}
}

func TestTupleDecodingChecksArityAndNeverAcceptsAList(t *testing.T) {
	slots, err := outboundTuple(outboundInt(1), outboundString("one")).Tuple(2)
	if err != nil || len(slots) != 2 {
		t.Fatalf("slots = %v, %v", slots, err)
	}
	if got, err := DecodeTupleSlot(slots, 1, Value.String); err != nil || got != "one" {
		t.Fatalf("slot 1 = %q, %v", got, err)
	}
	if _, err := DecodeTupleSlot(slots, 1, Value.Int64); err == nil || !strings.Contains(err.Error(), "slot 1") {
		t.Fatalf("a wrong slot type reported %v", err)
	}
	if _, err := outboundTuple(outboundInt(1)).Tuple(2); err == nil {
		t.Fatal("a one-slot tuple unexpectedly decoded as two slots")
	}
	list := Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ListValue{
		ListValue: &cffi.BamlValueList{Items: []*cffi.BamlOutboundValue{outboundInt(1), outboundString("one")}},
	}}}
	if _, err := list.Tuple(2); err == nil {
		t.Fatal("a two-element list unexpectedly decoded as a tuple")
	}
}

func TestTupleFallbacksDecodeSlotsInOrder(t *testing.T) {
	value := outboundTuple(outboundInt(1), outboundString("one"))
	got, err := TupleListDecoder(2)(value)
	if err != nil || !reflect.DeepEqual(got, []any{int64(1), "one"}) {
		t.Fatalf("[]any fallback = %#v, %v", got, err)
	}
	if _, err := TupleListDecoder(3)(value); err == nil {
		t.Fatal("[]any fallback ignored the arity")
	}
	dynamic, err := DecodeAs[any](value)
	if err != nil || !reflect.DeepEqual(dynamic, []any{int64(1), "one"}) {
		t.Fatalf("dynamic tuple = %#v, %v", dynamic, err)
	}
}

func TestTupleTypesCompareSlotsInOrder(t *testing.T) {
	stringInt := TupleBAMLType(PrimitiveBAMLType(StringType), PrimitiveBAMLType(IntType))
	if !intStringTuple().Equal(intStringTuple()) || intStringTuple().Equal(stringInt) {
		t.Fatal("tuple equality ignored slot order")
	}
	left := TupleBAMLType(UnionBAMLType(PrimitiveBAMLType(IntType), PrimitiveBAMLType(StringType)))
	right := TupleBAMLType(UnionBAMLType(PrimitiveBAMLType(StringType), PrimitiveBAMLType(IntType)))
	if !left.Equal(right) {
		t.Fatal("a union slot compared by member order")
	}
	if intStringTuple().Equal(ListBAMLType(PrimitiveBAMLType(IntType))) {
		t.Fatal("a tuple type equals a list type")
	}
	if err := validateBAMLTypeValue(TupleBAMLType(), typeStated); err == nil {
		t.Fatal("an empty tuple descriptor validated")
	}
}
