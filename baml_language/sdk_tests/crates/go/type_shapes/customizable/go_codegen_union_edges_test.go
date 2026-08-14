package sdk_test

import (
	"context"
	"reflect"
	"strings"
	"testing"

	"baml.local/sdk/baml_sdk"
)

func Test_dynamic_union_candidates_round_trip_as_concrete_go_values(t *testing.T) {
	ctx := context.Background()
	cases := []any{int64(7), "seven", 7.5, true}
	for _, want := range cases {
		got, err := baml_sdk.GoCodegenUnionEdgesRoundTripLargeUnion(ctx, want)
		if err != nil || !reflect.DeepEqual(got, want) {
			t.Fatalf("%T(%v) -> %T(%v), %v", want, want, got, got, err)
		}
	}
}

func Test_dynamic_union_accepts_natural_go_integers(t *testing.T) {
	got, err := baml_sdk.GoCodegenUnionEdgesRoundTripLargeUnion(context.Background(), 7)
	if err != nil || got != int64(7) {
		t.Fatalf("got %T(%v), %v", got, got, err)
	}
}

func Test_dynamic_union_delegates_semantic_validation_to_baml(t *testing.T) {
	_, err := baml_sdk.GoCodegenUnionEdgesRoundTripLargeUnion(context.Background(), []string{"not", "an", "arm"})
	if err == nil {
		t.Fatal("expected BAML argument type-mismatch error")
	}
	if !strings.Contains(err.Error(), "BAML error") || !strings.Contains(err.Error(), "baml.errors.TypeMismatch") {
		t.Fatalf("unexpected error: %v", err)
	}
}

func Test_dynamic_union_rejects_unserializable_go_values_in_bridge(t *testing.T) {
	_, err := baml_sdk.GoCodegenUnionEdgesRoundTripLargeUnion(context.Background(), func() {})
	if err == nil || !strings.Contains(err.Error(), "unsupported Go value") {
		t.Fatalf("unexpected error: %v", err)
	}
}

func Test_dynamic_union_nested_containers_round_trip(t *testing.T) {
	values := []any{int64(1), "two", 3.5, false}
	gotValues, err := baml_sdk.GoCodegenUnionEdgesRoundTripLargeUnionList(context.Background(), values)
	if err != nil || !reflect.DeepEqual(gotValues, values) {
		t.Fatalf("list = %#v, %v", gotValues, err)
	}

	container := baml_sdk.GoCodegenUnionEdgesDynamicUnionContainer{
		Value:  "top",
		Values: values,
		Lookup: map[string]any{"integer": int64(1), "boolean": true},
	}
	gotContainer, err := baml_sdk.GoCodegenUnionEdgesRoundTripDynamicUnionContainer(context.Background(), container)
	if err != nil || !reflect.DeepEqual(gotContainer, container) {
		t.Fatalf("container = %#v, %v", gotContainer, err)
	}
}

func Test_dynamic_union_of_containers_uses_selected_type_metadata(t *testing.T) {
	cases := []any{
		[]int64{},
		[]string{},
		map[string]int64{},
		map[string]string{},
	}
	for _, want := range cases {
		got, err := baml_sdk.GoCodegenUnionEdgesRoundTripUnionOfContainers(context.Background(), want)
		if err != nil || !reflect.DeepEqual(got, want) {
			t.Fatalf("%T -> %T(%#v), %v", want, got, got, err)
		}
	}
}

func Test_union_aliases_are_transparent_and_flatten_before_thresholding(t *testing.T) {
	ctx := context.Background()
	small := baml_sdk.NewStringOrIntFromInt(9)
	gotSmall, err := baml_sdk.GoCodegenUnionEdgesRoundTripSmallAlias(ctx, small)
	if err != nil || gotSmall.Kind() != small.Kind() {
		t.Fatalf("small alias = %#v, %v", gotSmall, err)
	}

	composed := baml_sdk.NewStringOrIntOrBoolFromBool(true)
	gotComposed, err := baml_sdk.GoCodegenUnionEdgesRoundTripComposedAlias(ctx, composed)
	if err != nil || gotComposed.Kind() != composed.Kind() {
		t.Fatalf("composed alias = %#v, %v", gotComposed, err)
	}

	gotLarge, err := baml_sdk.GoCodegenUnionEdgesRoundTripLargeAlias(ctx, "alias")
	if err != nil || gotLarge != "alias" {
		t.Fatalf("large alias = %T(%#v), %v", gotLarge, gotLarge, err)
	}

	gotNull, err := baml_sdk.GoCodegenUnionEdgesRoundTripNullableLargeUnion(ctx, nil)
	if err != nil || gotNull != nil {
		t.Fatalf("nullable large alias = %T(%#v), %v", gotNull, gotNull, err)
	}
}

func Test_closed_union_zero_value_returns_an_input_error(t *testing.T) {
	var zero baml_sdk.StringOrInt
	_, err := baml_sdk.GoCodegenUnionEdgesRoundTripSmallAlias(context.Background(), zero)
	if err == nil || !strings.Contains(err.Error(), "zero or invalid") {
		t.Fatalf("unexpected zero-union error: %v", err)
	}
}

func Test_overlapping_list_arms_preserve_exact_kind_for_empty_and_nonempty_values(t *testing.T) {
	integer := int64(7)
	cases := []baml_sdk.IntListOrOptionalIntList{
		baml_sdk.NewIntListOrOptionalIntListFromIntList([]int64{}),
		baml_sdk.NewIntListOrOptionalIntListFromIntList([]int64{7}),
		baml_sdk.NewIntListOrOptionalIntListFromOptionalIntList([]*int64{}),
		baml_sdk.NewIntListOrOptionalIntListFromOptionalIntList([]*int64{&integer}),
	}
	for _, input := range cases {
		got, err := baml_sdk.GoCodegenUnionEdgesRoundTripOverlappingLists(context.Background(), input)
		if err != nil || got.Kind() != input.Kind() {
			t.Fatalf("list kind %q -> %q, %v", input.Kind(), got.Kind(), err)
		}
		if input.Kind() == baml_sdk.IntListOrOptionalIntListKindIntList {
			if _, ok := got.AsIntList(); !ok {
				t.Fatalf("expected int-list arm, got %q", got.Kind())
			}
		} else if _, ok := got.AsOptionalIntList(); !ok {
			t.Fatalf("expected optional-int-list arm, got %q", got.Kind())
		}
	}
}

func Test_overlapping_map_arms_preserve_exact_kind_for_empty_and_nonempty_values(t *testing.T) {
	integer := int64(7)
	cases := []baml_sdk.StringToIntMapOrStringToOptionalIntMap{
		baml_sdk.NewStringToIntMapOrStringToOptionalIntMapFromStringToIntMap(map[string]int64{}),
		baml_sdk.NewStringToIntMapOrStringToOptionalIntMapFromStringToIntMap(map[string]int64{"x": 7}),
		baml_sdk.NewStringToIntMapOrStringToOptionalIntMapFromStringToOptionalIntMap(map[string]*int64{}),
		baml_sdk.NewStringToIntMapOrStringToOptionalIntMapFromStringToOptionalIntMap(map[string]*int64{"x": &integer}),
	}
	for _, input := range cases {
		got, err := baml_sdk.GoCodegenUnionEdgesRoundTripOverlappingMaps(context.Background(), input)
		if err != nil || got.Kind() != input.Kind() {
			t.Fatalf("map kind %q -> %q, %v", input.Kind(), got.Kind(), err)
		}
		if input.Kind() == baml_sdk.StringToIntMapOrStringToOptionalIntMapKindStringToIntMap {
			if _, ok := got.AsStringToIntMap(); !ok {
				t.Fatalf("expected int-map arm, got %q", got.Kind())
			}
		} else if _, ok := got.AsStringToOptionalIntMap(); !ok {
			t.Fatalf("expected optional-int-map arm, got %q", got.Kind())
		}
	}
}

func Test_overlapping_literal_arms_round_trip_with_exact_kind(t *testing.T) {
	broad := baml_sdk.NewStringOrStringLiteral5cbcfd2eFromString("ordinary")
	literal := baml_sdk.NewStringOrStringLiteral5cbcfd2eFromStringLiteral5cbcfd2e()
	for _, input := range []baml_sdk.StringOrStringLiteral5cbcfd2e{broad, literal} {
		got, err := baml_sdk.GoCodegenUnionEdgesRoundTripStringAndLiteral(context.Background(), input)
		if err != nil || got.Kind() != input.Kind() {
			t.Fatalf("string/literal kind %q -> %q, %v", input.Kind(), got.Kind(), err)
		}
	}

	draft := baml_sdk.NewStringLiteral5cbcfd2eOrStringLiteralc0776e37FromStringLiteral5cbcfd2e()
	published := baml_sdk.NewStringLiteral5cbcfd2eOrStringLiteralc0776e37FromStringLiteralc0776e37()
	for _, input := range []baml_sdk.StringLiteral5cbcfd2eOrStringLiteralc0776e37{draft, published} {
		got, err := baml_sdk.GoCodegenUnionEdgesRoundTripStringLiterals(context.Background(), input)
		if err != nil || got.Kind() != input.Kind() {
			t.Fatalf("literal kind %q -> %q, %v", input.Kind(), got.Kind(), err)
		}
		gotReordered, err := baml_sdk.GoCodegenUnionEdgesRoundTripReorderedLiterals(context.Background(), input)
		if err != nil || gotReordered.Kind() != input.Kind() {
			t.Fatalf("reordered literal kind %q -> %q, %v", input.Kind(), gotReordered.Kind(), err)
		}
	}
}

func Test_primitive_literal_union_constructors_round_trip_with_exact_kind_and_value(t *testing.T) {
	ctx := context.Background()

	minusOne := baml_sdk.NewIntLiteral49b6f42bOrIntLiteral49d0b523FromIntLiteral49b6f42b()
	gotMinusOne, err := baml_sdk.GoCodegenUnionEdgesRoundTripIntLiterals(ctx, minusOne)
	if value, ok := gotMinusOne.AsIntLiteral49b6f42b(); err != nil || gotMinusOne.Kind() != minusOne.Kind() || !ok || value != -1 {
		t.Fatalf("int -1 literal = %d, %t, %q -> %q, %v", value, ok, minusOne.Kind(), gotMinusOne.Kind(), err)
	}
	fortyTwo := baml_sdk.NewIntLiteral49b6f42bOrIntLiteral49d0b523FromIntLiteral49d0b523()
	gotFortyTwo, err := baml_sdk.GoCodegenUnionEdgesRoundTripIntLiterals(ctx, fortyTwo)
	if value, ok := gotFortyTwo.AsIntLiteral49d0b523(); err != nil || gotFortyTwo.Kind() != fortyTwo.Kind() || !ok || value != 42 {
		t.Fatalf("int 42 literal = %d, %t, %q -> %q, %v", value, ok, fortyTwo.Kind(), gotFortyTwo.Kind(), err)
	}

	huge := baml_sdk.NewBigintLiteral0b6bb443OrBigintLiteralc1360becFromBigintLiteral0b6bb443()
	gotHuge, err := baml_sdk.GoCodegenUnionEdgesRoundTripBigintLiterals(ctx, huge)
	if value, ok := gotHuge.AsBigintLiteral0b6bb443(); err != nil || gotHuge.Kind() != huge.Kind() || !ok || value.String() != "123456789012345678901234567890" {
		t.Fatalf("huge bigint literal = %v, %t, %q -> %q, %v", value, ok, huge.Kind(), gotHuge.Kind(), err)
	}
	bigFortyTwo := baml_sdk.NewBigintLiteral0b6bb443OrBigintLiteralc1360becFromBigintLiteralc1360bec()
	gotBigFortyTwo, err := baml_sdk.GoCodegenUnionEdgesRoundTripBigintLiterals(ctx, bigFortyTwo)
	if value, ok := gotBigFortyTwo.AsBigintLiteralc1360bec(); err != nil || gotBigFortyTwo.Kind() != bigFortyTwo.Kind() || !ok || value.String() != "42" {
		t.Fatalf("bigint 42 literal = %v, %t, %q -> %q, %v", value, ok, bigFortyTwo.Kind(), gotBigFortyTwo.Kind(), err)
	}

	falseLiteral := baml_sdk.NewBoolLiteral59aabd97OrBoolLiteral67054145FromBoolLiteral59aabd97()
	gotFalse, err := baml_sdk.GoCodegenUnionEdgesRoundTripBoolLiterals(ctx, falseLiteral)
	if value, ok := gotFalse.AsBoolLiteral59aabd97(); err != nil || gotFalse.Kind() != falseLiteral.Kind() || !ok || value {
		t.Fatalf("false literal = %t, %t, %q -> %q, %v", value, ok, falseLiteral.Kind(), gotFalse.Kind(), err)
	}
	trueLiteral := baml_sdk.NewBoolLiteral59aabd97OrBoolLiteral67054145FromBoolLiteral67054145()
	gotTrue, err := baml_sdk.GoCodegenUnionEdgesRoundTripBoolLiterals(ctx, trueLiteral)
	if value, ok := gotTrue.AsBoolLiteral67054145(); err != nil || gotTrue.Kind() != trueLiteral.Kind() || !ok || !value {
		t.Fatalf("true literal = %t, %t, %q -> %q, %v", value, ok, trueLiteral.Kind(), gotTrue.Kind(), err)
	}
}
