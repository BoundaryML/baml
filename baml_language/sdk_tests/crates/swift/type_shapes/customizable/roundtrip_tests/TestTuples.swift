import XCTest
import Baml
import BamlBridge

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

    private let pair = BamlSlots.tuple(BamlSlot<Int>.own, BamlSlot<String>.own)

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_round_trip_pair() throws {
        XCTAssertTupleEqual(try Baml.tuples.round_trip_pair_sync(t: (1, "a")), (1, "a"))
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_round_trip_pair_async() async throws {
        let result = try await Baml.tuples.round_trip_pair(t: (2, "b"))
        XCTAssertTupleEqual(result, (2, "b"))
    }

    // SDK_PARITY_LINT(skip): Swift-only: a received tuple destructures as a native Swift tuple
    func test_tuples_native_tuple_bridging() throws {
        let (n, s) = try Baml.tuples.make_pair_sync(n: 3, s: "c")
        XCTAssertEqual(n, 3)
        XCTAssertEqual(s, "c")
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_make_swap_and_index() throws {
        let swapped: (String, Int) = try Baml.tuples.swap_pair_sync(t: (4, "d"))
        XCTAssertTupleEqual(swapped, ("d", 4))
        XCTAssertEqual(try Baml.tuples.pair_first_sync(t: (5, "e")), 5)
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_round_trip_single_and_repeated() throws {
        let single: Tuple1<String> = try Baml.tuples.round_trip_single_sync(t: Tuple1("x"))
        XCTAssertEqual(single, Tuple1("x"))
        XCTAssertEqual(single.item0, "x")
        XCTAssertTupleEqual(try Baml.tuples.round_trip_repeated_sync(t: (1, 2, 3)), (1, 2, 3))
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_round_trip_nested() throws {
        let result = try Baml.tuples.round_trip_nested_sync(t: ((1, "inner"), true))
        XCTAssertTupleEqual(result.0, (1, "inner"))
        XCTAssertEqual(result.1, true)
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_round_trip_optional_slot_and_optional_tuple() throws {
        let some = try Baml.tuples.round_trip_optional_slot_sync(t: (1, "s"))
        XCTAssertEqual(some.0, 1)
        XCTAssertEqual(some.1, "s")
        let none = try Baml.tuples.round_trip_optional_slot_sync(t: (1, nil))
        XCTAssertEqual(none.0, 1)
        XCTAssertNil(none.1)
        let tuple = try XCTUnwrap(try Baml.tuples.round_trip_optional_tuple_sync(t: (1, "a")))
        XCTAssertTupleEqual(tuple, (1, "a"))
        XCTAssertNil(try Baml.tuples.round_trip_optional_tuple_sync(t: nil))
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_round_trip_containers() throws {
        let xs = try Baml.tuples.round_trip_tuple_list_sync(xs: [("a", 1), ("b", 2)])
        XCTAssertEqual(xs.count, 2)
        XCTAssertTupleEqual(xs[0], ("a", 1))
        XCTAssertTupleEqual(xs[1], ("b", 2))
        XCTAssertTrue(try Baml.tuples.round_trip_tuple_list_sync(xs: []).isEmpty)
        let m = try Baml.tuples.round_trip_tuple_map_sync(m: ["k": (1, true)])
        XCTAssertEqual(Array(m.keys), ["k"])
        XCTAssertTupleEqual(try XCTUnwrap(m["k"]), (1, true))
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_round_trip_union_arms() throws {
        let orString = BamlSlots.union(pair, BamlSlot<String>.own)
        for value: BamlUnion2<Pair, String> in [.t0((1, "a")), .t1("s")] {
            let result = try Baml.tuples.round_trip_tuple_or_string_sync(u: value)
            XCTAssertTrue(orString._bamlEqual(result, value), "\(result)")
        }
        let arms = BamlSlots.union(pair, BamlSlots.tuple(BamlSlot<String>.own, BamlSlot<Int>.own))
        for value: BamlUnion2<Pair, (String, Int)> in [.t0((1, "a")), .t1(("a", 1))] {
            let result = try Baml.tuples.round_trip_tuple_arms_sync(u: value)
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
        let result = try Baml.tuples.round_trip_holder_sync(h: holder)
        XCTAssertEqual(result, holder)
        XCTAssertTupleEqual(result.pair, (1, "a"))
        var changed = holder
        changed.pairs[0].1 = 5
        XCTAssertNotEqual(changed, holder)
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_round_trip_generic_argument() throws {
        XCTAssertEqual(try Baml.tuples.tuple_generic_identity_sync(x: Tuple1(3)), Tuple1(3))
        XCTAssertTupleEqual(try Baml.tuples.pair_with_sync(x: "s", n: 1), ("s", 1))
        let nested = try Baml.tuples.pair_with_sync(x: Tuple1(true), n: 2)
        XCTAssertEqual(nested.0, Tuple1(true))
        XCTAssertEqual(nested.1, 2)
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_host_callback_args_and_results() throws {
        let result = try Baml.tuples.call_tuple_callback_sync(
            callback: { t in (t.1, t.0 * 10) },
            t: (4, "four")
        )
        XCTAssertTupleEqual(result, ("four", 40))
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_thrown_value() throws {
        do {
            _ = try Baml.tuples.throw_pair_sync(t: (7, "boom"))
            XCTFail("expected BamlError")
        } catch let error as BamlError {
            XCTAssertTupleEqual(try error.value(as: pair), (7, "boom"))
        }
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_callback_forwards_a_thrown_tuple_under_its_carried_type() throws {
        do {
            _ = try Baml.tuples.call_tuple_callback_sync(
                callback: { t in
                    _ = try Baml.tuples.throw_pair_sync(t: t)
                    return (t.1, t.0)
                },
                t: (6, "forwarded")
            )
            XCTFail("expected BamlError")
        } catch let error as BamlError {
            XCTAssertTupleEqual(try error.value(as: pair), (6, "forwarded"))
        }
    }

    private func wide(_ offset: Int = 0) -> (Int, String, Int, String, Int, String, Int, String, Int, String, Int, String, Int) {
        (offset, "s1", 2, "s3", 4, "s5", 6, "s7", 8, "s9", 10, "s11", 12)
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_round_trip_wide() throws {
        XCTAssertTupleEqual(try Baml.tuples.round_trip_wide_sync(t: wide()), wide())
        var changed = wide()
        changed.0 = 100
        XCTAssertTupleEqual(try Baml.tuples.round_trip_wide_sync(t: changed), changed)
        let twenty = (0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19)
        XCTAssertTupleEqual(try Baml.tuples.round_trip_wide20_sync(t: twenty), twenty)
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_round_trip_wide_in_class_and_union() throws {
        let holder = Baml.tuples.WideTupleHolder(wide: wide(1), label: "w")
        XCTAssertEqual(try Baml.tuples.round_trip_wide_holder_sync(h: holder), holder)
        let tuple = try Baml.tuples.round_trip_wide_or_string_sync(u: .t0(wide(2)))
        XCTAssertTupleEqual(try XCTUnwrap(tuple.t0), wide(2))
        XCTAssertEqual(try Baml.tuples.round_trip_wide_or_string_sync(u: .t1("s")).t1, "s")
    }

    private func thrownPair(_ value: Pair) throws -> BamlOutboundValue {
        do {
            _ = try Baml.tuples.throw_pair_sync(t: value)
        } catch let error as BamlError {
            return try XCTUnwrap(error.payload)
        }
        XCTFail("expected BamlError")
        throw BamlDecodeError.unsupported("no thrown value")
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_wrong_arity_and_slots_rejected() throws {
        let thrown = try thrownPair((1, "a"))
        XCTAssertTupleEqual(try pair._bamlDecode(thrown), (1, "a"))
        XCTAssertThrowsError(
            try BamlSlots.tuple(BamlSlot<String>.own, BamlSlot<Int>.own)._bamlDecode(thrown)
        )
        XCTAssertThrowsError(
            try BamlSlots.tuple(BamlSlot<Int>.own, BamlSlot<String>.own, BamlSlot<Int>.own)._bamlDecode(thrown)
        )
        XCTAssertThrowsError(try Tuple1<Int>._bamlDecode(thrown))
        XCTAssertThrowsError(try [BamlUnion2<Int, String>]._bamlDecode(thrown))
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_reflected_type_token() throws {
        XCTAssertEqual(pair.type, .tuple([.int, .string]))
        XCTAssertNotEqual(pair.type, BamlSlots.tuple(BamlSlot<String>.own, BamlSlot<Int>.own).type)
        XCTAssertEqual(Tuple1<String>._bamlStatedType, .tuple([.string]))
        XCTAssertEqual(try thrownPair((2, "b")).carriedTupleType(), pair.type)
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_dynamic_value_keeps_carried_type() throws {
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
        let box = Baml.tuples.TuplePairBox(a: 1, b: "a")
        var received = try box.as_pair_sync()
        received.0 = 99
        XCTAssertEqual(received.0, 99)
        XCTAssertTupleEqual(try box.as_pair_sync(), (1, "a"))

        var sent: Pair = (2, "b")
        let built = try Baml.tuples.TuplePairBox.from_pair_sync(p: sent)
        sent.1 = "changed"
        XCTAssertEqual(built, Baml.tuples.TuplePairBox(a: 2, b: "b"))
        XCTAssertTupleEqual(try built.as_pair_sync(), (2, "b"))
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_method_round_trip() async throws {
        let box = Baml.tuples.TuplePairBox(a: 1, b: "a")
        XCTAssertTupleEqual(try box.as_pair_sync(), (1, "a"))
        XCTAssertEqual(try box.with_pair_sync(p: (2, "b")), Baml.tuples.TuplePairBox(a: 2, b: "b"))
        XCTAssertEqual(
            try Baml.tuples.TuplePairBox.from_pair_sync(p: (3, "c")),
            Baml.tuples.TuplePairBox(a: 3, b: "c")
        )
        let asPair = try await box.as_pair()
        XCTAssertTupleEqual(asPair, (1, "a"))
        let withPair = try await box.with_pair(p: (4, "d"))
        XCTAssertEqual(withPair, Baml.tuples.TuplePairBox(a: 4, b: "d"))
        let fromPair = try await Baml.tuples.TuplePairBox.from_pair(p: (5, "e"))
        XCTAssertEqual(fromPair, Baml.tuples.TuplePairBox(a: 5, b: "e"))
    }

    // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
    func test_tuples_function_handle_round_trip() async throws {
        let swap = try Baml.tuples.make_tuple_swapper_sync()
        let first = try await swap((1, "a"))
        XCTAssertTupleEqual(first, ("a", 1))
        let second = try await swap((2, "b"))
        XCTAssertTupleEqual(second, ("b", 2))
    }
}
