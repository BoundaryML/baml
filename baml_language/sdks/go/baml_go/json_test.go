package baml_go

import (
	"math"
	"reflect"
	"strings"
	"testing"

	"github.com/boundaryml/baml-go/internal/cffi"
)

func jsonValue(value *cffi.BamlOutboundValue) Value {
	return Value{value: value}
}

func TestJSONDecodesCanonicalRecursiveValueAlgebra(t *testing.T) {
	list := &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ListValue{
		ListValue: &cffi.BamlValueList{Items: []*cffi.BamlOutboundValue{
			{Value: &cffi.BamlOutboundValue_NullValue{NullValue: &cffi.BamlValueNull{}}},
			{Value: &cffi.BamlOutboundValue_BoolValue{BoolValue: true}},
			{Value: &cffi.BamlOutboundValue_IntValue{IntValue: 42}},
			{Value: &cffi.BamlOutboundValue_FloatValue{FloatValue: 1.5}},
			{Value: &cffi.BamlOutboundValue_StringValue{StringValue: "ok"}},
		}},
	}}
	nested := jsonValue(&cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_MapValue{
		MapValue: &cffi.BamlValueMap{Entries: []*cffi.BamlOutboundMapEntry{
			{Key: "array", Value: list},
		}},
	}})
	got, err := nested.JSON()
	if err != nil {
		t.Fatal(err)
	}
	want := map[string]any{"array": []any{nil, true, int64(42), 1.5, "ok"}}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("JSON() = %#v, want %#v", got, want)
	}
}

func TestJSONValidatesUnionEnvelopeBeforeDecoding(t *testing.T) {
	alias := TypeAliasBAMLType("baml.json.json")
	selected := PrimitiveBAMLType(StringType)
	index := uint32(4)
	selfType := &cffi.BamlTy{Ty: &cffi.BamlTy_Union{
		Union: &cffi.BamlTyUnion{Options: []*cffi.BamlTy{
			PrimitiveBAMLType(NullType).value,
			PrimitiveBAMLType(BoolType).value,
			PrimitiveBAMLType(IntType).value,
			PrimitiveBAMLType(FloatType).value,
			selected.value,
			ListBAMLType(alias).value,
			MapBAMLType(PrimitiveBAMLType(StringType), alias).value,
		}},
	}}
	variant := &cffi.BamlValueUnionVariant{
		SelfType:            selfType,
		SelectedOptionIndex: &index,
		Value:               &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_StringValue{StringValue: "wrapped"}},
	}
	wrapped := Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_UnionVariantValue{UnionVariantValue: variant}}}
	got, err := wrapped.JSON()
	if err != nil || got != "wrapped" {
		t.Fatalf("JSON() = %#v, %v", got, err)
	}
	badIndex := uint32(0)
	wrapped.value.GetUnionVariantValue().SelectedOptionIndex = &badIndex
	if _, err := wrapped.JSON(); err == nil {
		t.Fatalf("invalid union metadata error = %v", err)
	}

	bigint := PrimitiveBAMLType(BigintType)
	bigintIndex := uint32(0)
	forgedNonJSON := outboundUnion(
		UnionBAMLType(bigint, selected),
		&bigintIndex,
		&cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_StringValue{StringValue: "forged"}},
	)
	if _, err := forgedNonJSON.JSON(); err == nil || !strings.Contains(err.Error(), "selected union arm is not JSON") {
		t.Fatalf("non-JSON selected arm error = %v", err)
	}

	stringIndex := uint32(0)
	forgedMismatch := outboundUnion(
		UnionBAMLType(selected, PrimitiveBAMLType(IntType)),
		&stringIndex,
		&cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_IntValue{IntValue: 7}},
	)
	if _, err := forgedMismatch.JSON(); err == nil || !strings.Contains(err.Error(), "disagrees with payload") {
		t.Fatalf("selected/payload mismatch error = %v", err)
	}

	listOfInt := ListBAMLType(PrimitiveBAMLType(IntType))
	listIndex := uint32(0)
	forgedList := outboundUnion(
		UnionBAMLType(listOfInt, selected),
		&listIndex,
		&cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ListValue{ListValue: &cffi.BamlValueList{Items: []*cffi.BamlOutboundValue{
			{Value: &cffi.BamlOutboundValue_StringValue{StringValue: "wrong"}},
		}}}},
	)
	if _, err := forgedList.JSON(); err == nil || !strings.Contains(err.Error(), "$[0]") {
		t.Fatalf("list descriptor mismatch error = %v", err)
	}

	mapOfInt := MapBAMLType(PrimitiveBAMLType(StringType), PrimitiveBAMLType(IntType))
	mapIndex := uint32(0)
	forgedMap := outboundUnion(
		UnionBAMLType(mapOfInt, selected),
		&mapIndex,
		&cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_MapValue{MapValue: &cffi.BamlValueMap{Entries: []*cffi.BamlOutboundMapEntry{
			{Key: "wrong", Value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_StringValue{StringValue: "wrong"}}},
		}}}},
	)
	if _, err := forgedMap.JSON(); err == nil || !strings.Contains(err.Error(), `$["wrong"]`) {
		t.Fatalf("map descriptor mismatch error = %v", err)
	}

	jsonOnlyUnion := UnionBAMLType(PrimitiveBAMLType(IntType), selected)
	unionIndex := uint32(0)
	forgedNestedUnion := outboundUnion(
		UnionBAMLType(jsonOnlyUnion, PrimitiveBAMLType(BoolType)),
		&unionIndex,
		&cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_BoolValue{BoolValue: true}},
	)
	if _, err := forgedNestedUnion.JSON(); err == nil || !strings.Contains(err.Error(), "selected JSON union disagrees") {
		t.Fatalf("nested union descriptor mismatch error = %v", err)
	}
}

func TestJSONRejectsMalformedAndNonJSONWireValues(t *testing.T) {
	tests := []struct {
		name  string
		value Value
		want  string
	}{
		{"uninitialized", Value{}, "uninitialized"},
		{"empty null", jsonValue(&cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_NullValue{}}), "null payload is empty"},
		{"nan", jsonValue(&cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_FloatValue{FloatValue: math.NaN()}}), "non-finite"},
		{"infinity", jsonValue(&cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_FloatValue{FloatValue: math.Inf(1)}}), "non-finite"},
		{"bytes", jsonValue(&cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_Uint8ArrayValue{Uint8ArrayValue: []byte("no")}}), "non-JSON"},
		{"bigint", jsonValue(&cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_BigintValue{BigintValue: "ff"}}), "non-JSON"},
		{"class", jsonValue(&cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ClassValue{ClassValue: &cffi.BamlValueClass{Name: "user.C"}}}), "non-JSON"},
		{"enum", jsonValue(&cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_EnumValue{EnumValue: &cffi.BamlValueEnum{Name: "user.E", Value: "X"}}}), "non-JSON"},
		{"empty list", jsonValue(&cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ListValue{}}), "list payload is empty"},
		{"nil list item", jsonValue(&cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ListValue{ListValue: &cffi.BamlValueList{Items: []*cffi.BamlOutboundValue{nil}}}}), "$[0]"},
		{"empty map", jsonValue(&cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_MapValue{}}), "map payload is empty"},
		{"nil map entry", jsonValue(&cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_MapValue{MapValue: &cffi.BamlValueMap{Entries: []*cffi.BamlOutboundMapEntry{nil}}}}), "entry 0"},
		{"nil map value", jsonValue(&cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_MapValue{MapValue: &cffi.BamlValueMap{Entries: []*cffi.BamlOutboundMapEntry{{Key: "x"}}}}}), "$[\"x\"]"},
		{"duplicate map key", jsonValue(&cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_MapValue{MapValue: &cffi.BamlValueMap{Entries: []*cffi.BamlOutboundMapEntry{
			{Key: "x", Value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_NullValue{NullValue: &cffi.BamlValueNull{}}}},
			{Key: "x", Value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_NullValue{NullValue: &cffi.BamlValueNull{}}}},
		}}}}), "duplicate"},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			_, err := test.value.JSON()
			if err == nil || !strings.Contains(err.Error(), test.want) {
				t.Fatalf("JSON() error = %v, want substring %q", err, test.want)
			}
		})
	}
}

func TestJSONAcceptsRuntimeAndExplicitNullFormsAtEveryDepth(t *testing.T) {
	runtimeNull := &cffi.BamlOutboundValue{}
	explicitNull := &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_NullValue{NullValue: &cffi.BamlValueNull{}}}
	for name, value := range map[string]*cffi.BamlOutboundValue{
		"runtime":  runtimeNull,
		"explicit": explicitNull,
	} {
		t.Run(name, func(t *testing.T) {
			if got, err := (Value{value: value}).JSON(); err != nil || got != nil {
				t.Fatalf("top-level null = %#v, %v", got, err)
			}
			list := Value{value: &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ListValue{ListValue: &cffi.BamlValueList{Items: []*cffi.BamlOutboundValue{value}}}}}
			got, err := list.JSON()
			if err != nil || !reflect.DeepEqual(got, []any{nil}) {
				t.Fatalf("nested null = %#v, %v", got, err)
			}
		})
	}
}

type jsonTestMarshaler string

func (jsonTestMarshaler) BAMLInput() Input { return String("not JSON through marshaler") }

type jsonTestClass struct{}

func (jsonTestClass) BAMLClassName() string { return "user.JsonTestClass" }

type jsonTestEnum string

func (jsonTestEnum) BAMLEnumName() string       { return "user.JsonTestEnum" }
func (jsonTestEnum) BAMLEnumVariants() []string { return []string{"X"} }

func TestJSONInputAcceptsOnlyCanonicalGoJSONShapes(t *testing.T) {
	integer := int64(7)
	valid := []any{
		nil,
		(*int64)(nil),
		[]any(nil),
		map[string]any(nil),
		true,
		"text",
		int64(-2),
		uint64(math.MaxInt64),
		float64(1.25),
		[]any{nil, true, int64(2), "three", []string{"nested"}},
		map[string]any{"nested": map[string]any{"value": 3.5}},
		&integer,
	}
	for index, value := range valid {
		if input := JSON(value); input.err != nil {
			t.Errorf("valid input %d (%T): %v", index, value, input.err)
		}
	}

	nilValues := []any{(*int64)(nil), []any(nil), map[string]any(nil)}
	for _, value := range nilValues {
		input := JSON(value)
		if input.err != nil || input.value == nil || input.value.Value != nil {
			t.Fatalf("JSON(%T) = %#v, %v; want canonical absent-oneof null", value, input.value, input.err)
		}
	}
	emptySlice := JSON([]any{})
	if emptySlice.err != nil || emptySlice.value.GetListValue() == nil || len(emptySlice.value.GetListValue().Values) != 0 {
		t.Fatalf("JSON(empty slice) = %#v, %v; want empty list", emptySlice.value, emptySlice.err)
	}
	emptyMap := JSON(map[string]any{})
	if emptyMap.err != nil || emptyMap.value.GetMapValue() == nil || len(emptyMap.value.GetMapValue().Entries) != 0 {
		t.Fatalf("JSON(empty map) = %#v, %v; want empty map", emptyMap.value, emptyMap.err)
	}
}

func TestJSONInputRejectsBAMLExtensionsAndMalformedShapes(t *testing.T) {
	nonNilBytes := []byte{}
	invalid := []struct {
		name  string
		value any
		want  string
	}{
		{"nan", math.NaN(), "non-finite"},
		{"infinity", math.Inf(-1), "non-finite"},
		{"uint overflow", uint64(math.MaxInt64) + 1, "overflows"},
		{"bytes", nonNilBytes, "byte slices"},
		{"non-string map", map[int]string{1: "x"}, "string keys"},
		{"struct", struct{ Value string }{"x"}, "unsupported"},
		{"marshaler", jsonTestMarshaler("x"), "generated BAML"},
		{"class", jsonTestClass{}, "BAML classes"},
		{"enum", jsonTestEnum("X"), "BAML enums"},
	}
	for _, test := range invalid {
		t.Run(test.name, func(t *testing.T) {
			if input := JSON(test.value); input.err == nil || !strings.Contains(input.err.Error(), test.want) {
				t.Fatalf("JSON(%T) error = %v, want substring %q", test.value, input.err, test.want)
			}
		})
	}
}

func TestJSONInputRejectsCyclesAndExcessiveDepth(t *testing.T) {
	mapCycle := map[string]any{}
	mapCycle["self"] = mapCycle
	sliceCycle := make([]any, 1)
	sliceCycle[0] = sliceCycle
	var pointerValue any
	pointerCycle := &pointerValue
	pointerValue = pointerCycle
	for name, value := range map[string]any{
		"map":     mapCycle,
		"slice":   sliceCycle,
		"pointer": pointerCycle,
	} {
		t.Run(name, func(t *testing.T) {
			if input := JSON(value); input.err == nil || !strings.Contains(input.err.Error(), "cyclic") {
				t.Fatalf("cycle error = %v", input.err)
			}
		})
	}

	var deep any = nil
	for range maxJSONDecodeDepth + 2 {
		deep = []any{deep}
	}
	if input := JSON(deep); input.err == nil || !strings.Contains(input.err.Error(), "nesting exceeds") {
		t.Fatalf("deep input error = %v", input.err)
	}
}

func TestJSONDecodesPrimitiveLiteralsAndRejectsBigintLiteral(t *testing.T) {
	stringLiteral := jsonValue(&cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_LiteralValue{LiteralValue: &cffi.BamlLiteralValue{Literal: &cffi.BamlLiteralValue_StringValue{StringValue: "literal"}}}})
	if got, err := stringLiteral.JSON(); err != nil || got != "literal" {
		t.Fatalf("JSON() = %#v, %v", got, err)
	}
	bigintLiteral := jsonValue(&cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_LiteralValue{LiteralValue: &cffi.BamlLiteralValue{Literal: &cffi.BamlLiteralValue_BigintValue{BigintValue: "ff"}}}})
	if _, err := bigintLiteral.JSON(); err == nil || !strings.Contains(err.Error(), "non-JSON") {
		t.Fatalf("bigint literal error = %v", err)
	}
}

func TestJSONRejectsExcessiveNesting(t *testing.T) {
	value := &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_NullValue{NullValue: &cffi.BamlValueNull{}}}
	for range maxJSONDecodeDepth + 2 {
		value = &cffi.BamlOutboundValue{Value: &cffi.BamlOutboundValue_ListValue{ListValue: &cffi.BamlValueList{Items: []*cffi.BamlOutboundValue{value}}}}
	}
	if _, err := (Value{value: value}).JSON(); err == nil || !strings.Contains(err.Error(), "nesting exceeds") {
		t.Fatalf("deep JSON error = %v", err)
	}
}
