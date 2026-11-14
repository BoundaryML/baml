package baml_go

import (
	"reflect"
	"strings"
	"testing"

	"github.com/boundaryml/baml-go/internal/cffi"
	"google.golang.org/protobuf/proto"
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

func carriedTuple(tupleType BAMLType, items ...*cffi.BamlOutboundValue) Value {
	value := outboundTuple(items...)
	value.value.GetTupleValue().TupleType = tupleType.value
	return value
}

func outboundBool(value bool) *cffi.BamlOutboundValue {
	return &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_BoolValue{BoolValue: value}}
}

// testFloatString stands in for a generated named tuple struct.
type testFloatString struct {
	Item0 float64
	Item1 string
}

func floatStringTuple() BAMLType {
	return TupleBAMLType(PrimitiveBAMLType(FloatType), PrimitiveBAMLType(StringType))
}

func (testFloatString) BAMLType() BAMLType { return floatStringTuple() }

func (testFloatString) BAMLDecode(value Value) (any, error) {
	slots, err := value.Tuple(2)
	if err != nil {
		return nil, err
	}
	first, err := DecodeTupleSlot(slots, 0, Value.Float64)
	if err != nil {
		return nil, err
	}
	second, err := DecodeTupleSlot(slots, 1, Value.String)
	if err != nil {
		return nil, err
	}
	return testFloatString{Item0: first, Item1: second}, nil
}

func outboundFloat(value float64) *cffi.BamlOutboundValue {
	return &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_FloatValue{FloatValue: value}}
}

func TestDynamicTuplesFindTheirRegisteredStructByCarriedType(t *testing.T) {
	RegisterTuple[testFloatString]()
	RegisterTuple[testFloatString]() // a second registration keeps the first
	value := carriedTuple(floatStringTuple(), outboundFloat(1.5), outboundString("x"))
	got, err := DecodeAs[any](value)
	if err != nil || got != (testFloatString{Item0: 1.5, Item1: "x"}) {
		t.Fatalf("registered tuple = %#v, %v", got, err)
	}
	// Slot order is part of the identity: `(string, float)` is not registered.
	swapped := TupleBAMLType(PrimitiveBAMLType(StringType), PrimitiveBAMLType(FloatType))
	got, err = DecodeAs[any](carriedTuple(swapped, outboundString("x"), outboundFloat(1.5)))
	if err != nil || !reflect.DeepEqual(got, []any{"x", 1.5}) {
		t.Fatalf("unregistered swapped tuple = %#v, %v", got, err)
	}
	// Nested: the inner tuple carries its own type and finds its struct.
	nested := TupleBAMLType(floatStringTuple(), PrimitiveBAMLType(IntType))
	inner := carriedTuple(floatStringTuple(), outboundFloat(2), outboundString("y")).value
	got, err = DecodeAs[any](carriedTuple(nested, inner, outboundInt(3)))
	if err != nil || !reflect.DeepEqual(got, []any{testFloatString{Item0: 2, Item1: "y"}, int64(3)}) {
		t.Fatalf("nested tuple = %#v, %v", got, err)
	}
	// Slots of one registered tuple type join to a typed slice of its struct.
	pairOfPairs := TupleBAMLType(floatStringTuple(), floatStringTuple())
	got, err = DecodeAs[any](carriedTuple(pairOfPairs, inner, inner))
	want := []testFloatString{{Item0: 2, Item1: "y"}, {Item0: 2, Item1: "y"}}
	if err != nil || !reflect.DeepEqual(got, want) {
		t.Fatalf("joined registered slots = %#v, %v", got, err)
	}
	// A malformed registered tuple is an error, not a fallback.
	if _, err := DecodeAs[any](carriedTuple(floatStringTuple(), outboundFloat(1), outboundInt(2))); err == nil {
		t.Fatal("a malformed registered tuple decoded")
	}
}

func TestUnregisteredDynamicTuplesDecodeAsTheListOfTheirSlotJoin(t *testing.T) {
	boolPair := TupleBAMLType(PrimitiveBAMLType(BoolType), PrimitiveBAMLType(BoolType))
	got, err := DecodeAs[any](carriedTuple(boolPair, outboundBool(true), outboundBool(false)))
	if err != nil || !reflect.DeepEqual(got, []bool{true, false}) {
		t.Fatalf("(bool, bool) = %#v, %v", got, err)
	}
	repeated := TupleBAMLType(PrimitiveBAMLType(IntType), PrimitiveBAMLType(IntType), PrimitiveBAMLType(IntType))
	got, err = DecodeAs[any](carriedTuple(repeated, outboundInt(1), outboundInt(1), outboundInt(2)))
	if err != nil || !reflect.DeepEqual(got, []int64{1, 1, 2}) {
		t.Fatalf("(int, int, int) keeps every slot = %#v, %v", got, err)
	}
	// A heterogeneous join is a union, whose Go carrier is `any`.
	mixed := TupleBAMLType(PrimitiveBAMLType(BoolType), PrimitiveBAMLType(IntType))
	got, err = DecodeAs[any](carriedTuple(mixed, outboundBool(true), outboundInt(1)))
	if err != nil || !reflect.DeepEqual(got, []any{true, int64(1)}) {
		t.Fatalf("(bool, int) = %#v, %v", got, err)
	}
	// An optional slot's join is not one Go type either.
	optional := TupleBAMLType(OptionalBAMLType(PrimitiveBAMLType(IntType)), OptionalBAMLType(PrimitiveBAMLType(IntType)))
	got, err = DecodeAs[any](carriedTuple(optional, outboundInt(1), &cffi.BamlOutboundValue{}))
	if err != nil || !reflect.DeepEqual(got, []any{int64(1), nil}) {
		t.Fatalf("(int?, int?) = %#v, %v", got, err)
	}
}

func TestReceivedTuplesRestateTheirCarriedType(t *testing.T) {
	value := carriedTuple(intStringTuple(), outboundInt(1), outboundString("one"))
	encoded, err := outboundToInbound(value.value, nil)
	if err != nil {
		t.Fatal(err)
	}
	if got := encoded.GetValueType(); !proto.Equal(got, intStringTuple().value) {
		t.Fatalf("restated tuple states %v", got)
	}
	slots := encoded.GetListValue().GetValues()
	if len(slots) != 2 || slots[0].GetIntValue() != 1 || slots[1].GetStringValue() != "one" {
		t.Fatalf("restated slots %#v", slots)
	}
	if _, err := outboundToInbound(outboundTuple(outboundInt(1)).value, nil); err == nil {
		t.Fatal("a tuple without its carried type was restated")
	}
	short := carriedTuple(intStringTuple(), outboundInt(1))
	if _, err := outboundToInbound(short.value, nil); err == nil || !strings.Contains(err.Error(), "arity") {
		t.Fatalf("an arity mismatch restated: %v", err)
	}
}
