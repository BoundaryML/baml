// Roundtrip coverage for `baml_sdk.tuples` — tuples at the host boundary.
//
// java-port note: Java has no native tuple, so each concrete tuple identity
// renders as a generated record in the root package, `TupleN_Type1Type2…`
// (arity, then one union-arm token per slot in slot order), with components
// `item0`, `item1`, … (zero-based, like BAML's `t[0]`). The record is
// registered in the typemap under its tuple identity: decode constructs it by
// identity, encode states the identity its class was registered for. Every
// tuple carries its own type on the wire, so a value that reaches Java without
// a declared type (a dynamic call's result, a thrown value, an `unknown` or
// unbound `T` position) still finds its record by that carried identity. An
// identity with no record is a `BamlTupleList`: the list of its slots (as
// arms of their join where Java has that union), which keeps the carried type
// and restates as that tuple when sent back.
package roundtrip_tests;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_bridge.BamlError;
import baml_bridge.BamlTupleList;
import baml_bridge.BamlType;
import baml_bridge.BamlTypes;
import baml_bridge.Union2;
import baml_sdk.Target;
import baml_sdk.Tuple13_IntStringIntStringIntStringIntStringIntStringIntStringInt;
import baml_sdk.Tuple1_String;
import baml_sdk.Tuple2_IntBoolean;
import baml_sdk.Tuple2_IntOptionalString;
import baml_sdk.Tuple2_IntOrStringBoolean;
import baml_sdk.Tuple2_IntString;
import baml_sdk.Tuple2_StringInt;
import baml_sdk.Tuple2_Tuple2_IntStringBoolean;
import baml_sdk.Tuple3_IntIntInt;
import baml_sdk.tuples.Fns;
import baml_sdk.tuples.TupleBox;
import baml_sdk.tuples.TupleHolder;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import org.junit.jupiter.api.Test;

class TestTuples {

    private static final Tuple2_IntString PAIR = new Tuple2_IntString(1L, "a");

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    @Test
    void test_tuples_round_trip_pair() {
        assertEquals(PAIR, Fns.round_trip_pair(PAIR));
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    @Test
    void test_tuples_round_trip_pair_async() {
        assertEquals(PAIR, Fns.round_trip_pair_async(PAIR).join());
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    @Test
    void test_tuples_round_trip_single_and_repeated() {
        assertEquals(new Tuple1_String("x"), Fns.round_trip_single(new Tuple1_String("x")));
        assertEquals(new Tuple3_IntIntInt(1L, 2L, 3L), Fns.round_trip_repeated(new Tuple3_IntIntInt(1L, 2L, 3L)));
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    @Test
    void test_tuples_make_swap_and_index() {
        // Slot order is identity: the swap returns the other record.
        assertEquals(new Tuple2_StringInt("a", 1L), Fns.swap_pair(PAIR));
        assertEquals(1L, Fns.pair_first(PAIR));
        assertEquals(new Tuple2_IntString(7L, "seven"), Fns.make_pair(7L, "seven"));
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    @Test
    void test_tuples_round_trip_nested() {
        Tuple2_Tuple2_IntStringBoolean nested = new Tuple2_Tuple2_IntStringBoolean(PAIR, true);
        assertEquals(nested, Fns.round_trip_nested(nested));
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    @Test
    void test_tuples_round_trip_optional_slot_and_optional_tuple() {
        assertEquals(new Tuple2_IntOptionalString(1L, null), Fns.round_trip_optional_slot(new Tuple2_IntOptionalString(1L, null)));
        assertEquals(new Tuple2_IntOptionalString(1L, "s"), Fns.round_trip_optional_slot(new Tuple2_IntOptionalString(1L, "s")));
        assertEquals(PAIR, Fns.round_trip_optional_tuple(PAIR));
        assertNull(Fns.round_trip_optional_tuple(null));
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    @Test
    void test_tuples_round_trip_containers() {
        List<Tuple2_StringInt> pairs = List.of(new Tuple2_StringInt("a", 1L), new Tuple2_StringInt("b", 2L));
        assertEquals(pairs, Fns.round_trip_tuple_list(pairs));
        assertEquals(List.of(), Fns.round_trip_tuple_list(List.of()));
        Map<String, Tuple2_IntBoolean> map = Map.of("k", new Tuple2_IntBoolean(1L, false));
        assertEquals(map, Fns.round_trip_tuple_map(map));
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    @Test
    void test_tuples_round_trip_union_arms() {
        assertEquals(
                new Union2.Arm0<Tuple2_IntString, String>(PAIR),
                Fns.round_trip_tuple_or_string(new Union2.Arm0<Tuple2_IntString, String>(PAIR)));
        assertEquals(
                new Union2.Arm1<Tuple2_IntString, String>("s"),
                Fns.round_trip_tuple_or_string(new Union2.Arm1<Tuple2_IntString, String>("s")));
        // Two same-arity tuple arms: the slot types pick the arm.
        assertEquals(
                new Union2.Arm0<Tuple2_IntString, Tuple2_StringInt>(PAIR),
                Fns.round_trip_tuple_arms(new Union2.Arm0<Tuple2_IntString, Tuple2_StringInt>(PAIR)));
        Tuple2_StringInt swapped = new Tuple2_StringInt("a", 1L);
        assertEquals(
                new Union2.Arm1<Tuple2_IntString, Tuple2_StringInt>(swapped),
                Fns.round_trip_tuple_arms(new Union2.Arm1<Tuple2_IntString, Tuple2_StringInt>(swapped)));
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    @Test
    void test_tuples_round_trip_class_fields() {
        TupleHolder holder = new TupleHolder(
                PAIR,
                new Tuple2_Tuple2_IntStringBoolean(new Tuple2_IntString(2L, "b"), false),
                new Tuple2_IntOptionalString(3L, null),
                List.of(new Tuple2_StringInt("c", 4L)));
        assertEquals(holder, Fns.round_trip_holder(holder));
        TupleHolder empty = new TupleHolder(PAIR, holder.nested(), null, List.of());
        assertEquals(empty, Fns.round_trip_holder(empty));
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    @Test
    void test_tuples_round_trip_generic_argument() {
        // A type argument that is a tuple: the instance's type args bind `T`.
        TupleBox<Tuple2_IntString> box = TupleBox.of(BamlType.of(Tuple2_IntString.class), PAIR);
        TupleBox<Tuple2_IntString> back = Fns.round_trip_tuple_box(box);
        assertEquals(PAIR, back.value());
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    @Test
    void test_tuples_round_trip_wide() {
        var wide = new Tuple13_IntStringIntStringIntStringIntStringIntStringIntStringInt(
                1L, "a", 2L, "b", 3L, "c", 4L, "d", 5L, "e", 6L, "f", 7L);
        assertEquals(wide, Fns.round_trip_wide(wide));
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    @Test
    void test_tuples_host_callback_args_and_results() {
        List<Tuple2_IntString> received = new ArrayList<>();
        Tuple2_StringInt result = Fns.call_tuple_callback(
                t -> {
                    received.add(t);
                    return new Tuple2_StringInt(t.item1(), t.item0());
                },
                PAIR);
        assertEquals(new Tuple2_StringInt("a", 1L), result);
        assertEquals(List.of(PAIR), received);
        assertEquals(
                new Tuple2_StringInt("a", 1L),
                Fns.call_tuple_callback_async(t -> new Tuple2_StringInt(t.item1(), t.item0()), PAIR).join());
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    @Test
    void test_tuples_thrown_value() {
        // A thrown value decodes with no declared type: its carried identity
        // still finds the record.
        BamlError error = assertThrows(BamlError.class, () -> Fns.throw_pair(PAIR));
        assertEquals(PAIR, error.value());
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    @Test
    void test_tuples_dynamic_value_decodes_as_a_tuple() {
        // A dynamic call states no return type: the tuple's carried type finds
        // its record.
        Object result = baml_sdk.Baml.invoke(
                Target.named("user.tuples.round_trip_pair"), Map.of("t", PAIR), null, baml_sdk.BamlOptions.empty());
        assertEquals(PAIR, result);
        // A plain list has no tuple identity of its own, and no slot supplies
        // one here: the engine refuses it as the tuple argument.
        assertThrows(
                Throwable.class,
                () -> baml_sdk.Baml.invoke(
                        Target.named("user.tuples.round_trip_pair"),
                        Map.of("t", List.of(1L, "a")),
                        null,
                        baml_sdk.BamlOptions.empty()));
    }

    // SDK_PARITY_LINT(skip): only Python and Java accept a plain list at a tuple slot
    @Test
    void test_tuples_plain_list_stands_in_at_a_tuple_slot() {
        // A plain list stands in for a tuple where the slot supplies the shape:
        // exactly its arity, each item converted against its own slot.
        Object ok = baml_sdk.Baml.invoke(
                Target.named("user.tuples.round_trip_pair"),
                Map.of("t", new baml_bridge.BamlTypedValue(List.of(5L, "five"), BamlType.of(Tuple2_IntString.class))),
                null,
                baml_sdk.BamlOptions.empty());
        assertEquals(new Tuple2_IntString(5L, "five"), ok);
        assertThrows(
                IllegalArgumentException.class,
                () -> baml_sdk.Baml.invoke(
                        Target.named("user.tuples.round_trip_pair"),
                        Map.of("t", new baml_bridge.BamlTypedValue(List.of(5L), BamlType.of(Tuple2_IntString.class))),
                        null,
                        baml_sdk.BamlOptions.empty()));
        // A wrong slot type is refused by the engine's own check.
        assertThrows(
                Throwable.class,
                () -> baml_sdk.Baml.invoke(
                        Target.named("user.tuples.round_trip_pair"),
                        Map.of("t", new baml_bridge.BamlTypedValue(List.of("five", 5L), BamlType.of(Tuple2_IntString.class))),
                        null,
                        baml_sdk.BamlOptions.empty()));
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    @Test
    void test_tuples_reflected_type_token() {
        // The first call loads the SDK, which registers the generated records.
        assertInstanceOf(Tuple2_IntString.class, Fns.round_trip_pair(PAIR));
        assertEquals(BamlType.tuple(BamlType.INT, BamlType.STRING), BamlType.of(Tuple2_IntString.class));
    }

    private static final Tuple2_IntOrStringBoolean PRODUCT =
            new Tuple2_IntOrStringBoolean(new Union2.Arm0<Long, String>(1L), true);

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    @Test
    void test_tuples_dynamic_value_keeps_carried_type() {
        // Returned as `unknown`, `(1, true)` is still the `(int | string, bool)`
        // it was built as, not the `(int, bool)` its elements suggest.
        Object back = Fns.tuple_as_unknown(PRODUCT);
        assertEquals(PRODUCT, back);
        assertEquals(PRODUCT, Fns.tuple_as_unknown_async(PRODUCT).join());
        // Sent back, it restates that identity.
        assertTrue(Fns.unknown_has_carried_tuple_type(back));
        // A callback receiving it at an open slot and returning it unchanged.
        List<Object> received = new ArrayList<>();
        assertTrue(Fns.call_unknown_tuple_callback(
                v -> {
                    received.add(v);
                    return v;
                },
                PRODUCT));
        assertEquals(List.of(PRODUCT), received);
    }

    // SDK_PARITY_LINT(skip): only Go and Java need a fallback for a tuple with no generated type
    @Test
    void test_tuples_type_parameter_slots_use_the_carried_type() {
        // `(T, int)[]` with `T` unbound: each tuple's carried type finds the
        // record where one is registered.
        assertEquals(
                List.of(new Tuple2_StringInt("x", 1L), new Tuple2_StringInt("x", 2L)),
                baml_sdk.go_codegen.tuple_edges.Fns.tag_values("x"));
        // No record for `(float, int)`: the list of its slot join, keeping the
        // carried type.
        List<Object> floats = baml_sdk.go_codegen.tuple_edges.Fns.tag_values(1.5);
        BamlType floatInt = BamlType.tuple(BamlType.FLOAT, BamlType.INT);
        BamlTupleList first = assertInstanceOf(BamlTupleList.class, floats.get(0));
        assertEquals(floatInt, first.tupleType());
        assertEquals(List.of(new Union2.Arm0<Double, Long>(1.5), new Union2.Arm1<Double, Long>(1L)), first);
        // Sent back, it is that tuple; a plain list of the same items is a list.
        assertTrue(Fns.value_has_type(first, BamlTypes.of("T", floatInt)));
        assertFalse(Fns.value_has_type(List.of(1.5, 1L), BamlTypes.of("T", floatInt)));
    }

    // SDK_PARITY_LINT(skip): only Go and Java need a fallback for a tuple with no generated type
    @Test
    void test_tuples_thrown_unnamed_tuple() {
        BamlError error = assertThrows(BamlError.class, baml_sdk.go_codegen.tuple_edges.Fns::throw_unnamed_pair);
        assertEquals(new baml_sdk.Tuple2_BooleanBoolean(true, false), error.value());
    }
}
