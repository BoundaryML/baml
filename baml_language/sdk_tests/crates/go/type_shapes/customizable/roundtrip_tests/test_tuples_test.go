package sdk_test

import (
	"context"
	"errors"
	"reflect"
	"strings"
	"testing"

	"baml.local/sdk/baml_sdk"
	baml_go "github.com/boundaryml/baml-go"
)

// Go has no tuple type. A tuple is a generated `TupleN_<slots>` struct with one
// `ItemN` field per slot, except that a generated function returning a tuple
// returns one Go result per slot beside its error.

// SDK_PARITY_LINT(skip): Go-only: a tuple return expands to one Go result per slot
func Test_tuples_return_expands_to_one_result_per_slot(t *testing.T) {
	ctx := context.Background()
	n, s, err := baml_sdk.TuplesRoundTripPair(ctx, baml_sdk.Tuple2_IntString{Item0: 1, Item1: "one"})
	if err != nil || n != 1 || s != "one" {
		t.Fatalf("pair = %v, %q, %v", n, s, err)
	}
	single, err := baml_sdk.TuplesRoundTripSingle(ctx, baml_sdk.Tuple1_String{Item0: "only"})
	if err != nil || single != "only" {
		t.Fatalf("single = %q, %v", single, err)
	}
	a, b, c, err := baml_sdk.TuplesRoundTripRepeated(ctx, baml_sdk.Tuple3_IntIntInt{Item0: 1, Item1: 2, Item2: 3})
	if err != nil || a != 1 || b != 2 || c != 3 {
		t.Fatalf("repeated = %v, %v, %v, %v", a, b, c, err)
	}
	made, label, err := baml_sdk.TuplesMakePair(ctx, 5, "five")
	if err != nil || made != 5 || label != "five" {
		t.Fatalf("made = %v, %q, %v", made, label, err)
	}
	swapped, count, err := baml_sdk.TuplesSwapPair(ctx, baml_sdk.Tuple2_IntString{Item0: 2, Item1: "two"})
	if err != nil || swapped != "two" || count != 2 {
		t.Fatalf("swapped = %q, %v, %v", swapped, count, err)
	}
	first, err := baml_sdk.TuplesPairFirst(ctx, baml_sdk.Tuple2_IntString{Item0: 9, Item1: "nine"})
	if err != nil || first != 9 {
		t.Fatalf("first = %v, %v", first, err)
	}
}

// SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
func Test_tuples_round_trip_nested(t *testing.T) {
	want := baml_sdk.Tuple2_Tuple2_IntStringBool{Item0: baml_sdk.Tuple2_IntString{Item0: 1, Item1: "one"}, Item1: true}
	inner, flag, err := baml_sdk.TuplesRoundTripNested(context.Background(), want)
	if err != nil || inner != want.Item0 || !flag {
		t.Fatalf("nested = %#v, %v, %v", inner, flag, err)
	}
}

// SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
func Test_tuples_round_trip_optional_slot_and_optional_tuple(t *testing.T) {
	ctx := context.Background()
	label := "x"
	n, got, err := baml_sdk.TuplesRoundTripOptionalSlot(ctx, baml_sdk.Tuple2_IntOptionalString{Item0: 1, Item1: &label})
	if err != nil || n != 1 || got == nil || *got != "x" {
		t.Fatalf("optional slot = %v, %v, %v", n, got, err)
	}
	n, got, err = baml_sdk.TuplesRoundTripOptionalSlot(ctx, baml_sdk.Tuple2_IntOptionalString{Item0: 2})
	if err != nil || n != 2 || got != nil {
		t.Fatalf("null slot = %v, %v, %v", n, got, err)
	}
	// A tuple inside an optional is one value, not expanded.
	pair := baml_sdk.Tuple2_IntString{Item0: 3, Item1: "three"}
	present, err := baml_sdk.TuplesRoundTripOptionalTuple(ctx, &pair)
	if err != nil || present == nil || *present != pair {
		t.Fatalf("optional tuple = %#v, %v", present, err)
	}
	absent, err := baml_sdk.TuplesRoundTripOptionalTuple(ctx, nil)
	if err != nil || absent != nil {
		t.Fatalf("null tuple = %#v, %v", absent, err)
	}
}

// SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
func Test_tuples_round_trip_containers(t *testing.T) {
	ctx := context.Background()
	list := []baml_sdk.Tuple2_StringInt{{Item0: "a", Item1: 1}, {Item0: "b", Item1: 2}}
	gotList, err := baml_sdk.TuplesRoundTripTupleList(ctx, list)
	if err != nil || !reflect.DeepEqual(gotList, list) {
		t.Fatalf("list = %#v, %v", gotList, err)
	}
	lookup := map[string]baml_sdk.Tuple2_IntBool{"x": {Item0: 1, Item1: true}}
	gotMap, err := baml_sdk.TuplesRoundTripTupleMap(ctx, lookup)
	if err != nil || !reflect.DeepEqual(gotMap, lookup) {
		t.Fatalf("map = %#v, %v", gotMap, err)
	}
}

// SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
func Test_tuples_round_trip_class_fields(t *testing.T) {
	list := []baml_sdk.Tuple2_StringInt{{Item0: "a", Item1: 1}, {Item0: "b", Item1: 2}}
	label := "maybe"
	holder := baml_sdk.TuplesTupleHolder{
		Pair:   baml_sdk.Tuple2_IntString{Item0: 1, Item1: "one"},
		Nested: baml_sdk.Tuple2_Tuple2_IntStringBool{Item0: baml_sdk.Tuple2_IntString{Item0: 2, Item1: "two"}, Item1: false},
		Maybe:  &baml_sdk.Tuple2_IntOptionalString{Item0: 3, Item1: &label},
		Pairs:  list,
	}
	gotHolder, err := baml_sdk.TuplesRoundTripHolder(context.Background(), holder)
	if err != nil || !reflect.DeepEqual(gotHolder, holder) {
		t.Fatalf("holder = %#v, %v", gotHolder, err)
	}
}

// SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
func Test_tuples_round_trip_generic_argument(t *testing.T) {
	box := baml_sdk.TuplesTupleBox[baml_sdk.Tuple2_IntString]{Value: baml_sdk.Tuple2_IntString{Item0: 4, Item1: "four"}}
	gotBox, err := baml_sdk.TuplesRoundTripTupleBox(context.Background(), box)
	if err != nil || gotBox != box {
		t.Fatalf("generic box = %#v, %v", gotBox, err)
	}
}

// SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
func Test_tuples_round_trip_union_arms(t *testing.T) {
	ctx := context.Background()
	pair := baml_sdk.Tuple2_IntString{Item0: 1, Item1: "one"}
	got, err := baml_sdk.TuplesRoundTripTupleOrString(ctx, baml_sdk.NewStringOrTuple2_IntStringFromTuple2_IntString(pair))
	if gotPair, ok := got.AsTuple2_IntString(); err != nil || !ok || gotPair != pair {
		t.Fatalf("tuple arm = %#v, %v", got, err)
	}
	got, err = baml_sdk.TuplesRoundTripTupleOrString(ctx, baml_sdk.NewStringOrTuple2_IntStringFromString("text"))
	if text, ok := got.AsString(); err != nil || !ok || text != "text" {
		t.Fatalf("string arm = %#v, %v", got, err)
	}
	swapped := baml_sdk.Tuple2_StringInt{Item0: "one", Item1: 1}
	arms, err := baml_sdk.TuplesRoundTripTupleArms(ctx, baml_sdk.NewTuple2_StringIntOrTuple2_IntStringFromTuple2_StringInt(swapped))
	if gotSwapped, ok := arms.AsTuple2_StringInt(); err != nil || !ok || gotSwapped != swapped {
		t.Fatalf("(string, int) arm = %#v, %v", arms, err)
	}
	arms, err = baml_sdk.TuplesRoundTripTupleArms(ctx, baml_sdk.NewTuple2_StringIntOrTuple2_IntStringFromTuple2_IntString(pair))
	if gotPair, ok := arms.AsTuple2_IntString(); err != nil || !ok || gotPair != pair {
		t.Fatalf("(int, string) arm = %#v, %v", arms, err)
	}
}

// SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
func Test_tuples_dynamic_value_decodes_as_a_tuple(t *testing.T) {
	ctx := context.Background()
	pair := baml_sdk.Tuple2_IntString{Item0: 1, Item1: "one"}
	got, err := baml_sdk.GoCodegenTupleEdgesRoundTripDynamicTuple(ctx, pair)
	if err != nil || got != pair {
		t.Fatalf("dynamic tuple = %T(%#v), %v", got, got, err)
	}
	got, err = baml_sdk.GoCodegenTupleEdgesRoundTripDynamicTuple(ctx, "text")
	if err != nil || got != "text" {
		t.Fatalf("dynamic string = %T(%#v), %v", got, got, err)
	}
}

// SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
func Test_tuples_host_callback_args_and_results(t *testing.T) {
	var received baml_sdk.Tuple2_IntString
	callback := func(_ context.Context, value baml_sdk.Tuple2_IntString) baml_sdk.Tuple2_StringInt {
		received = value
		return baml_sdk.Tuple2_StringInt{Item0: value.Item1, Item1: value.Item0 * 10}
	}
	label, count, err := baml_sdk.TuplesCallTupleCallback(context.Background(), callback, baml_sdk.Tuple2_IntString{Item0: 4, Item1: "four"})
	if err != nil || label != "four" || count != 40 {
		t.Fatalf("callback result = %q, %v, %v", label, count, err)
	}
	if received != (baml_sdk.Tuple2_IntString{Item0: 4, Item1: "four"}) {
		t.Fatalf("callback received %#v", received)
	}
}

// SDK_PARITY_LINT(skip): Go-only: a failing expanded tuple return yields zero values beside the error
func Test_tuples_failing_return_gives_zero_values_and_the_error(t *testing.T) {
	ctx := context.Background()
	n, s, err := baml_sdk.GoCodegenTupleEdgesPairOrFail(ctx, false)
	if err != nil || n != 7 || s != "seven" {
		t.Fatalf("success = %v, %q, %v", n, s, err)
	}
	n, s, err = baml_sdk.GoCodegenTupleEdgesPairOrFail(ctx, true)
	if err == nil || n != 0 || s != "" {
		t.Fatalf("failure = %v, %q, %v", n, s, err)
	}
	if !strings.Contains(err.Error(), "pair_or_fail") {
		t.Fatalf("failure error = %v", err)
	}
	// A thrown tuple fails the call; the error carries the BAML failure.
	if _, err := baml_sdk.TuplesThrowPair(ctx, baml_sdk.Tuple2_IntString{Item0: 1, Item1: "one"}); err == nil {
		t.Fatal("throw_pair unexpectedly returned")
	}
}

// SDK_PARITY_LINT(skip): Go-only: a generic tuple expands at the top level and falls back to []any below it
func Test_tuples_over_type_parameters_fall_back_below_the_top_level(t *testing.T) {
	ctx := context.Background()
	value, tag, err := baml_sdk.GoCodegenTupleEdgesTagValue(ctx, "x")
	if err != nil || value != "x" || tag != 1 {
		t.Fatalf("expanded generic = %q, %v, %v", value, tag, err)
	}
	tagged, err := baml_sdk.GoCodegenTupleEdgesTagValues(ctx, "x")
	want := [][]any{{"x", int64(1)}, {"x", int64(2)}}
	if err != nil || !reflect.DeepEqual(tagged, want) {
		t.Fatalf("[]any fallback = %#v, %v", tagged, err)
	}
}

// SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
func Test_tuples_round_trip_wide(t *testing.T) {
	want := baml_sdk.Tuple13_IntStringIntStringIntStringIntStringIntStringIntStringInt{
		Item0: 0, Item1: "1", Item2: 2, Item3: "3", Item4: 4, Item5: "5", Item6: 6,
		Item7: "7", Item8: 8, Item9: "9", Item10: 10, Item11: "11", Item12: 12,
	}
	i0, s1, i2, s3, i4, s5, i6, s7, i8, s9, i10, s11, i12, err := baml_sdk.TuplesRoundTripWide(context.Background(), want)
	got := baml_sdk.Tuple13_IntStringIntStringIntStringIntStringIntStringIntStringInt{
		Item0: i0, Item1: s1, Item2: i2, Item3: s3, Item4: i4, Item5: s5, Item6: i6,
		Item7: s7, Item8: i8, Item9: s9, Item10: i10, Item11: s11, Item12: i12,
	}
	if err != nil || got != want {
		t.Fatalf("wide = %#v, %v", got, err)
	}
}

// SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
func Test_tuples_reflected_type_token(t *testing.T) {
	want := baml_go.TupleBAMLType(baml_go.PrimitiveBAMLType(baml_go.IntType), baml_go.PrimitiveBAMLType(baml_go.StringType))
	if got := baml_go.TypeOf[baml_sdk.Tuple2_IntString](); !got.Equal(want) {
		t.Fatalf("TypeOf[Tuple2_IntString] = %v", got)
	}
	if baml_go.TypeOf[baml_sdk.Tuple2_StringInt]().Equal(want) {
		t.Fatal("(string, int) reified as (int, string)")
	}
}

// A thrown tuple, which no declaration names, finds its named struct by the
// tuple type it carries.
// SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
func Test_tuples_thrown_value(t *testing.T) {
	pair := baml_sdk.Tuple2_IntString{Item0: 1, Item1: "one"}
	_, err := baml_sdk.TuplesThrowPair(context.Background(), pair)
	var failure *baml_go.BamlError
	if !errors.As(err, &failure) || failure.Value != pair {
		t.Fatalf("thrown pair = %#v (%v)", failure, err)
	}
}

// A thrown tuple no struct registers is the list of its slot types' join.
// SDK_PARITY_LINT(skip): only Go and Java need a fallback for a tuple with no generated type
func Test_tuples_thrown_unnamed_tuple(t *testing.T) {
	_, err := baml_sdk.GoCodegenTupleEdgesThrowUnnamedPair(context.Background())
	var failure *baml_go.BamlError
	if !errors.As(err, &failure) || !reflect.DeepEqual(failure.Value, []bool{true, false}) {
		t.Fatalf("thrown unnamed pair = %#v (%v)", failure, err)
	}
}

// SDK_PARITY_LINT(skip): only Go and Java need a fallback for a tuple with no generated type
func Test_tuples_type_parameter_slots_use_the_carried_type(t *testing.T) {
	pair := baml_sdk.Tuple2_IntString{Item0: 1, Item1: "one"}
	tagged, err := baml_sdk.GoCodegenTupleEdgesTagValues(context.Background(), pair)
	want := [][]any{{pair, int64(1)}, {pair, int64(2)}}
	if err != nil || !reflect.DeepEqual(tagged, want) {
		t.Fatalf("tagged pairs = %#v, %v", tagged, err)
	}
}

// A callback throws on the BAML error it received: the tuple it carries is
// restated with its carried tuple type, so BAML catches it as that tuple.
// SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
func Test_tuples_callback_forwards_a_thrown_tuple_under_its_carried_type(t *testing.T) {
	ctx := context.Background()
	rethrow := func(ctx context.Context) (int64, error) {
		_, err := baml_sdk.TuplesThrowPair(ctx, baml_sdk.Tuple2_IntString{Item0: 7, Item1: "seven"})
		return 0, err
	}
	caught, err := baml_sdk.GoCodegenTupleEdgesCatchCallbackPair(ctx, rethrow)
	if err != nil || caught != 7 {
		t.Fatalf("caught = %v, %v", caught, err)
	}
	_, err = baml_sdk.GoCodegenTupleEdgesPropagateCallbackPair(ctx, rethrow)
	var failure *baml_go.BamlError
	if !errors.As(err, &failure) || failure.Value != (baml_sdk.Tuple2_IntString{Item0: 7, Item1: "seven"}) {
		t.Fatalf("propagated = %#v (%v)", failure, err)
	}
}

// SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
func Test_tuples_method_round_trip(t *testing.T) {
	ctx := context.Background()
	box := baml_sdk.TuplesTuplePairBox{A: 1, B: "one"}
	// A method returning a tuple expands it, as a generated function does.
	n, s, err := box.AsPair(ctx)
	if err != nil || n != 1 || s != "one" {
		t.Fatalf("AsPair() = %v, %q, %v", n, s, err)
	}
	// A method taking a tuple, and the static method Go exposes as a
	// package helper.
	with, err := box.WithPair(ctx, baml_sdk.Tuple2_IntString{Item0: 2, Item1: "two"})
	if err != nil || with != (baml_sdk.TuplesTuplePairBox{A: 2, B: "two"}) {
		t.Fatalf("WithPair() = %#v, %v", with, err)
	}
	from, err := baml_sdk.TuplesTuplePairBoxFromPair(ctx, baml_sdk.Tuple2_IntString{Item0: 3, Item1: "three"})
	if err != nil || from != (baml_sdk.TuplesTuplePairBox{A: 3, B: "three"}) {
		t.Fatalf("TuplePairBoxFromPair() = %#v, %v", from, err)
	}
}

// SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
func Test_tuples_function_handle_round_trip(t *testing.T) {
	ctx := context.Background()
	// A BAML closure returned to the host: its handle takes and returns
	// tuples, as one named struct each (a handle's result is not expanded).
	swap, err := baml_sdk.TuplesMakeTupleSwapper(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if got := swap.Call(ctx, baml_sdk.Tuple2_IntString{Item0: 1, Item1: "one"}); got != (baml_sdk.Tuple2_StringInt{Item0: "one", Item1: 1}) {
		t.Fatalf("swap(1, one) = %#v", got)
	}
	if got := swap.Call(ctx, baml_sdk.Tuple2_IntString{Item0: 2, Item1: "two"}); got != (baml_sdk.Tuple2_StringInt{Item0: "two", Item1: 2}) {
		t.Fatalf("swap(2, two) = %#v", got)
	}
}

// SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
func Test_tuples_wrong_arity_and_slots_rejected(t *testing.T) {
	ctx := context.Background()
	pairType := baml_go.TypeOf[baml_sdk.Tuple2_IntString]()
	call := func(input baml_go.Input) error {
		_, err := baml_sdk.Invoke(ctx, baml_sdk.NamedTarget("user.tuples.round_trip_pair"), baml_sdk.Arguments{"t": input}, nil)
		return err
	}
	// The named structs make a wrong arity or slot unrepresentable, so the
	// rejections go through inputs stating the tuple type: too few and too
	// many slots are refused host-side, swapped slots by the engine.
	if err := call(baml_go.TupleInput(pairType, baml_go.Int64(1))); err == nil {
		t.Fatal("a one-slot input was accepted as a pair")
	}
	if err := call(baml_go.TupleListEncoder(pairType)([]any{int64(1), "one", int64(2)})); err == nil {
		t.Fatal("a three-slot input was accepted as a pair")
	}
	if err := call(baml_go.TupleInput(pairType, baml_go.String("one"), baml_go.Int64(1))); err == nil {
		t.Fatal("swapped slots were accepted as a pair")
	}
	if err := call(baml_go.TupleInput(pairType, baml_go.Int64(1), baml_go.String("one"))); err != nil {
		t.Fatalf("a well-formed pair was refused: %v", err)
	}
	// A received tuple decodes only at its own arity and slot types.
	pair, err := baml_sdk.Invoke(ctx, baml_sdk.NamedTarget("user.tuples.make_pair"), baml_sdk.Arguments{"n": baml_go.Int64(1), "s": baml_go.String("one")}, nil)
	if err != nil {
		t.Fatal(err)
	}
	if got, err := baml_go.DecodeAs[baml_sdk.Tuple2_IntString](pair); err != nil || got != (baml_sdk.Tuple2_IntString{Item0: 1, Item1: "one"}) {
		t.Fatalf("pair decoded as %#v, %v", got, err)
	}
	if _, err := baml_go.DecodeAs[baml_sdk.Tuple2_StringInt](pair); err == nil {
		t.Fatal("(int, string) decoded as (string, int)")
	}
	if _, err := baml_go.DecodeAs[baml_sdk.Tuple3_IntIntInt](pair); err == nil {
		t.Fatal("a pair decoded as a triple")
	}
	if _, err := baml_go.DecodeAs[baml_sdk.Tuple1_String](pair); err == nil {
		t.Fatal("a pair decoded as a single")
	}
}
