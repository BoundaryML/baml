// Roundtrip coverage for `Baml.tuples`.
//
// A BAML tuple is the runtime's `BamlTupleN` family everywhere it appears:
// `(int, string)` is `BamlTuple2<Int, String>`, with no generated tuple
// names (structural identity, like `BamlUnionN`). A bare Swift tuple
// conforms to no protocol, so it cannot fill a position itself; a
// `BamlTupleN` carries one as `values`. A tuple wider than `BamlTuple12`
// crosses as an array of the join of its slot types, one element per slot.
import XCTest
import Baml
import BamlBridge

final class TestTuples: XCTestCase {
    typealias Pair = BamlTuple2<Int, String>

    func test_tuples_round_trip_pair() throws {
        let pair = Pair(1, "a")
        XCTAssertEqual(try Baml.tuples.round_trip_pair(t: pair), pair)
    }

    func test_tuples_round_trip_pair_async() async throws {
        let pair = Pair(2, "b")
        let result = try await Baml.tuples.round_trip_pair_async(t: pair)
        XCTAssertEqual(result, pair)
    }

    func test_tuples_native_tuple_bridging() throws {
        // Built from, and read back as, a native Swift tuple.
        let result = try Baml.tuples.make_pair(n: 3, s: "c")
        let (n, s) = result.values
        XCTAssertEqual(n, 3)
        XCTAssertEqual(s, "c")
        XCTAssertEqual(result, Pair((3, "c")))
        XCTAssertEqual(result.t0, 3)
        XCTAssertEqual(result.t1, "c")
    }

    func test_tuples_slot_order_is_identity() throws {
        // `(int, string)` -> `(string, int)`: a distinct Swift type.
        let swapped: BamlTuple2<String, Int> = try Baml.tuples.swap_pair(t: Pair(4, "d"))
        XCTAssertEqual(swapped, BamlTuple2("d", 4))
        XCTAssertEqual(try Baml.tuples.pair_first(t: Pair(5, "e")), 5)
    }

    func test_tuples_round_trip_single_and_repeated() throws {
        XCTAssertEqual(try Baml.tuples.round_trip_single(t: BamlTuple1("x")), BamlTuple1("x"))
        // Repeated slot types keep every slot.
        let repeated = BamlTuple3(1, 2, 3)
        XCTAssertEqual(try Baml.tuples.round_trip_repeated(t: repeated), repeated)
    }

    func test_tuples_round_trip_nested() throws {
        let nested = BamlTuple2(Pair(1, "inner"), true)
        XCTAssertEqual(try Baml.tuples.round_trip_nested(t: nested), nested)
    }

    func test_tuples_round_trip_optional_slot_and_optional_tuple() throws {
        // An optional slot still occupies its position.
        let some = BamlTuple2<Int, String?>(1, "s")
        let none = BamlTuple2<Int, String?>(1, nil)
        XCTAssertEqual(try Baml.tuples.round_trip_optional_slot(t: some), some)
        XCTAssertEqual(try Baml.tuples.round_trip_optional_slot(t: none), none)
        XCTAssertEqual(try Baml.tuples.round_trip_optional_tuple(t: Pair(1, "a")), Pair(1, "a"))
        XCTAssertNil(try Baml.tuples.round_trip_optional_tuple(t: nil))
    }

    func test_tuples_round_trip_containers() throws {
        let xs = [BamlTuple2("a", 1), BamlTuple2("b", 2)]
        XCTAssertEqual(try Baml.tuples.round_trip_tuple_list(xs: xs), xs)
        XCTAssertEqual(try Baml.tuples.round_trip_tuple_list(xs: []), [])
        let m = ["k": BamlTuple2(1, true)]
        XCTAssertEqual(try Baml.tuples.round_trip_tuple_map(m: m), m)
    }

    func test_tuples_round_trip_union_arms() throws {
        let tuple: BamlUnion2<Pair, String> = .t0(Pair(1, "a"))
        let string: BamlUnion2<Pair, String> = .t1("s")
        XCTAssertEqual(try Baml.tuples.round_trip_tuple_or_string(u: tuple), tuple)
        XCTAssertEqual(try Baml.tuples.round_trip_tuple_or_string(u: string), string)
        // Two same-arity tuple arms: the engine's selection picks the case.
        let first: BamlUnion2<Pair, BamlTuple2<String, Int>> = .t0(Pair(1, "a"))
        let second: BamlUnion2<Pair, BamlTuple2<String, Int>> = .t1(BamlTuple2("a", 1))
        XCTAssertEqual(try Baml.tuples.round_trip_tuple_arms(u: first), first)
        XCTAssertEqual(try Baml.tuples.round_trip_tuple_arms(u: second), second)
    }

    func test_tuples_round_trip_class_fields() throws {
        let holder = Baml.tuples.TupleHolder(
            pair: Pair(1, "a"),
            nested: BamlTuple2(Pair(2, "b"), false),
            maybe: BamlTuple2(3, nil),
            pairs: [BamlTuple2("c", 4)]
        )
        XCTAssertEqual(try Baml.tuples.round_trip_holder(h: holder), holder)
    }

    func test_tuples_round_trip_generic_argument() throws {
        let box = Baml.tuples.TupleBox<Pair>(value: Pair(9, "z"))
        XCTAssertEqual(try Baml.tuples.round_trip_tuple_box(b: box), box)
    }

    func test_tuples_host_callback_receives_and_returns_tuples() throws {
        let result = try Baml.tuples.call_tuple_callback(
            callback: { t in BamlTuple2(t.t1, t.t0 * 10) },
            t: Pair(4, "four")
        )
        XCTAssertEqual(result, BamlTuple2("four", 40))
    }

    func test_tuples_thrown_tuple_decodes_from_the_error_payload() throws {
        do {
            _ = try Baml.tuples.throw_pair(t: Pair(7, "boom"))
            XCTFail("expected BamlError")
        } catch let error as BamlError {
            XCTAssertEqual(try error.value(as: Pair.self), Pair(7, "boom"))
        }
    }

    func test_tuples_callback_forwards_a_thrown_tuple_under_its_carried_type() throws {
        // The callback lets a nested call's thrown tuple through: the bridge
        // restates it under the tuple type it carries, and it comes back out
        // of the outer call as the same tuple.
        do {
            _ = try Baml.tuples.call_tuple_callback(
                callback: { t in
                    _ = try Baml.tuples.throw_pair(t: t)
                    return BamlTuple2(t.t1, t.t0)
                },
                t: Pair(6, "forwarded")
            )
            XCTFail("expected BamlError")
        } catch let error as BamlError {
            XCTAssertEqual(try error.value(as: Pair.self), Pair(6, "forwarded"))
        }
    }

    func test_tuples_wide_tuple_crosses_as_a_list_of_the_join() throws {
        // Thirteen slots: `[BamlUnion2<Int, String>]`, one element per slot,
        // each still checked by the engine against its own slot type.
        let wide: [BamlUnion2<Int, String>] = (0..<13).map { i in
            i % 2 == 0 ? .t0(i) : .t1("s\(i)")
        }
        XCTAssertEqual(try Baml.tuples.round_trip_wide(t: wide), wide)
    }

    func test_tuples_wide_tuple_rejects_wrong_arity() {
        let short: [BamlUnion2<Int, String>] = [.t0(0), .t1("s1")]
        XCTAssertThrowsError(try Baml.tuples.round_trip_wide(t: short))
    }

    func test_tuples_wide_tuple_rejects_a_wrong_slot() {
        // Right arity, but slot 0 is `int` and receives a string: the
        // union-list representation does not make every permutation fit.
        var swapped: [BamlUnion2<Int, String>] = (0..<13).map { i in
            i % 2 == 0 ? .t0(i) : .t1("s\(i)")
        }
        swapped[0] = .t1("not an int")
        XCTAssertThrowsError(try Baml.tuples.round_trip_wide(t: swapped))
    }
}
