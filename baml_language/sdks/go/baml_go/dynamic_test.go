package baml_go

import (
	"math/big"
	"testing"
)

type nilTestMarshaler struct{}

func (*nilTestMarshaler) BAMLInput() Input {
	panic("nil InputMarshaler receiver must not be called")
}

type nilTestClass struct{}

func (*nilTestClass) BAMLClassName() string {
	panic("nil DynamicClass receiver must not be called")
}

type pointerTestEnum string

func (pointerTestEnum) BAMLEnumName() string { return "test.PointerEnum" }

func (pointerTestEnum) BAMLEnumVariants() []string { return []string{"ready", "done"} }

func assertNullInput(t *testing.T, input Input) {
	t.Helper()
	if input.err != nil || input.value == nil || input.value.Value != nil {
		t.Fatalf("expected BAML null input, got %#v, %v", input.value, input.err)
	}
}

func TestAnyTreatsNilPointersAsNullBeforeInterfaceDispatch(t *testing.T) {
	var marshaler *nilTestMarshaler
	var integer *big.Int
	var class *nilTestClass
	assertNullInput(t, Any(marshaler))
	assertNullInput(t, Any(integer))
	assertNullInput(t, Any(class))
	var enum *pointerTestEnum
	assertNullInput(t, Any(enum))
}

func TestAnyEncodesNonNilPointersToGeneratedEnums(t *testing.T) {
	ready := pointerTestEnum("ready")
	input := Any(&ready)
	if input.err != nil {
		t.Fatal(input.err)
	}
	encoded := input.value.GetEnumValue()
	if encoded == nil || encoded.Name != "test.PointerEnum" || encoded.Value != "ready" {
		t.Fatalf("encoded enum pointer = %#v", encoded)
	}
}

func TestAnyEncodesNestedOptionalEnumPointers(t *testing.T) {
	ready := pointerTestEnum("ready")
	input := Any([]*pointerTestEnum{&ready, nil})
	if input.err != nil {
		t.Fatal(input.err)
	}
	list := input.value.GetListValue()
	if list == nil || len(list.Values) != 2 {
		t.Fatalf("encoded enum pointer list = %#v", list)
	}
	if enum := list.Values[0].GetEnumValue(); enum == nil || enum.Name != "test.PointerEnum" || enum.Value != "ready" {
		t.Fatalf("first enum pointer = %#v", list.Values[0])
	}
	if list.Values[1] == nil || list.Values[1].Value != nil {
		t.Fatalf("nil enum pointer did not encode as null: %#v", list.Values[1])
	}
	wantItemType := OptionalBAMLType(EnumBAMLType("test.PointerEnum"))
	wantListType := ListBAMLType(wantItemType)
	if input.value.ValueType == nil || !(BAMLType{value: input.value.ValueType}).Equal(wantListType) {
		t.Fatalf("enum pointer list value type = %#v", input.value.ValueType)
	}
}

func TestBAMLTypeEqualityPreservesNestedOptionality(t *testing.T) {
	integer := PrimitiveBAMLType(IntType)
	optionalInteger := OptionalBAMLType(integer)
	if ListBAMLType(integer).Equal(ListBAMLType(optionalInteger)) {
		t.Fatal("list<int> compared equal to list<int?>")
	}
	stringType := PrimitiveBAMLType(StringType)
	if MapBAMLType(stringType, integer).Equal(MapBAMLType(stringType, optionalInteger)) {
		t.Fatal("map<string,int> compared equal to map<string,int?>")
	}
	if integer.Equal(optionalInteger) {
		t.Fatal("exact equality erased top-level optionality")
	}
	if !integer.MatchesUnionArm(optionalInteger) {
		t.Fatal("selected top-level optional wrapper did not compare equal to its non-null arm")
	}
	if !optionalInteger.Equal(OptionalBAMLType(integer)) {
		t.Fatal("equivalent top-level optional descriptors did not compare equal")
	}
}

func TestAnyPreservesStaticTypesForEmptyContainers(t *testing.T) {
	list := Any([]string{})
	if list.err != nil || list.value.GetValueType() == nil {
		t.Fatalf("empty string list lost item type: %#v, %v", list.value, list.err)
	}
	if !(BAMLType{value: list.value.GetValueType()}).Equal(ListBAMLType(PrimitiveBAMLType(StringType))) {
		t.Fatalf("empty string list got wrong value type: %#v", list.value.GetValueType())
	}

	mapValue := Any(map[string]int64{})
	if mapValue.err != nil || mapValue.value.GetValueType() == nil {
		t.Fatalf("empty int map lost types: %#v, %v", mapValue.value, mapValue.err)
	}
	wantMapType := MapBAMLType(PrimitiveBAMLType(StringType), PrimitiveBAMLType(IntType))
	if !(BAMLType{value: mapValue.value.GetValueType()}).Equal(wantMapType) {
		t.Fatalf("empty int map got wrong value type: %#v", mapValue.value.GetValueType())
	}
}

func TestOrdinaryTypedContainerEncodersRemainMetadataOptional(t *testing.T) {
	list := List([]string{}, String)
	if list.value.GetValueType() != nil {
		t.Fatalf("schema-driven list unexpectedly added dynamic type metadata")
	}
	mapValue := Map(map[string]string{}, String)
	if mapValue.value.GetValueType() != nil {
		t.Fatalf("schema-driven map unexpectedly added dynamic type metadata")
	}
}
