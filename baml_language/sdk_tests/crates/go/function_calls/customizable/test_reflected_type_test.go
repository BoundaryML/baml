package sdk_test

import (
	"context"
	"testing"

	"baml.local/sdk/baml_sdk"
	baml_go "github.com/boundaryml/baml-go"
)

func Test_reflected_type_top_level_and_runtime_produced_values(t *testing.T) {
	ctx := context.Background()
	integer := baml_go.PrimitiveBAMLType(baml_go.IntType)
	got, err := baml_sdk.GoTypeTestsRoundTripType(ctx, integer)
	if err != nil || !got.Equal(integer) {
		t.Fatalf("type round trip = %#v, %v", got, err)
	}
	name, err := baml_sdk.GoTypeTestsTypeName(ctx, baml_go.ListBAMLType(integer))
	if err != nil || name != "int[]" {
		t.Fatalf("type name = %q, %v", name, err)
	}

	got, err = baml_sdk.GoTypeTestsReflectedInt(ctx)
	if err != nil || !got.Equal(integer) {
		t.Fatalf("reflected int = %#v, %v", got, err)
	}
	got, err = baml_sdk.GoTypeTestsReflectedContainer(ctx)
	wantContainer := baml_go.MapBAMLType(
		baml_go.PrimitiveBAMLType(baml_go.StringType),
		baml_go.ListBAMLType(integer),
	)
	if err != nil || !got.Equal(wantContainer) {
		t.Fatalf("reflected container = %#v, %v", got, err)
	}
	got, err = baml_sdk.GoTypeTestsReflectedUnion(ctx)
	wantUnion := baml_go.UnionBAMLType(integer, baml_go.PrimitiveBAMLType(baml_go.StringType))
	if err != nil || !got.Equal(wantUnion) {
		t.Fatalf("reflected union = %#v, %v", got, err)
	}
}

func Test_reflected_type_primitive_literal_and_nominal_descriptors(t *testing.T) {
	ctx := context.Background()
	primitives, err := baml_sdk.GoTypeTestsReflectedPrimitives(ctx)
	wantPrimitives := []baml_go.BAMLType{
		baml_go.PrimitiveBAMLType(baml_go.StringType),
		baml_go.PrimitiveBAMLType(baml_go.IntType),
		baml_go.PrimitiveBAMLType(baml_go.BigintType),
		baml_go.PrimitiveBAMLType(baml_go.FloatType),
		baml_go.PrimitiveBAMLType(baml_go.BoolType),
		baml_go.PrimitiveBAMLType(baml_go.NullType),
		baml_go.PrimitiveBAMLType(baml_go.BytesType),
		baml_go.MetaTypeBAMLType(),
	}
	if err != nil || !equalTypeSlices(primitives, wantPrimitives) {
		t.Fatalf("primitive descriptors = %#v, %v", primitives, err)
	}

	literals, err := baml_sdk.GoTypeTestsReflectedLiterals(ctx)
	wantLiterals := []baml_go.BAMLType{
		baml_go.StringLiteralBAMLType("literal"),
		baml_go.IntLiteralBAMLType(42),
		baml_go.BigintLiteralBAMLType("42"),
		baml_go.BoolLiteralBAMLType(true),
	}
	if err != nil || !equalTypeSlices(literals, wantLiterals) {
		t.Fatalf("literal descriptors = %#v, %v", literals, err)
	}

	classType, err := baml_sdk.GoTypeTestsGetReflectedClass(ctx)
	if err != nil || !classType.Equal(baml_go.ClassBAMLType("user.go_type_tests.ReflectedClass")) {
		t.Fatalf("class descriptor = %#v, %v", classType, err)
	}
	enumType, err := baml_sdk.GoTypeTestsGetReflectedEnum(ctx)
	if err != nil || !enumType.Equal(baml_go.EnumBAMLType("user.go_type_tests.ReflectedEnum")) {
		t.Fatalf("enum descriptor = %#v, %v", enumType, err)
	}
	aliasType, err := baml_sdk.GoTypeTestsGetReflectedAlias(ctx)
	if err != nil || !aliasType.Equal(baml_go.ListBAMLType(baml_go.PrimitiveBAMLType(baml_go.StringType))) {
		t.Fatalf("alias descriptor = %#v, %v", aliasType, err)
	}
	genericType, err := baml_sdk.GoTypeTestsReflectedGenericClass(ctx)
	wantGeneric := baml_go.ClassBAMLType("user.generic_tests.GenericBox", baml_go.PrimitiveBAMLType(baml_go.IntType))
	if err != nil || !genericType.Equal(wantGeneric) {
		t.Fatalf("generic class descriptor = %#v, %v", genericType, err)
	}
}

func Test_reflected_type_composes_through_optional_containers_and_classes(t *testing.T) {
	ctx := context.Background()
	integer := baml_go.PrimitiveBAMLType(baml_go.IntType)
	stringType := baml_go.PrimitiveBAMLType(baml_go.StringType)

	if got, err := baml_sdk.GoTypeTestsRoundTripOptionalType(ctx, nil); err != nil || got != nil {
		t.Fatalf("nil optional type = %#v, %v", got, err)
	}
	gotOptional, err := baml_sdk.GoTypeTestsRoundTripOptionalType(ctx, &integer)
	if err != nil || gotOptional == nil || !gotOptional.Equal(integer) {
		t.Fatalf("present optional type = %#v, %v", gotOptional, err)
	}
	if got, err := baml_sdk.GoTypeTestsDefaultOptionalType(ctx); err != nil || got != nil {
		t.Fatalf("default optional type = %#v, %v", got, err)
	}
	gotDefault, err := baml_sdk.GoTypeTestsDefaultOptionalType(
		ctx,
		baml_sdk.WithGoTypeTestsDefaultOptionalTypeValue(&integer),
	)
	if err != nil || gotDefault == nil || !gotDefault.Equal(integer) {
		t.Fatalf("explicit defaulted type = %#v, %v", gotDefault, err)
	}

	wantList := []baml_go.BAMLType{integer, baml_go.OptionalBAMLType(stringType)}
	gotList, err := baml_sdk.GoTypeTestsRoundTripTypeList(ctx, wantList)
	if err != nil || !equalTypeSlices(gotList, wantList) {
		t.Fatalf("type list = %#v, %v", gotList, err)
	}
	wantMap := map[string]baml_go.BAMLType{"int": integer, "string": stringType}
	gotMap, err := baml_sdk.GoTypeTestsRoundTripTypeMap(ctx, wantMap)
	if err != nil || !equalTypeMaps(gotMap, wantMap) {
		t.Fatalf("type map = %#v, %v", gotMap, err)
	}

	wantBox := baml_sdk.GoTypeTestsTypeBox{
		Direct:  integer,
		List:    wantList,
		Mapping: wantMap,
	}
	gotBox, err := baml_sdk.GoTypeTestsRoundTripTypeBox(ctx, wantBox)
	if err != nil || !gotBox.Direct.Equal(wantBox.Direct) || gotBox.Optional != nil ||
		!equalTypeSlices(gotBox.List, wantBox.List) || !equalTypeMaps(gotBox.Mapping, wantBox.Mapping) {
		t.Fatalf("type class = %#v, %v", gotBox, err)
	}
}

func Test_reflected_type_closed_dynamic_unions_and_callback(t *testing.T) {
	ctx := context.Background()
	integer := baml_go.PrimitiveBAMLType(baml_go.IntType)

	closed := baml_sdk.NewStringOrTypeFromType(integer)
	gotClosed, err := baml_sdk.GoTypeTestsRoundTripTypeOrString(ctx, closed)
	if err != nil {
		t.Fatal(err)
	}
	gotType, ok := gotClosed.AsType()
	if !ok || !gotType.Equal(integer) {
		t.Fatalf("closed union = %#v, %v", gotClosed, ok)
	}

	gotDynamic, err := baml_sdk.GoTypeTestsRoundTripDynamicTypeUnion(ctx, integer)
	if err != nil {
		t.Fatal(err)
	}
	dynamicType, ok := gotDynamic.(baml_go.BAMLType)
	if !ok || !dynamicType.Equal(integer) {
		t.Fatalf("dynamic union = %#v (%T)", gotDynamic, gotDynamic)
	}

	gotCallback, err := baml_sdk.GoTypeTestsCallTypeCallback(ctx, func(value baml_go.BAMLType) baml_go.BAMLType {
		if !value.Equal(integer) {
			t.Fatalf("callback input = %#v", value)
		}
		return baml_go.ListBAMLType(value)
	}, integer)
	if err != nil || !gotCallback.Equal(baml_go.ListBAMLType(integer)) {
		t.Fatalf("callback result = %#v, %v", gotCallback, err)
	}
}

func equalTypeSlices(left, right []baml_go.BAMLType) bool {
	if len(left) != len(right) {
		return false
	}
	for index := range left {
		if !left[index].Equal(right[index]) {
			return false
		}
	}
	return true
}

func equalTypeMaps(left, right map[string]baml_go.BAMLType) bool {
	if len(left) != len(right) {
		return false
	}
	for key, leftValue := range left {
		rightValue, ok := right[key]
		if !ok || !leftValue.Equal(rightValue) {
			return false
		}
	}
	return true
}
