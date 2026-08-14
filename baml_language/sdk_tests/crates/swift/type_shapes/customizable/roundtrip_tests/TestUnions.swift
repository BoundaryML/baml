// Roundtrip coverage for `Baml.unions` — port of python_pydantic2
// `roundtrip_tests/test_unions.py`, on the BamlUnionN generic family
// (design: sdks/swift/docs/unions-design.md).
//
// `int | string` is `BamlUnion2<Int, String>` everywhere — no
// generated union names. Construction is positional (`.t0(1)`) or
// type-directed (`.init("s")`); consumption is exhaustive `switch` /
// `match`, or type-directed `value(as:)`.
import XCTest
import Baml
import BamlBridge

final class TestUnions: XCTestCase {
    func test_unions_round_trip_null_to_end() throws {
        XCTAssertEqual(try Baml.unions.round_trip_null_to_end(u: .t0(1)), .t0(1))
        XCTAssertEqual(try Baml.unions.round_trip_null_to_end(u: .t1("s")), .t1("s"))
        XCTAssertNil(try Baml.unions.round_trip_null_to_end(u: nil))
    }

    func test_unions_round_trip_dedup() throws {
        // `int | int | string` dedups upstream: same BamlUnion2.
        XCTAssertEqual(try Baml.unions.round_trip_dedup(u: .t0(2)), .t0(2))
        XCTAssertEqual(try Baml.unions.round_trip_dedup(u: .t1("x")), .t1("x"))
    }

    func test_unions_round_trip_singleton_unwrap() throws {
        // `int | int` collapses to plain `Int` — no union type at all.
        XCTAssertEqual(try Baml.unions.round_trip_singleton_unwrap(u: 7), 7)
    }

    func test_unions_round_trip_optional_plus_null() throws {
        let t = Baml.unions.T(v: 1)
        XCTAssertEqual(try Baml.unions.round_trip_optional_plus_null(u: .t0(t)), .t0(t))
        XCTAssertEqual(try Baml.unions.round_trip_optional_plus_null(u: .t1("s")), .t1("s"))
        XCTAssertNil(try Baml.unions.round_trip_optional_plus_null(u: nil))
    }

    func test_unions_round_trip_t() throws {
        XCTAssertEqual(try Baml.unions.round_trip_t(t: Baml.unions.T(v: 4)), Baml.unions.T(v: 4))
    }

    func test_unions_round_trip_union_container() throws {
        let c = Baml.unions.UnionContainer(
            null_to_end: nil,
            dedup: .t1("d"),
            singleton_unwrap: 5,
            optional_plus_null: .t0(Baml.unions.T(v: 2))
        )
        XCTAssertEqual(try Baml.unions.round_trip_union_container(c: c), c)
    }

    func test_round_trip_empty_list_preserves_selected_arm() throws {
        let strings: BamlUnion2<[String], [Int]> = .t0([])
        let ints: BamlUnion2<[String], [Int]> = .t1([])

        XCTAssertEqual(try Baml.unions.round_trip_str_or_int_list(x: strings), strings)
        XCTAssertEqual(try Baml.unions.round_trip_str_or_int_list(x: ints), ints)
    }

    // Swift-specific: the three consumption tiers over one result.
    func test_unions_consumption_surfaces() throws {
        let result = try Baml.unions.round_trip_dedup(u: .init("hi"))  // type-directed init

        // 1. Exhaustive native switch (compiler-checked coverage).
        switch result {
        case .t0(let n): XCTFail("unexpected int arm: \(n)")
        case .t1(let s): XCTAssertEqual(s, "hi")
        }

        // 2. match — the canonical cross-bridge API.
        let display = result.match(
            t0: { n in "int: \(n)" },
            t1: { s in "string: \(s)" }
        )
        XCTAssertEqual(display, "string: hi")

        // 3. Type-directed access (insertion-stable).
        XCTAssertEqual(result.value(as: String.self), "hi")
        XCTAssertNil(result.value(as: Int.self))
        XCTAssertTrue(result.holds(String.self))
        XCTAssertEqual(result.t1, "hi")
        XCTAssertNil(result.t0)
    }
}
