// Roundtrip coverage for `Baml.tuples`.
//
// A BAML tuple is the runtime's `BamlTupleN` family everywhere it appears:
// `(int, string)` is `BamlTuple2<Int, String>` (structural identity, like
// `BamlUnionN`). A bare Swift tuple conforms to no protocol, so it cannot
// fill a position itself; a `BamlTupleN` carries one as `values`. A tuple
// wider than `BamlTuple12` is a generated `Baml.TupleN_...` struct of the
// same shape, one per tuple type.
import XCTest
import Baml
import BamlBridge

final class TestTuples: XCTestCase {
    typealias Pair = BamlTuple2<Int, String>

    // SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
    func test_tuples_round_trip_pair() throws {
        let pair = Pair(1, "a")
        XCTAssertEqual(try Baml.tuples.round_trip_pair(t: pair), pair)
    }

    // SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
    func test_tuples_round_trip_pair_async() async throws {
        let pair = Pair(2, "b")
        let result = try await Baml.tuples.round_trip_pair_async(t: pair)
        XCTAssertEqual(result, pair)
    }

    // SDK_PARITY_LINT(skip): Swift-only: BamlTupleN is built from and read back as a native Swift tuple
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

    // SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
    func test_tuples_make_swap_and_index() throws {
        // `(int, string)` -> `(string, int)`: a distinct Swift type.
        let swapped: BamlTuple2<String, Int> = try Baml.tuples.swap_pair(t: Pair(4, "d"))
        XCTAssertEqual(swapped, BamlTuple2("d", 4))
        XCTAssertEqual(try Baml.tuples.pair_first(t: Pair(5, "e")), 5)
    }

    // SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
    func test_tuples_round_trip_single_and_repeated() throws {
        XCTAssertEqual(try Baml.tuples.round_trip_single(t: BamlTuple1("x")), BamlTuple1("x"))
        // Repeated slot types keep every slot.
        let repeated = BamlTuple3(1, 2, 3)
        XCTAssertEqual(try Baml.tuples.round_trip_repeated(t: repeated), repeated)
    }

    // SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
    func test_tuples_round_trip_nested() throws {
        let nested = BamlTuple2(Pair(1, "inner"), true)
        XCTAssertEqual(try Baml.tuples.round_trip_nested(t: nested), nested)
    }

    // SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
    func test_tuples_round_trip_optional_slot_and_optional_tuple() throws {
        // An optional slot still occupies its position.
        let some = BamlTuple2<Int, String?>(1, "s")
        let none = BamlTuple2<Int, String?>(1, nil)
        XCTAssertEqual(try Baml.tuples.round_trip_optional_slot(t: some), some)
        XCTAssertEqual(try Baml.tuples.round_trip_optional_slot(t: none), none)
        XCTAssertEqual(try Baml.tuples.round_trip_optional_tuple(t: Pair(1, "a")), Pair(1, "a"))
        XCTAssertNil(try Baml.tuples.round_trip_optional_tuple(t: nil))
    }

    // SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
    func test_tuples_round_trip_containers() throws {
        let xs = [BamlTuple2("a", 1), BamlTuple2("b", 2)]
        XCTAssertEqual(try Baml.tuples.round_trip_tuple_list(xs: xs), xs)
        XCTAssertEqual(try Baml.tuples.round_trip_tuple_list(xs: []), [])
        let m = ["k": BamlTuple2(1, true)]
        XCTAssertEqual(try Baml.tuples.round_trip_tuple_map(m: m), m)
    }

    // SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
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

    // SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
    func test_tuples_round_trip_class_fields() throws {
        let holder = Baml.tuples.TupleHolder(
            pair: Pair(1, "a"),
            nested: BamlTuple2(Pair(2, "b"), false),
            maybe: BamlTuple2(3, nil),
            pairs: [BamlTuple2("c", 4)]
        )
        XCTAssertEqual(try Baml.tuples.round_trip_holder(h: holder), holder)
    }

    // SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
    func test_tuples_round_trip_generic_argument() throws {
        let box = Baml.tuples.TupleBox<Pair>(value: Pair(9, "z"))
        XCTAssertEqual(try Baml.tuples.round_trip_tuple_box(b: box), box)
    }

    // SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
    func test_tuples_host_callback_args_and_results() throws {
        let result = try Baml.tuples.call_tuple_callback(
            callback: { t in BamlTuple2(t.t1, t.t0 * 10) },
            t: Pair(4, "four")
        )
        XCTAssertEqual(result, BamlTuple2("four", 40))
    }

    // SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
    func test_tuples_thrown_value() throws {
        do {
            _ = try Baml.tuples.throw_pair(t: Pair(7, "boom"))
            XCTFail("expected BamlError")
        } catch let error as BamlError {
            XCTAssertEqual(try error.value(as: Pair.self), Pair(7, "boom"))
        }
    }

    // SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
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

    typealias Wide = Baml.Tuple13_IntStringIntStringIntStringIntStringIntStringIntStringInt

    private func wide(_ offset: Int = 0) -> Wide {
        Wide(
            offset, "s1", 2, "s3", 4, "s5", 6, "s7", 8, "s9", 10, "s11", 12
        )
    }

    /// The thrown `(int, string)` of `throw_pair`, as the dynamic value it
    /// arrives as.
    private func thrownPair(_ pair: Pair) throws -> BamlOutboundValue {
        do {
            _ = try Baml.tuples.throw_pair(t: pair)
        } catch let error as BamlError {
            return try XCTUnwrap(error.payload)
        }
        XCTFail("expected BamlError")
        throw BamlDecodeError.unsupported("no thrown value")
    }

    // SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
    func test_tuples_round_trip_wide() throws {
        // Thirteen slots: the generated `Tuple13_...` struct, slots in
        // order, read as a native Swift tuple or one slot at a time.
        let value = wide()
        XCTAssertEqual(try Baml.tuples.round_trip_wide(t: value), value)
        XCTAssertEqual(value.t12, 12)
        XCTAssertEqual(value.values.11, "s11")
        var changed = value
        changed.values.0 = 100
        XCTAssertEqual(changed.t0, 100)
        XCTAssertEqual(value.t0, 0, "a tuple struct is a value")
        XCTAssertEqual(try Baml.tuples.round_trip_wide(t: changed), changed)
    }

    // SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
    func test_tuples_wrong_arity_and_slots_rejected() throws {
        // `BamlTupleN` and the generated structs make a wrong arity or slot
        // unrepresentable host-side; a dynamic value is checked when decoded.
        let thrown = try thrownPair(Pair(1, "a"))
        XCTAssertEqual(try Pair._bamlDecode(thrown), Pair(1, "a"))
        // Slot types swapped.
        XCTAssertThrowsError(try BamlTuple2<String, Int>._bamlDecode(thrown))
        // Too many and too few slots.
        XCTAssertThrowsError(try BamlTuple3<Int, String, Int>._bamlDecode(thrown))
        XCTAssertThrowsError(try BamlTuple1<Int>._bamlDecode(thrown))
        XCTAssertThrowsError(try Wide._bamlDecode(thrown))
        // A tuple never decodes as a list, whatever its length.
        XCTAssertThrowsError(try [BamlUnion2<Int, String>]._bamlDecode(thrown))
    }

    // SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
    func test_tuples_reflected_type_token() throws {
        // A tuple type's token is the tuple of its slot types, in order.
        XCTAssertEqual(Pair._bamlStatedType, .tuple([.int, .string]))
        XCTAssertNotEqual(Pair._bamlStatedType, BamlTuple2<String, Int>._bamlStatedType)
        XCTAssertEqual(
            Wide._bamlStatedType,
            .tuple((0..<13).map { $0 % 2 == 0 ? .int : .string })
        )
        // It is the type a received tuple carries...
        XCTAssertEqual(try thrownPair(Pair(2, "b")).carriedTupleType(), Pair._bamlStatedType)
        // ...and binds a type parameter: each comes back as its own type.
        XCTAssertEqual(try Baml.tuples.tuple_generic_identity(x: Pair(3, "c")), Pair(3, "c"))
        XCTAssertEqual(try Baml.tuples.tuple_generic_identity(x: wide(3)), wide(3))
        let tuple: BamlUnion2<Wide, String> = .t0(wide(4))
        XCTAssertEqual(try Baml.tuples.tuple_generic_identity(x: tuple), tuple)
        let string: BamlUnion2<Wide, String> = .t1("s")
        XCTAssertEqual(try Baml.tuples.tuple_generic_identity(x: string), string)
    }

    // SDK_PARITY_LINT(skip): Rust and C# cover tuples outside type_shapes
    func test_tuples_dynamic_value_keeps_carried_type() throws {
        // A tuple received as a dynamic value keeps the tuple type it
        // carries: sent back at an open type parameter it is restated under
        // that type, and comes back carrying it still.
        let thrown = try thrownPair(Pair(5, "e"))
        let echoed = try Baml.tuples.tuple_generic_identity(x: thrown)
        XCTAssertEqual(echoed.carriedTupleType(), Pair._bamlStatedType)
        XCTAssertEqual(try Pair._bamlDecode(echoed), Pair(5, "e"))
        // A union decoding it takes the arm its carried type spells, though
        // its elements would fit the first arm too.
        typealias Open = BamlTuple2<BamlUnion2<Int, String>, String>
        let arms = try BamlUnion2<Open, Pair>._bamlDecode(echoed)
        XCTAssertEqual(arms, .t1(Pair(5, "e")))
    }
}
