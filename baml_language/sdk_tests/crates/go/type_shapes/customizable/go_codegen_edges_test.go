package sdk_test

import (
	"bytes"
	"context"
	"math/big"
	"reflect"
	"strings"
	"testing"

	"baml.local/sdk/baml_sdk"
	"github.com/boundaryml/baml-go"
)

// These assignments are intentionally compile-only. They pin the exact public
// Go signature generated for every primitive and primitive literal currently
// in scope, independently of runtime round-trip coverage.
var (
	_ func(context.Context) (int64, error)                      = baml_sdk.PrimitivesReturnInt
	_ func(context.Context) (*big.Int, error)                   = baml_sdk.PrimitivesReturnBigint
	_ func(context.Context) (float64, error)                    = baml_sdk.PrimitivesReturnFloat
	_ func(context.Context) (string, error)                     = baml_sdk.PrimitivesReturnString
	_ func(context.Context) (bool, error)                       = baml_sdk.PrimitivesReturnBool
	_ func(context.Context) (baml_go.Null, error)               = baml_sdk.PrimitivesReturnNull
	_ func(context.Context, int64) (int64, error)               = baml_sdk.PrimitivesRoundTripInt
	_ func(context.Context, *big.Int) (*big.Int, error)         = baml_sdk.PrimitivesRoundTripBigint
	_ func(context.Context, float64) (float64, error)           = baml_sdk.PrimitivesRoundTripFloat
	_ func(context.Context, string) (string, error)             = baml_sdk.PrimitivesRoundTripString
	_ func(context.Context, bool) (bool, error)                 = baml_sdk.PrimitivesRoundTripBool
	_ func(context.Context, baml_go.Null) (baml_go.Null, error) = baml_sdk.PrimitivesRoundTripNull
	_ func(context.Context, []byte) ([]byte, error)             = baml_sdk.PrimitivesRoundTripUint8Array

	_ func(context.Context) (int64, error)          = baml_sdk.LiteralsReturnLiteral42
	_ func(context.Context) (int64, error)          = baml_sdk.LiteralsReturnLiteralNegOne
	_ func(context.Context) (string, error)         = baml_sdk.LiteralsReturnLiteralDraft
	_ func(context.Context) (string, error)         = baml_sdk.LiteralsReturnLiteralEscaped
	_ func(context.Context) (bool, error)           = baml_sdk.LiteralsReturnLiteralTrue
	_ func(context.Context) (bool, error)           = baml_sdk.LiteralsReturnLiteralFalse
	_ func(context.Context, int64) (int64, error)   = baml_sdk.LiteralsRoundTripLiteral42
	_ func(context.Context, string) (string, error) = baml_sdk.LiteralsRoundTripLiteralDraft
	_ func(context.Context, string) (string, error) = baml_sdk.LiteralsRoundTripLiteralEscaped
	_ func(context.Context, bool) (bool, error)     = baml_sdk.LiteralsRoundTripLiteralTrue
	_ func(context.Context, bool) (bool, error)     = baml_sdk.LiteralsRoundTripLiteralFalse

	_ func(context.Context) error = baml_sdk.VoidNoOp

	_ func(context.Context, string) (string, error)                                                                                           = baml_sdk.GoCodegenLeftEcho
	_ func(context.Context, string) (string, error)                                                                                           = baml_sdk.GoCodegenRightEcho
	_ func(context.Context, string, int64, bool, string, string, string, string, string, string, string) (string, error)                      = baml_sdk.GoCodegenNestedReservedArgs
	_ func(context.Context, *big.Int) (*big.Int, error)                                                                                       = baml_sdk.GoCodegenPrimitiveEdgesRoundTripLiteralBigint
	_ func(context.Context, string, string, string, string, string, string, string, string, string, string) (string, error)                   = baml_sdk.GoCodegenPrimitiveEdgesProtectedLocalNames
	_ func(context.Context, *big.Int, int64, float64, []byte, string, string) (*big.Int, error)                                               = baml_sdk.GoCodegenPrimitiveEdgesReservedTypeNames
	_ func(context.Context, baml_sdk.GoCodegenPrimitiveEdgesWirePrimitives) (baml_sdk.GoCodegenPrimitiveEdgesWirePrimitives, error)           = baml_sdk.GoCodegenPrimitiveEdgesRoundTripWirePrimitives
	_ func(context.Context, int64) (baml_sdk.ClassRefsOuter, error)                                                                           = baml_sdk.ClassRefsMakeOuter
	_ func(context.Context, baml_sdk.ClassRefsOuter) (baml_sdk.ClassRefsOuter, error)                                                         = baml_sdk.ClassRefsRoundTripOuter
	_ func(context.Context, *string) (*string, error)                                                                                         = baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalString
	_ func(context.Context, *int64) (*int64, error)                                                                                           = baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalInt
	_ func(context.Context, *big.Int) (*big.Int, error)                                                                                       = baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalBigint
	_ func(context.Context, *float64) (*float64, error)                                                                                       = baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalFloat
	_ func(context.Context, *bool) (*bool, error)                                                                                             = baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalBool
	_ func(context.Context, *[]byte) (*[]byte, error)                                                                                         = baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalBytes
	_ func(context.Context, *baml_sdk.GoCodegenPrimitiveEdgesWirePrimitives) (*baml_sdk.GoCodegenPrimitiveEdgesWirePrimitives, error)         = baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalClass
	_ func(context.Context, baml_sdk.GoCodegenPrimitiveEdgesNullableWire) (baml_sdk.GoCodegenPrimitiveEdgesNullableWire, error)               = baml_sdk.GoCodegenPrimitiveEdgesRoundTripNullableWire
	_ func(context.Context, []int64) ([]int64, error)                                                                                         = baml_sdk.ListsRoundTripInts
	_ func(context.Context, []*string) ([]*string, error)                                                                                     = baml_sdk.ListsRoundTripOptionalStrings
	_ func(context.Context, map[string]int64) (map[string]int64, error)                                                                       = baml_sdk.MapsRoundTripSimpleMap
	_ func(context.Context, map[string]baml_sdk.MapsResume) (map[string]baml_sdk.MapsResume, error)                                           = baml_sdk.MapsRoundTripEnumKeyedMap
	_ func(context.Context, map[string][]int64) (map[string][]int64, error)                                                                   = baml_sdk.MapsRoundTripListValuedMap
	_ func(context.Context, baml_sdk.MapsMapContainer) (baml_sdk.MapsMapContainer, error)                                                     = baml_sdk.MapsRoundTripMapContainer
	_ func(context.Context, *[]int64) (*[]int64, error)                                                                                       = baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalList
	_ func(context.Context, *map[string]int64) (*map[string]int64, error)                                                                     = baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalMap
	_ func(context.Context, *[]*string) (*[]*string, error)                                                                                   = baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalListOfOptional
	_ func(context.Context, *string) (*string, error)                                                                                         = baml_sdk.GoCodegenPrimitiveEdgesRoundTripRepeatedNull
	_ func(context.Context, baml_sdk.GoCodegenPrimitiveEdgesNullableContainers) (baml_sdk.GoCodegenPrimitiveEdgesNullableContainers, error)   = baml_sdk.GoCodegenPrimitiveEdgesRoundTripNullableContainers
	_ func(context.Context, baml_sdk.GoCodegenPrimitiveEdgesContainerTree) (baml_sdk.GoCodegenPrimitiveEdgesContainerTree, error)             = baml_sdk.GoCodegenPrimitiveEdgesRoundTripContainerTree
	_ func(context.Context, baml_sdk.GoCodegenPrimitiveEdgesContainerLeafMatrix) (baml_sdk.GoCodegenPrimitiveEdgesContainerLeafMatrix, error) = baml_sdk.GoCodegenPrimitiveEdgesRoundTripContainerLeafMatrix

	_ func(context.Context, bool) (baml_sdk.EnumsSentiment, error)                                                                = baml_sdk.EnumsPickSentiment
	_ func(context.Context) (baml_sdk.EnumsSentiment, error)                                                                      = baml_sdk.EnumsPickPositive
	_ func(context.Context, baml_sdk.EnumsSentiment) (baml_sdk.EnumsSentiment, error)                                             = baml_sdk.EnumsRoundTripSentiment
	_ func(context.Context, baml_sdk.EnumsSentiment) (baml_sdk.EnumsSentiment, error)                                             = baml_sdk.EnumsRoundTripSentimentPositive
	_ func(context.Context, baml_sdk.EnumsEnums) (baml_sdk.EnumsEnums, error)                                                     = baml_sdk.EnumsRoundTripEnums
	_ func(context.Context, baml_sdk.MapsSentiment) (baml_sdk.MapsSentiment, error)                                               = baml_sdk.MapsRoundTripSentiment
	_ func(context.Context, baml_sdk.GoCodegenEnumEdgesResponseState) (baml_sdk.GoCodegenEnumEdgesResponseState, error)           = baml_sdk.GoCodegenEnumEdgesRoundTripState
	_ func(context.Context, *baml_sdk.GoCodegenEnumEdgesResponseState) (*baml_sdk.GoCodegenEnumEdgesResponseState, error)         = baml_sdk.GoCodegenEnumEdgesRoundTripOptionalState
	_ func(context.Context, baml_sdk.GoCodegenEnumEdgesEnumMatrix) (baml_sdk.GoCodegenEnumEdgesEnumMatrix, error)                 = baml_sdk.GoCodegenEnumEdgesRoundTripMatrix
	_ func(context.Context, ...baml_sdk.GoCodegenEnumEdgesDefaultedStateOption) (baml_sdk.GoCodegenEnumEdgesResponseState, error) = baml_sdk.GoCodegenEnumEdgesDefaultedState
	_ func(baml_sdk.GoCodegenEnumEdgesResponseState) baml_sdk.GoCodegenEnumEdgesDefaultedStateOption                              = baml_sdk.WithGoCodegenEnumEdgesDefaultedStateValue

	_ func(context.Context, baml_sdk.AliasesStringList) (baml_sdk.AliasesStringList, error)                                 = baml_sdk.AliasesRoundTripStringList
	_ func(context.Context, baml_sdk.GoCodegenAliasEdgesTextChain) (baml_sdk.GoCodegenAliasEdgesTextChain, error)           = baml_sdk.GoCodegenAliasEdgesRoundTripText
	_ func(context.Context, *baml_sdk.GoCodegenAliasEdgesTextChain) (*baml_sdk.GoCodegenAliasEdgesTextChain, error)         = baml_sdk.GoCodegenAliasEdgesRoundTripOptionalText
	_ func(context.Context, baml_sdk.GoCodegenAliasEdgesOptionalState) (baml_sdk.GoCodegenAliasEdgesOptionalState, error)   = baml_sdk.GoCodegenAliasEdgesRoundTripOptionalState
	_ func(context.Context, baml_sdk.GoCodegenAliasEdgesBigNumber) (baml_sdk.GoCodegenAliasEdgesBigNumber, error)           = baml_sdk.GoCodegenAliasEdgesRoundTripOptionalBigNumber
	_ func(context.Context, baml_sdk.GoCodegenAliasEdgesStates) (baml_sdk.GoCodegenAliasEdgesStates, error)                 = baml_sdk.GoCodegenAliasEdgesRoundTripStates
	_ func(context.Context, baml_sdk.GoCodegenAliasEdgesStatesByKey) (baml_sdk.GoCodegenAliasEdgesStatesByKey, error)       = baml_sdk.GoCodegenAliasEdgesRoundTripStatesByKey
	_ func(context.Context, baml_sdk.GoCodegenAliasEdgesOptionalStates) (baml_sdk.GoCodegenAliasEdgesOptionalStates, error) = baml_sdk.GoCodegenAliasEdgesRoundTripOptionalStates
	_ func(context.Context, ...baml_sdk.GoCodegenAliasEdgesDefaultedStateOption) (baml_sdk.GoCodegenAliasEdgesState, error) = baml_sdk.GoCodegenAliasEdgesDefaultedState
	_ func(baml_sdk.GoCodegenAliasEdgesState) baml_sdk.GoCodegenAliasEdgesDefaultedStateOption                              = baml_sdk.WithGoCodegenAliasEdgesDefaultedStateValue
)

var (
	_ = baml_sdk.PrimitivesPrimitives{
		IntField:        1,
		FloatField:      1.5,
		StringField:     "value",
		BoolField:       true,
		NullField:       baml_go.Null{},
		Uint8arrayField: []byte{1},
	}
	_ = baml_sdk.GoCodegenPrimitiveEdgesPrimitiveHolder{
		BigValue: big.NewInt(1),
	}
	_ = baml_sdk.ClassRefsOuter{
		Inner: baml_sdk.ClassRefsInner{Value: 1},
	}
	_ = baml_sdk.RecursionIntBinaryTree{
		Left: &baml_sdk.RecursionIntBinaryTree{},
	}
	_ = baml_sdk.RecursionA{
		B: &baml_sdk.RecursionB{},
	}
	_ = baml_sdk.GoCodegenPrimitiveEdgesNullableContainers{
		OptionalList:       new([]string),
		OptionalMap:        new(map[string]int64),
		OptionalElements:   []*string{nil},
		OptionalValues:     map[string]*int64{"null": nil},
		NestedLists:        [][]int64{{1}},
		MapOfOptionalLists: map[string][]*string{"items": []*string{nil}},
	}
	_ = baml_sdk.GoCodegenPrimitiveEdgesContainerTree{
		Children: []baml_sdk.GoCodegenPrimitiveEdgesContainerTree{{Value: 1}},
	}
)

func Test_primitive_class_round_trip(t *testing.T) {
	want := baml_sdk.GoCodegenPrimitiveEdgesWirePrimitives{
		StringValue: "wire",
		IntValue:    42,
		BigintValue: new(big.Int).Lsh(big.NewInt(1), 80),
		FloatValue:  3.5,
		BoolValue:   true,
		NullValue:   baml_go.Null{},
		BytesValue:  []byte{0, 1, 127, 255},
	}
	got, err := baml_sdk.GoCodegenPrimitiveEdgesRoundTripWirePrimitives(context.Background(), want)
	if err != nil {
		t.Fatal(err)
	}
	if got.StringValue != want.StringValue ||
		got.IntValue != want.IntValue ||
		got.BigintValue.Cmp(want.BigintValue) != 0 ||
		got.FloatValue != want.FloatValue ||
		got.BoolValue != want.BoolValue ||
		!bytes.Equal(got.BytesValue, want.BytesValue) {
		t.Fatalf("round trip = %#v, want %#v", got, want)
	}
}

func Test_nested_class_round_trip(t *testing.T) {
	want := baml_sdk.ClassRefsOuter{Inner: baml_sdk.ClassRefsInner{Value: 42}}
	got, err := baml_sdk.ClassRefsRoundTripOuter(context.Background(), want)
	if err != nil {
		t.Fatal(err)
	}
	if got != want {
		t.Fatalf("round trip = %#v, want %#v", got, want)
	}

	made, err := baml_sdk.ClassRefsMakeOuter(context.Background(), 73)
	if err != nil {
		t.Fatal(err)
	}
	if made.Inner.Value != 73 {
		t.Fatalf("MakeOuter() = %#v, want nested value 73", made)
	}
}

func Test_optional_top_level_round_trips(t *testing.T) {
	stringValue := "value"
	intValue := int64(42)
	bigintValue := new(big.Int).Lsh(big.NewInt(1), 80)
	floatValue := 3.5
	boolValue := true
	bytesValue := []byte{0, 1, 255}
	classValue := baml_sdk.GoCodegenPrimitiveEdgesWirePrimitives{
		StringValue: "nested",
		IntValue:    7,
		BigintValue: big.NewInt(99),
		FloatValue:  1.5,
		BoolValue:   true,
		NullValue:   baml_go.Null{},
		BytesValue:  []byte{3, 2, 1},
	}

	assertOptionalRoundTrip(t, baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalString, &stringValue)
	assertOptionalRoundTrip(t, baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalInt, &intValue)
	assertOptionalRoundTrip(t, baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalBigint, bigintValue)
	assertOptionalRoundTrip(t, baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalFloat, &floatValue)
	assertOptionalRoundTrip(t, baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalBool, &boolValue)
	assertOptionalRoundTrip(t, baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalBytes, &bytesValue)
	assertOptionalRoundTrip(t, baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalClass, &classValue)
}

func Test_nullable_class_fields_round_trip(t *testing.T) {
	empty, err := baml_sdk.GoCodegenPrimitiveEdgesRoundTripNullableWire(
		context.Background(),
		baml_sdk.GoCodegenPrimitiveEdgesNullableWire{},
	)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(empty, baml_sdk.GoCodegenPrimitiveEdgesNullableWire{}) {
		t.Fatalf("null fields round trip = %#v", empty)
	}

	stringValue := "value"
	intValue := int64(42)
	bigintValue := big.NewInt(123456789)
	floatValue := 3.5
	boolValue := true
	bytesValue := []byte{9, 8, 7}
	classValue := baml_sdk.GoCodegenPrimitiveEdgesWirePrimitives{
		StringValue: "nested",
		IntValue:    7,
		BigintValue: big.NewInt(99),
		FloatValue:  1.5,
		BoolValue:   true,
		NullValue:   baml_go.Null{},
		BytesValue:  []byte{3, 2, 1},
	}
	want := baml_sdk.GoCodegenPrimitiveEdgesNullableWire{
		StringValue: &stringValue,
		IntValue:    &intValue,
		BigintValue: bigintValue,
		FloatValue:  &floatValue,
		BoolValue:   &boolValue,
		BytesValue:  &bytesValue,
		ClassValue:  &classValue,
	}
	got, err := baml_sdk.GoCodegenPrimitiveEdgesRoundTripNullableWire(context.Background(), want)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("non-null fields round trip = %#v, want %#v", got, want)
	}
}

func Test_nullable_recursive_classes_round_trip(t *testing.T) {
	tree := baml_sdk.RecursionIntBinaryTree{
		Value: 1,
		Left:  &baml_sdk.RecursionIntBinaryTree{Value: 2},
		Right: &baml_sdk.RecursionIntBinaryTree{
			Value: 3,
			Left:  &baml_sdk.RecursionIntBinaryTree{Value: 4},
		},
	}
	gotTree, err := baml_sdk.RecursionRoundTripIntBinaryTree(context.Background(), tree)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(gotTree, tree) {
		t.Fatalf("tree round trip = %#v, want %#v", gotTree, tree)
	}

	mutual := baml_sdk.RecursionA{B: &baml_sdk.RecursionB{}}
	gotMutual, err := baml_sdk.RecursionRoundTripA(context.Background(), mutual)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(gotMutual, mutual) {
		t.Fatalf("mutual round trip = %#v, want %#v", gotMutual, mutual)
	}
}

func Test_required_container_round_trips(t *testing.T) {
	ints := []int64{1, 2, 3}
	gotInts, err := baml_sdk.ListsRoundTripInts(context.Background(), ints)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(gotInts, ints) {
		t.Fatalf("int list round trip = %#v, want %#v", gotInts, ints)
	}

	value := "present"
	optionalStrings := []*string{nil, &value, nil}
	gotStrings, err := baml_sdk.ListsRoundTripOptionalStrings(context.Background(), optionalStrings)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(gotStrings, optionalStrings) {
		t.Fatalf("nullable-element list round trip = %#v, want %#v", gotStrings, optionalStrings)
	}

	simpleMap := map[string]int64{"": 0, "one": 1}
	gotSimpleMap, err := baml_sdk.MapsRoundTripSimpleMap(context.Background(), simpleMap)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(gotSimpleMap, simpleMap) {
		t.Fatalf("simple map round trip = %#v, want %#v", gotSimpleMap, simpleMap)
	}

	classMap := map[string]baml_sdk.MapsResume{
		"first":  {Name: "Ada"},
		"second": {Name: "Grace"},
	}
	gotClassMap, err := baml_sdk.MapsRoundTripEnumKeyedMap(context.Background(), classMap)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(gotClassMap, classMap) {
		t.Fatalf("class map round trip = %#v, want %#v", gotClassMap, classMap)
	}

	listMap := map[string][]int64{"empty": {}, "values": {4, 5}}
	gotListMap, err := baml_sdk.MapsRoundTripListValuedMap(context.Background(), listMap)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(gotListMap, listMap) {
		t.Fatalf("list-valued map round trip = %#v, want %#v", gotListMap, listMap)
	}

	container := baml_sdk.MapsMapContainer{
		Simple:     simpleMap,
		EnumKeyed:  classMap,
		ListValued: listMap,
	}
	gotContainer, err := baml_sdk.MapsRoundTripMapContainer(context.Background(), container)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(gotContainer, container) {
		t.Fatalf("map container round trip = %#v, want %#v", gotContainer, container)
	}
}

func Test_nullable_container_boundaries_round_trip(t *testing.T) {
	if got, err := baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalList(context.Background(), nil); err != nil || got != nil {
		t.Fatalf("null list round trip = %#v, %v", got, err)
	}
	list := []int64{1, 2, 3}
	gotList, err := baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalList(context.Background(), &list)
	if err != nil {
		t.Fatal(err)
	}
	if gotList == nil || !reflect.DeepEqual(*gotList, list) {
		t.Fatalf("present list round trip = %#v, want %#v", gotList, list)
	}
	nilList := []int64(nil)
	gotEmptyList, err := baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalList(context.Background(), &nilList)
	if err != nil {
		t.Fatal(err)
	}
	if gotEmptyList == nil || *gotEmptyList == nil || len(*gotEmptyList) != 0 {
		t.Fatalf("present nil slice round trip = %#v, want pointer to present empty slice", gotEmptyList)
	}

	if got, err := baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalMap(context.Background(), nil); err != nil || got != nil {
		t.Fatalf("null map round trip = %#v, %v", got, err)
	}
	mapValue := map[string]int64{"answer": 42}
	gotMap, err := baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalMap(context.Background(), &mapValue)
	if err != nil {
		t.Fatal(err)
	}
	if gotMap == nil || !reflect.DeepEqual(*gotMap, mapValue) {
		t.Fatalf("present map round trip = %#v, want %#v", gotMap, mapValue)
	}
	var nilMap map[string]int64
	gotEmptyMap, err := baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalMap(context.Background(), &nilMap)
	if err != nil {
		t.Fatal(err)
	}
	if gotEmptyMap == nil || *gotEmptyMap == nil || len(*gotEmptyMap) != 0 {
		t.Fatalf("present nil map round trip = %#v, want pointer to present empty map", gotEmptyMap)
	}

	text := "value"
	listOfOptional := []*string{nil, &text, nil}
	gotListOfOptional, err := baml_sdk.GoCodegenPrimitiveEdgesRoundTripOptionalListOfOptional(
		context.Background(),
		&listOfOptional,
	)
	if err != nil {
		t.Fatal(err)
	}
	if gotListOfOptional == nil || !reflect.DeepEqual(*gotListOfOptional, listOfOptional) {
		t.Fatalf("optional list of optional values = %#v, want %#v", gotListOfOptional, listOfOptional)
	}

	assertOptionalRoundTrip(t, baml_sdk.GoCodegenPrimitiveEdgesRoundTripRepeatedNull, &text)
}

func Test_nullable_container_fields_round_trip(t *testing.T) {
	text := "present"
	integer := int64(7)
	list := []string{"one", "two"}
	mapValue := map[string]int64{"answer": 42}
	want := baml_sdk.GoCodegenPrimitiveEdgesNullableContainers{
		OptionalList:       &list,
		OptionalMap:        &mapValue,
		OptionalElements:   []*string{nil, &text},
		OptionalValues:     map[string]*int64{"null": nil, "present": &integer},
		NestedLists:        [][]int64{{1, 2}, {}, {3}},
		MapOfOptionalLists: map[string][]*string{"values": {nil, &text}},
	}
	got, err := baml_sdk.GoCodegenPrimitiveEdgesRoundTripNullableContainers(context.Background(), want)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("nullable container fields round trip = %#v, want %#v", got, want)
	}

	empty, err := baml_sdk.GoCodegenPrimitiveEdgesRoundTripNullableContainers(
		context.Background(),
		baml_sdk.GoCodegenPrimitiveEdgesNullableContainers{},
	)
	if err != nil {
		t.Fatal(err)
	}
	if empty.OptionalList != nil || empty.OptionalMap != nil {
		t.Fatalf("null container fields round trip = %#v", empty)
	}
}

func Test_every_supported_leaf_through_lists_and_maps(t *testing.T) {
	stringValue := "optional"
	intValue := int64(8)
	floatValue := 2.5
	boolValue := true
	bytesValue := []byte{7, 8, 9}
	classValue := baml_sdk.GoCodegenPrimitiveEdgesWirePrimitives{
		StringValue: "class",
		IntValue:    9,
		BigintValue: big.NewInt(10),
		FloatValue:  3.5,
		BoolValue:   true,
		NullValue:   baml_go.Null{},
		BytesValue:  []byte{1, 2, 3},
	}
	want := baml_sdk.GoCodegenPrimitiveEdgesContainerLeafMatrix{
		ListStrings:         []string{"value"},
		ListInts:            []int64{1},
		ListBigints:         []*big.Int{big.NewInt(2)},
		ListFloats:          []float64{1.5},
		ListBools:           []bool{true},
		ListNulls:           []baml_go.Null{{}},
		ListBytes:           [][]byte{{3, 4}},
		ListClasses:         []baml_sdk.GoCodegenPrimitiveEdgesWirePrimitives{classValue},
		ListOptionalStrings: []*string{nil, &stringValue},
		ListOptionalInts:    []*int64{nil, &intValue},
		ListOptionalBigints: []*big.Int{nil, big.NewInt(11)},
		ListOptionalFloats:  []*float64{nil, &floatValue},
		ListOptionalBools:   []*bool{nil, &boolValue},
		ListOptionalBytes:   []*[]byte{nil, &bytesValue},
		ListOptionalClasses: []*baml_sdk.GoCodegenPrimitiveEdgesWirePrimitives{nil, &classValue},
		MapStrings:          map[string]string{"value": "string"},
		MapInts:             map[string]int64{"value": 12},
		MapBigints:          map[string]*big.Int{"value": big.NewInt(13)},
		MapFloats:           map[string]float64{"value": 4.5},
		MapBools:            map[string]bool{"value": true},
		MapNulls:            map[string]baml_go.Null{"value": {}},
		MapBytes:            map[string][]byte{"value": {5, 6}},
		MapClasses:          map[string]baml_sdk.GoCodegenPrimitiveEdgesWirePrimitives{"value": classValue},
		MapOptionalStrings:  map[string]*string{"null": nil, "value": &stringValue},
		MapOptionalInts:     map[string]*int64{"null": nil, "value": &intValue},
		MapOptionalBigints:  map[string]*big.Int{"null": nil, "value": big.NewInt(14)},
		MapOptionalFloats:   map[string]*float64{"null": nil, "value": &floatValue},
		MapOptionalBools:    map[string]*bool{"null": nil, "value": &boolValue},
		MapOptionalBytes:    map[string]*[]byte{"null": nil, "value": &bytesValue},
		MapOptionalClasses: map[string]*baml_sdk.GoCodegenPrimitiveEdgesWirePrimitives{
			"null": nil, "value": &classValue,
		},
	}
	got, err := baml_sdk.GoCodegenPrimitiveEdgesRoundTripContainerLeafMatrix(context.Background(), want)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("container leaf matrix round trip = %#v, want %#v", got, want)
	}
}

func Test_container_recursive_class_round_trip(t *testing.T) {
	want := baml_sdk.GoCodegenPrimitiveEdgesContainerTree{
		Value: 1,
		Children: []baml_sdk.GoCodegenPrimitiveEdgesContainerTree{
			{Value: 2, Children: []baml_sdk.GoCodegenPrimitiveEdgesContainerTree{}},
			{
				Value: 3,
				Children: []baml_sdk.GoCodegenPrimitiveEdgesContainerTree{
					{Value: 4, Children: []baml_sdk.GoCodegenPrimitiveEdgesContainerTree{}},
				},
			},
		},
	}
	got, err := baml_sdk.GoCodegenPrimitiveEdgesRoundTripContainerTree(context.Background(), want)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("container-recursive class round trip = %#v, want %#v", got, want)
	}
}

func Test_declared_enum_functions_and_class_round_trip(t *testing.T) {
	positive, err := baml_sdk.EnumsPickSentiment(context.Background(), true)
	if err != nil || positive != baml_sdk.EnumsSentimentPositive {
		t.Fatalf("PickSentiment(true) = %q, %v", positive, err)
	}
	negative, err := baml_sdk.EnumsPickSentiment(context.Background(), false)
	if err != nil || negative != baml_sdk.EnumsSentimentNegative {
		t.Fatalf("PickSentiment(false) = %q, %v", negative, err)
	}
	picked, err := baml_sdk.EnumsPickPositive(context.Background())
	if err != nil || picked != baml_sdk.EnumsSentimentPositive {
		t.Fatalf("PickPositive() = %q, %v", picked, err)
	}

	roundTripped, err := baml_sdk.EnumsRoundTripSentiment(
		context.Background(),
		baml_sdk.EnumsSentimentNegative,
	)
	if err != nil || roundTripped != baml_sdk.EnumsSentimentNegative {
		t.Fatalf("RoundTripSentiment() = %q, %v", roundTripped, err)
	}
	roundTripped, err = baml_sdk.EnumsRoundTripSentimentPositive(
		context.Background(),
		baml_sdk.EnumsSentimentPositive,
	)
	if err != nil || roundTripped != baml_sdk.EnumsSentimentPositive {
		t.Fatalf("RoundTripSentimentPositive() = %q, %v", roundTripped, err)
	}

	wantClass := baml_sdk.EnumsEnums{
		BareEnum:      baml_sdk.EnumsSentimentNegative,
		VariantAsType: baml_sdk.EnumsSentimentPositive,
	}
	gotClass, err := baml_sdk.EnumsRoundTripEnums(context.Background(), wantClass)
	if err != nil || gotClass != wantClass {
		t.Fatalf("RoundTripEnums() = %#v, %v, want %#v", gotClass, err, wantClass)
	}

	mapEnum, err := baml_sdk.MapsRoundTripSentiment(
		context.Background(),
		baml_sdk.MapsSentimentPositive,
	)
	if err != nil || mapEnum != baml_sdk.MapsSentimentPositive {
		t.Fatalf("MapsRoundTripSentiment() = %q, %v", mapEnum, err)
	}
}

func Test_enum_composition_matrix_round_trip(t *testing.T) {
	pending := baml_sdk.GoCodegenEnumEdgesResponseStatePendingReview
	accepted := baml_sdk.GoCodegenEnumEdgesResponseStateAccepted
	httpError := baml_sdk.GoCodegenEnumEdgesResponseStateHTTPError
	if string(httpError) != "HTTP_error" {
		t.Fatalf("HTTP variant wire value = %q", httpError)
	}

	optionalList := []baml_sdk.GoCodegenEnumEdgesResponseState{accepted, pending}
	optionalMap := map[string]baml_sdk.GoCodegenEnumEdgesResponseState{"error": httpError}
	want := baml_sdk.GoCodegenEnumEdgesEnumMatrix{
		State:                httpError,
		OptionalState:        &accepted,
		States:               []baml_sdk.GoCodegenEnumEdgesResponseState{pending, accepted, httpError},
		OptionalStates:       []*baml_sdk.GoCodegenEnumEdgesResponseState{nil, &pending, &httpError},
		StatesByName:         map[string]baml_sdk.GoCodegenEnumEdgesResponseState{"pending": pending, "accepted": accepted},
		OptionalStatesByName: map[string]*baml_sdk.GoCodegenEnumEdgesResponseState{"nil": nil, "value": &httpError},
		OptionalList:         &optionalList,
		OptionalMap:          &optionalMap,
	}
	got, err := baml_sdk.GoCodegenEnumEdgesRoundTripMatrix(context.Background(), want)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("enum matrix = %#v, want %#v", got, want)
	}

	empty, err := baml_sdk.GoCodegenEnumEdgesRoundTripMatrix(
		context.Background(),
		baml_sdk.GoCodegenEnumEdgesEnumMatrix{
			State:                pending,
			States:               []baml_sdk.GoCodegenEnumEdgesResponseState{},
			OptionalStates:       []*baml_sdk.GoCodegenEnumEdgesResponseState{},
			StatesByName:         map[string]baml_sdk.GoCodegenEnumEdgesResponseState{},
			OptionalStatesByName: map[string]*baml_sdk.GoCodegenEnumEdgesResponseState{},
		},
	)
	if err != nil || empty.OptionalState != nil || empty.OptionalList != nil || empty.OptionalMap != nil {
		t.Fatalf("null enum boundaries = %#v, %v", empty, err)
	}

	assertOptionalRoundTrip(
		t,
		baml_sdk.GoCodegenEnumEdgesRoundTripOptionalState,
		&accepted,
	)
}

func Test_defaulted_enum_argument_and_invalid_values(t *testing.T) {
	pending := baml_sdk.GoCodegenEnumEdgesResponseStatePendingReview
	httpError := baml_sdk.GoCodegenEnumEdgesResponseStateHTTPError

	got, err := baml_sdk.GoCodegenEnumEdgesDefaultedState(context.Background())
	if err != nil || got != pending {
		t.Fatalf("omitted default = %q, %v", got, err)
	}
	got, err = baml_sdk.GoCodegenEnumEdgesDefaultedState(
		context.Background(),
		baml_sdk.WithGoCodegenEnumEdgesDefaultedStateValue(httpError),
	)
	if err != nil || got != httpError {
		t.Fatalf("explicit default override = %q, %v", got, err)
	}

	invalid := baml_sdk.GoCodegenEnumEdgesResponseState("not_declared")
	if _, err := baml_sdk.GoCodegenEnumEdgesRoundTripState(context.Background(), invalid); err == nil || !strings.Contains(err.Error(), `invalid BAML enum "user.go_codegen.enum_edges.ResponseState" variant "not_declared"`) {
		t.Fatalf("invalid direct enum error = %v", err)
	}
	if _, err := baml_sdk.GoCodegenEnumEdgesDefaultedState(
		context.Background(),
		baml_sdk.WithGoCodegenEnumEdgesDefaultedStateValue(invalid),
	); err == nil || !strings.Contains(err.Error(), `invalid BAML enum "user.go_codegen.enum_edges.ResponseState" variant "not_declared"`) {
		t.Fatalf("invalid option enum error = %v", err)
	}
	if _, err := baml_sdk.GoCodegenEnumEdgesRoundTripMatrix(
		context.Background(),
		baml_sdk.GoCodegenEnumEdgesEnumMatrix{
			State:                pending,
			States:               []baml_sdk.GoCodegenEnumEdgesResponseState{invalid},
			OptionalStates:       []*baml_sdk.GoCodegenEnumEdgesResponseState{},
			StatesByName:         map[string]baml_sdk.GoCodegenEnumEdgesResponseState{},
			OptionalStatesByName: map[string]*baml_sdk.GoCodegenEnumEdgesResponseState{},
		},
	); err == nil || !strings.Contains(err.Error(), `variant "not_declared"`) {
		t.Fatalf("invalid nested enum error = %v", err)
	}
}

func Test_enum_package_scope_collisions_compile_and_run(t *testing.T) {
	_ = baml_sdk.GoCodegenEnumEdgesCollisionItem_737da72b
	_ = baml_sdk.GoCodegenEnumEdgesCollisionItem_3b21dc73{}
	got, err := baml_sdk.GoCodegenEnumEdgesCollisionItem_40cc0a70(context.Background())
	if err != nil || got != "function" {
		t.Fatalf("colliding function = %q, %v", got, err)
	}
}

func Test_transparent_alias_functions_round_trip(t *testing.T) {
	list := baml_sdk.AliasesStringList{"first", "second"}
	gotList, err := baml_sdk.AliasesRoundTripStringList(context.Background(), list)
	if err != nil || !reflect.DeepEqual(gotList, list) {
		t.Fatalf("alias list = %#v, %v, want %#v", gotList, err, list)
	}

	text := baml_sdk.GoCodegenAliasEdgesTextChain("hello")
	gotText, err := baml_sdk.GoCodegenAliasEdgesRoundTripText(context.Background(), text)
	if err != nil || gotText != text {
		t.Fatalf("alias chain = %q, %v", gotText, err)
	}
	gotOptionalText, err := baml_sdk.GoCodegenAliasEdgesRoundTripOptionalText(
		context.Background(),
		&text,
	)
	if err != nil || gotOptionalText == nil || *gotOptionalText != text {
		t.Fatalf("optional alias chain = %#v, %v", gotOptionalText, err)
	}
	gotOptionalText, err = baml_sdk.GoCodegenAliasEdgesRoundTripOptionalText(
		context.Background(),
		nil,
	)
	if err != nil || gotOptionalText != nil {
		t.Fatalf("null optional alias chain = %#v, %v", gotOptionalText, err)
	}

	state := baml_sdk.GoCodegenAliasEdgesState(
		baml_sdk.GoCodegenEnumEdgesResponseStateAccepted,
	)
	gotState, err := baml_sdk.GoCodegenAliasEdgesRoundTripOptionalState(
		context.Background(),
		&state,
	)
	if err != nil || gotState == nil || *gotState != state {
		t.Fatalf("nullable alias over nullable enum = %#v, %v", gotState, err)
	}
	gotState, err = baml_sdk.GoCodegenAliasEdgesRoundTripOptionalState(
		context.Background(),
		nil,
	)
	if err != nil || gotState != nil {
		t.Fatalf("null alias over nullable enum = %#v, %v", gotState, err)
	}

	bigNumber := baml_sdk.GoCodegenAliasEdgesBigNumber(big.NewInt(123456789))
	gotBigNumber, err := baml_sdk.GoCodegenAliasEdgesRoundTripOptionalBigNumber(
		context.Background(),
		bigNumber,
	)
	if err != nil || gotBigNumber == nil || gotBigNumber.Cmp(bigNumber) != 0 {
		t.Fatalf("optional bigint alias = %v, %v", gotBigNumber, err)
	}
	gotBigNumber, err = baml_sdk.GoCodegenAliasEdgesRoundTripOptionalBigNumber(
		context.Background(),
		nil,
	)
	if err != nil || gotBigNumber != nil {
		t.Fatalf("null bigint alias = %v, %v", gotBigNumber, err)
	}
}

func Test_alias_container_composition_and_defaults(t *testing.T) {
	pending := baml_sdk.GoCodegenAliasEdgesState(
		baml_sdk.GoCodegenEnumEdgesResponseStatePendingReview,
	)
	httpError := baml_sdk.GoCodegenAliasEdgesState(
		baml_sdk.GoCodegenEnumEdgesResponseStateHTTPError,
	)
	states := baml_sdk.GoCodegenAliasEdgesStates{pending, httpError}
	statesByKey := baml_sdk.GoCodegenAliasEdgesStatesByKey{
		baml_sdk.GoCodegenAliasEdgesKey("error"): httpError,
	}
	gotStates, err := baml_sdk.GoCodegenAliasEdgesRoundTripStates(context.Background(), states)
	if err != nil || !reflect.DeepEqual(gotStates, states) {
		t.Fatalf("aliased list = %#v, %v, want %#v", gotStates, err, states)
	}
	gotByKey, err := baml_sdk.GoCodegenAliasEdgesRoundTripStatesByKey(
		context.Background(),
		statesByKey,
	)
	if err != nil || !reflect.DeepEqual(gotByKey, statesByKey) {
		t.Fatalf("aliased map key = %#v, %v, want %#v", gotByKey, err, statesByKey)
	}
	gotOptionalStates, err := baml_sdk.GoCodegenAliasEdgesRoundTripOptionalStates(
		context.Background(),
		&states,
	)
	if err != nil || gotOptionalStates == nil || !reflect.DeepEqual(*gotOptionalStates, states) {
		t.Fatalf("optional aliased list = %#v, %v", gotOptionalStates, err)
	}
	gotOptionalStates, err = baml_sdk.GoCodegenAliasEdgesRoundTripOptionalStates(
		context.Background(),
		nil,
	)
	if err != nil || gotOptionalStates != nil {
		t.Fatalf("null optional aliased list = %#v, %v", gotOptionalStates, err)
	}

	defaulted, err := baml_sdk.GoCodegenAliasEdgesDefaultedState(context.Background())
	if err != nil || defaulted != pending {
		t.Fatalf("defaulted alias = %q, %v", defaulted, err)
	}
	defaulted, err = baml_sdk.GoCodegenAliasEdgesDefaultedState(
		context.Background(),
		baml_sdk.WithGoCodegenAliasEdgesDefaultedStateValue(httpError),
	)
	if err != nil || defaulted != httpError {
		t.Fatalf("overridden alias default = %q, %v", defaulted, err)
	}

	invalid := baml_sdk.GoCodegenAliasEdgesState("not_declared")
	if _, err := baml_sdk.GoCodegenAliasEdgesDefaultedState(
		context.Background(),
		baml_sdk.WithGoCodegenAliasEdgesDefaultedStateValue(invalid),
	); err == nil || !strings.Contains(err.Error(), `variant "not_declared"`) {
		t.Fatalf("invalid enum alias error = %v", err)
	}
}

func Test_alias_package_scope_collisions_compile_and_run(t *testing.T) {
	_ = baml_sdk.GoCodegenAliasEdgesCollisionItem_ea5961c6("alias")
	_ = baml_sdk.GoCodegenAliasEdgesCollisionItem_f279d073
	got, err := baml_sdk.GoCodegenAliasEdgesCollisionItem_77850bc8(context.Background())
	if err != nil || got != "function" {
		t.Fatalf("colliding alias function = %q, %v", got, err)
	}
}

func assertOptionalRoundTrip[T any](
	t *testing.T,
	roundTrip func(context.Context, *T) (*T, error),
	want *T,
) {
	t.Helper()
	for _, value := range []*T{nil, want} {
		got, err := roundTrip(context.Background(), value)
		if err != nil {
			t.Fatal(err)
		}
		if !reflect.DeepEqual(got, value) {
			t.Fatalf("round trip = %#v, want %#v", got, value)
		}
	}
}
