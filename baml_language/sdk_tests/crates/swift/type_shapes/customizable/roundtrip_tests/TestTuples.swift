// Roundtrip coverage for `Baml.tuples`.
//
// A BAML tuple of two or more slots is a native Swift tuple at every arity:
// `(int, string)` is `(Int, String)`. Swift has no one-element tuple, so
// `(string,)` is the runtime's `Tuple1<String>`. A native tuple conforms to
// no protocol: it crosses through its position's slot, which the generated
// code builds, and a generated struct holding one spells its `==` out.
// Values cross by copy.
import XCTest
import Baml
import BamlBridge

/// Slot-wise equality of two native tuples of `Equatable` slots, at any
/// arity (the standard library's `==` stops at six slots).
private func XCTAssertTupleEqual<each Slot: Equatable>(
    _ lhs: (repeat each Slot),
    _ rhs: (repeat each Slot),
    file: StaticString = #filePath,
    line: UInt = #line
) {
    for (left, right) in repeat (each lhs, each rhs) {
        XCTAssertEqual(left, right, file: file, line: line)
    }
}

final class TestTuples: XCTestCase {
    typealias Pair = (Int, String)

    /// The slot of `(int, string)`, for comparing values holding one.
    private let pair = BamlSlots.tuple(BamlSlot<Int>.own, BamlSlot<String>.own)

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_round_trip_pair() throws {
        XCTAssertTupleEqual(try Baml.tuples.round_trip_pair(t: (1, "a")), (1, "a"))
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_round_trip_pair_async() async throws {
        let result = try await Baml.tuples.round_trip_pair_async(t: (2, "b"))
        XCTAssertTupleEqual(result, (2, "b"))
    }

    // SDK_PARITY_LINT(skip): Swift-only: a received tuple destructures as a native Swift tuple
    func test_tuples_native_tuple_bridging() throws {
        let (n, s) = try Baml.tuples.make_pair(n: 3, s: "c")
        XCTAssertEqual(n, 3)
        XCTAssertEqual(s, "c")
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_make_swap_and_index() throws {
        // `(int, string)` -> `(string, int)`: a distinct Swift type.
        let swapped: (String, Int) = try Baml.tuples.swap_pair(t: (4, "d"))
        XCTAssertTupleEqual(swapped, ("d", 4))
        XCTAssertEqual(try Baml.tuples.pair_first(t: (5, "e")), 5)
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_round_trip_single_and_repeated() throws {
        // `(string,)` is a `Tuple1`, never a bare `String`.
        let single: Tuple1<String> = try Baml.tuples.round_trip_single(t: Tuple1("x"))
        XCTAssertEqual(single, Tuple1("x"))
        XCTAssertEqual(single.item0, "x")
        // Repeated slot types keep every slot.
        XCTAssertTupleEqual(try Baml.tuples.round_trip_repeated(t: (1, 2, 3)), (1, 2, 3))
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_round_trip_nested() throws {
        let result = try Baml.tuples.round_trip_nested(t: ((1, "inner"), true))
        XCTAssertTupleEqual(result.0, (1, "inner"))
        XCTAssertEqual(result.1, true)
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_round_trip_optional_slot_and_optional_tuple() throws {
        // An optional slot still occupies its position.
        let some = try Baml.tuples.round_trip_optional_slot(t: (1, "s"))
        XCTAssertEqual(some.0, 1)
        XCTAssertEqual(some.1, "s")
        let none = try Baml.tuples.round_trip_optional_slot(t: (1, nil))
        XCTAssertEqual(none.0, 1)
        XCTAssertNil(none.1)
        let tuple = try XCTUnwrap(try Baml.tuples.round_trip_optional_tuple(t: (1, "a")))
        XCTAssertTupleEqual(tuple, (1, "a"))
        XCTAssertNil(try Baml.tuples.round_trip_optional_tuple(t: nil))
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_round_trip_containers() throws {
        let xs = try Baml.tuples.round_trip_tuple_list(xs: [("a", 1), ("b", 2)])
        XCTAssertEqual(xs.count, 2)
        XCTAssertTupleEqual(xs[0], ("a", 1))
        XCTAssertTupleEqual(xs[1], ("b", 2))
        XCTAssertTrue(try Baml.tuples.round_trip_tuple_list(xs: []).isEmpty)
        let m = try Baml.tuples.round_trip_tuple_map(m: ["k": (1, true)])
        XCTAssertEqual(Array(m.keys), ["k"])
        XCTAssertTupleEqual(try XCTUnwrap(m["k"]), (1, true))
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_round_trip_union_arms() throws {
        let orString = BamlSlots.union(pair, BamlSlot<String>.own)
        for value: BamlUnion2<Pair, String> in [.t0((1, "a")), .t1("s")] {
            let result = try Baml.tuples.round_trip_tuple_or_string(u: value)
            XCTAssertTrue(orString._bamlEqual(result, value), "\(result)")
        }
        // Two same-arity tuple arms: the engine's selection picks the case.
        let arms = BamlSlots.union(pair, BamlSlots.tuple(BamlSlot<String>.own, BamlSlot<Int>.own))
        for value: BamlUnion2<Pair, (String, Int)> in [.t0((1, "a")), .t1(("a", 1))] {
            let result = try Baml.tuples.round_trip_tuple_arms(u: value)
            XCTAssertTrue(arms._bamlEqual(result, value), "\(result)")
        }
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_round_trip_class_fields() throws {
        let holder = Baml.tuples.TupleHolder(
            pair: (1, "a"),
            nested: ((2, "b"), false),
            maybe: (3, nil),
            pairs: [("c", 4)]
        )
        let result = try Baml.tuples.round_trip_holder(h: holder)
        // A struct holding tuples is still `Equatable`, slot by slot.
        XCTAssertEqual(result, holder)
        XCTAssertTupleEqual(result.pair, (1, "a"))
        var changed = holder
        changed.pairs[0].1 = 5
        XCTAssertNotEqual(changed, holder)
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_round_trip_generic_argument() throws {
        // A Swift type parameter takes only `BamlCodableValue`s, which a
        // native tuple cannot be, so `round_trip_tuple_box`'s
        // `TupleBox<(int, string)>` has no Swift signature. A `Tuple1` of a
        // conforming slot is a `BamlCodableValue`, and binds `T` itself.
        XCTAssertEqual(try Baml.tuples.tuple_generic_identity(x: Tuple1(3)), Tuple1(3))
        // A tuple whose slot is a bound type parameter.
        XCTAssertTupleEqual(try Baml.tuples.pair_with(x: "s", n: 1), ("s", 1))
        let nested = try Baml.tuples.pair_with(x: Tuple1(true), n: 2)
        XCTAssertEqual(nested.0, Tuple1(true))
        XCTAssertEqual(nested.1, 2)
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_host_callback_args_and_results() throws {
        let result = try Baml.tuples.call_tuple_callback(
            callback: { t in (t.1, t.0 * 10) },
            t: (4, "four")
        )
        XCTAssertTupleEqual(result, ("four", 40))
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_thrown_value() throws {
        do {
            _ = try Baml.tuples.throw_pair(t: (7, "boom"))
            XCTFail("expected BamlError")
        } catch let error as BamlError {
            XCTAssertTupleEqual(try error.value(as: pair), (7, "boom"))
        }
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_callback_forwards_a_thrown_tuple_under_its_carried_type() throws {
        // The callback lets a nested call's thrown tuple through: the bridge
        // restates it under the tuple type it carries, and it comes back out
        // of the outer call as the same tuple.
        do {
            _ = try Baml.tuples.call_tuple_callback(
                callback: { t in
                    _ = try Baml.tuples.throw_pair(t: t)
                    return (t.1, t.0)
                },
                t: (6, "forwarded")
            )
            XCTFail("expected BamlError")
        } catch let error as BamlError {
            XCTAssertTupleEqual(try error.value(as: pair), (6, "forwarded"))
        }
    }

    /// The thirteen-slot tuple of `round_trip_wide`.
    private func wide(_ offset: Int = 0) -> (Int, String, Int, String, Int, String, Int, String, Int, String, Int, String, Int) {
        (offset, "s1", 2, "s3", 4, "s5", 6, "s7", 8, "s9", 10, "s11", 12)
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_round_trip_wide() throws {
        // Wider than any fixed tuple family: a native tuple all the same.
        XCTAssertTupleEqual(try Baml.tuples.round_trip_wide(t: wide()), wide())
        var changed = wide()
        changed.0 = 100
        XCTAssertTupleEqual(try Baml.tuples.round_trip_wide(t: changed), changed)
        let twenty = (0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19)
        XCTAssertTupleEqual(try Baml.tuples.round_trip_wide20(t: twenty), twenty)
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_round_trip_wide_in_class_and_union() throws {
        let holder = Baml.tuples.WideTupleHolder(wide: wide(1), label: "w")
        XCTAssertEqual(try Baml.tuples.round_trip_wide_holder(h: holder), holder)
        let tuple = try Baml.tuples.round_trip_wide_or_string(u: .t0(wide(2)))
        XCTAssertTupleEqual(try XCTUnwrap(tuple.t0), wide(2))
        XCTAssertEqual(try Baml.tuples.round_trip_wide_or_string(u: .t1("s")).t1, "s")
    }

    /// The thrown `(int, string)` of `throw_pair`, as the raw value it
    /// arrives as.
    private func thrownPair(_ value: Pair) throws -> BamlOutboundValue {
        do {
            _ = try Baml.tuples.throw_pair(t: value)
        } catch let error as BamlError {
            return try XCTUnwrap(error.payload)
        }
        XCTFail("expected BamlError")
        throw BamlDecodeError.unsupported("no thrown value")
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_wrong_arity_and_slots_rejected() throws {
        // A Swift tuple's arity and slot types are static; a received value
        // is checked when decoded.
        let thrown = try thrownPair((1, "a"))
        XCTAssertTupleEqual(try pair._bamlDecode(thrown), (1, "a"))
        // Slot types swapped.
        XCTAssertThrowsError(
            try BamlSlots.tuple(BamlSlot<String>.own, BamlSlot<Int>.own)._bamlDecode(thrown)
        )
        // Too many and too few slots.
        XCTAssertThrowsError(
            try BamlSlots.tuple(BamlSlot<Int>.own, BamlSlot<String>.own, BamlSlot<Int>.own)._bamlDecode(thrown)
        )
        XCTAssertThrowsError(try Tuple1<Int>._bamlDecode(thrown))
        // A tuple never decodes as a list, whatever its length.
        XCTAssertThrowsError(try [BamlUnion2<Int, String>]._bamlDecode(thrown))
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_reflected_type_token() throws {
        // A tuple type's token is the tuple of its slot types, in order.
        XCTAssertEqual(pair.type, .tuple([.int, .string]))
        XCTAssertNotEqual(pair.type, BamlSlots.tuple(BamlSlot<String>.own, BamlSlot<Int>.own).type)
        XCTAssertEqual(Tuple1<String>._bamlStatedType, .tuple([.string]))
        // It is the type a received tuple carries.
        XCTAssertEqual(try thrownPair((2, "b")).carriedTupleType(), pair.type)
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_dynamic_value_keeps_carried_type() throws {
        // A tuple received with no declared type (a thrown value's raw
        // payload) keeps the tuple type it carries: a union decoding it takes
        // the arm that type spells, though its elements would fit the first
        // arm too. Swift spells no `unknown` position, so the payload is the
        // only dynamic value a tuple arrives as.
        let thrown = try thrownPair((5, "e"))
        XCTAssertEqual(thrown.carriedTupleType(), pair.type)
        let open = BamlSlots.tuple(BamlSlots.union(BamlSlot<Int>.own, BamlSlot<String>.own), BamlSlot<String>.own)
        guard case .t1(let picked) = try BamlSlots.union(open, pair)._bamlDecode(thrown) else {
            return XCTFail("expected the arm the carried type spells")
        }
        XCTAssertTupleEqual(picked, (5, "e"))
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_cross_by_copy() throws {
        // Changing a host tuple after sending it changes nothing BAML holds,
        // and changing a received tuple changes nothing BAML sends next.
        let box = Baml.tuples.TuplePairBox(a: 1, b: "a")
        var received = try box.as_pair()
        received.0 = 99
        XCTAssertEqual(received.0, 99)
        XCTAssertTupleEqual(try box.as_pair(), (1, "a"))

        var sent: Pair = (2, "b")
        let built = try Baml.tuples.TuplePairBox.from_pair(p: sent)
        sent.1 = "changed"
        XCTAssertEqual(built, Baml.tuples.TuplePairBox(a: 2, b: "b"))
        XCTAssertTupleEqual(try built.as_pair(), (2, "b"))
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_method_round_trip() async throws {
        let box = Baml.tuples.TuplePairBox(a: 1, b: "a")
        // An instance method returning a tuple, and one taking a tuple.
        XCTAssertTupleEqual(try box.as_pair(), (1, "a"))
        XCTAssertEqual(try box.with_pair(p: (2, "b")), Baml.tuples.TuplePairBox(a: 2, b: "b"))
        // A static method taking a tuple.
        XCTAssertEqual(
            try Baml.tuples.TuplePairBox.from_pair(p: (3, "c")),
            Baml.tuples.TuplePairBox(a: 3, b: "c")
        )
        let asPair = try await box.as_pair_async()
        XCTAssertTupleEqual(asPair, (1, "a"))
        let withPair = try await box.with_pair_async(p: (4, "d"))
        XCTAssertEqual(withPair, Baml.tuples.TuplePairBox(a: 4, b: "d"))
        let fromPair = try await Baml.tuples.TuplePairBox.from_pair_async(p: (5, "e"))
        XCTAssertEqual(fromPair, Baml.tuples.TuplePairBox(a: 5, b: "e"))
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_function_handle_round_trip() async throws {
        // A BAML closure returned to the host: its handle takes and returns
        // tuples.
        let swap = try Baml.tuples.make_tuple_swapper()
        let first = try await swap((1, "a"))
        XCTAssertTupleEqual(first, ("a", 1))
        let second = try await swap((2, "b"))
        XCTAssertTupleEqual(second, ("b", 2))
    }
}
