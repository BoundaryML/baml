// Roundtrip coverage for `baml_sdk.tuples` — tuples at the host boundary.
//
// java-port note: Java has no native tuple, so each concrete tuple identity
// renders as a generated record in the root package, `TupleN_Type1Type2…`
// (arity, then one union-arm token per slot in slot order), with components
// `item0`, `item1`, … (zero-based, like BAML's `t[0]`). The record is
// registered in the typemap under its tuple identity: decode constructs it by
// identity, encode states the identity its class was registered for. A value
// that reaches Java without a declared type (a dynamic call's result, a thrown
// value) has no identity on the wire yet, so it takes the last fallback, the
// plain `List<Object>` of its slots.
package roundtrip_tests;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;

import baml_bridge.BamlError;
import baml_bridge.BamlType;
import baml_bridge.Union2;
import baml_sdk.Target;
import baml_sdk.Tuple13_IntStringIntStringIntStringIntStringIntStringIntStringInt;
import baml_sdk.Tuple1_String;
import baml_sdk.Tuple2_IntBoolean;
import baml_sdk.Tuple2_IntOptionalString;
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

    @Test
    void test_tuples_round_trip_pair() {
        assertEquals(PAIR, Fns.round_trip_pair(PAIR));
        assertEquals(PAIR, Fns.round_trip_pair_async(PAIR).join());
    }

    @Test
    void test_tuples_slot_order_and_repeats_are_identity() {
        assertEquals(new Tuple1_String("x"), Fns.round_trip_single(new Tuple1_String("x")));
        assertEquals(new Tuple3_IntIntInt(1L, 2L, 3L), Fns.round_trip_repeated(new Tuple3_IntIntInt(1L, 2L, 3L)));
        assertEquals(new Tuple2_StringInt("a", 1L), Fns.swap_pair(PAIR));
        assertEquals(1L, Fns.pair_first(PAIR));
        assertEquals(new Tuple2_IntString(7L, "seven"), Fns.make_pair(7L, "seven"));
    }

    @Test
    void test_tuples_round_trip_nested_and_optional() {
        Tuple2_Tuple2_IntStringBoolean nested = new Tuple2_Tuple2_IntStringBoolean(PAIR, true);
        assertEquals(nested, Fns.round_trip_nested(nested));
        assertEquals(new Tuple2_IntOptionalString(1L, null), Fns.round_trip_optional_slot(new Tuple2_IntOptionalString(1L, null)));
        assertEquals(new Tuple2_IntOptionalString(1L, "s"), Fns.round_trip_optional_slot(new Tuple2_IntOptionalString(1L, "s")));
        assertEquals(PAIR, Fns.round_trip_optional_tuple(PAIR));
        assertNull(Fns.round_trip_optional_tuple(null));
    }

    @Test
    void test_tuples_round_trip_containers() {
        List<Tuple2_StringInt> pairs = List.of(new Tuple2_StringInt("a", 1L), new Tuple2_StringInt("b", 2L));
        assertEquals(pairs, Fns.round_trip_tuple_list(pairs));
        assertEquals(List.of(), Fns.round_trip_tuple_list(List.of()));
        Map<String, Tuple2_IntBoolean> map = Map.of("k", new Tuple2_IntBoolean(1L, false));
        assertEquals(map, Fns.round_trip_tuple_map(map));
    }

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

    @Test
    void test_tuples_round_trip_class_fields_and_generics() {
        TupleHolder holder = new TupleHolder(
                PAIR,
                new Tuple2_Tuple2_IntStringBoolean(new Tuple2_IntString(2L, "b"), false),
                new Tuple2_IntOptionalString(3L, null),
                List.of(new Tuple2_StringInt("c", 4L)));
        assertEquals(holder, Fns.round_trip_holder(holder));
        TupleHolder empty = new TupleHolder(PAIR, holder.nested(), null, List.of());
        assertEquals(empty, Fns.round_trip_holder(empty));
        // A type argument that is a tuple: the instance's type args bind `T`.
        TupleBox<Tuple2_IntString> box = TupleBox.of(BamlType.of(Tuple2_IntString.class), PAIR);
        TupleBox<Tuple2_IntString> back = Fns.round_trip_tuple_box(box);
        assertEquals(PAIR, back.value());
    }

    @Test
    void test_tuples_wide_tuple_is_a_record() {
        var wide = new Tuple13_IntStringIntStringIntStringIntStringIntStringIntStringInt(
                1L, "a", 2L, "b", 3L, "c", 4L, "d", 5L, "e", 6L, "f", 7L);
        assertEquals(wide, Fns.round_trip_wide(wide));
    }

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

    @Test
    void test_tuples_thrown_value_is_its_slots() {
        // A thrown value decodes with no declared type: the plain list of its slots.
        BamlError error = assertThrows(BamlError.class, () -> Fns.throw_pair(PAIR));
        assertEquals(List.of(1L, "a"), error.value());
    }

    @Test
    void test_tuples_dynamic_call_takes_the_list_fallback() {
        // A dynamic call states no return type: the result is the plain list of
        // the tuple's slots, in order.
        Object result = baml_sdk.Baml.invoke(
                Target.named("user.tuples.round_trip_pair"), Map.of("t", PAIR), null, baml_sdk.BamlOptions.empty());
        assertEquals(List.of(1L, "a"), result);
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

    @Test
    void test_tuples_plain_list_needs_the_slot_shape() {
        // A plain list stands in for a tuple where the slot supplies the shape:
        // exactly its arity, each item converted against its own slot.
        Object ok = baml_sdk.Baml.invoke(
                Target.named("user.tuples.round_trip_pair"),
                Map.of("t", new baml_bridge.BamlTypedValue(List.of(5L, "five"), BamlType.of(Tuple2_IntString.class))),
                null,
                baml_sdk.BamlOptions.empty());
        assertEquals(List.of(5L, "five"), ok);
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

    @Test
    void test_tuples_record_reflects_its_tuple_type() {
        assertEquals(BamlType.tuple(BamlType.INT, BamlType.STRING), BamlType.of(Tuple2_IntString.class));
        assertInstanceOf(Tuple2_IntString.class, Fns.round_trip_pair(PAIR));
    }
}
