// Roundtrip coverage for `Baml.literals` — port of python_pydantic2
// `roundtrip_tests/test_literals.py`.
//
// Standalone literal types collapse to their Swift base type (Swift
// has no literal types; the engine re-validates values). Literal
// *unions* get their own raw-value enums — see TestComplexModels.
import XCTest
import Baml

final class TestLiterals: XCTestCase {
    func test_literals_return_literals() throws {
        XCTAssertEqual(try Baml.literals.return_literal42(), 42)
        XCTAssertEqual(try Baml.literals.return_literal_neg_one(), -1)
        XCTAssertEqual(try Baml.literals.return_literal_draft(), "draft")
        XCTAssertEqual(try Baml.literals.return_literal_escaped(), "has \"quotes\"")
        XCTAssertEqual(try Baml.literals.return_literal_true(), true)
        XCTAssertEqual(try Baml.literals.return_literal_false(), false)
    }

    func test_literals_round_trip_literal42() throws {
        XCTAssertEqual(try Baml.literals.round_trip_literal42(x: 42), 42)
    }

    func test_literals_round_trip_literal_draft() throws {
        XCTAssertEqual(try Baml.literals.round_trip_literal_draft(x: "draft"), "draft")
    }

    func test_literals_round_trip_literal_escaped() throws {
        XCTAssertEqual(
            try Baml.literals.round_trip_literal_escaped(x: "has \"quotes\""),
            "has \"quotes\""
        )
    }

    func test_literals_round_trip_literal_true() throws {
        XCTAssertEqual(try Baml.literals.round_trip_literal_true(x: true), true)
    }

    func test_literals_round_trip_literal_false() throws {
        XCTAssertEqual(try Baml.literals.round_trip_literal_false(x: false), false)
    }

    func test_literals_round_trip_literals() throws {
        let lit = Baml.literals.Literals(
            literal_42: 42,
            literal_draft: "draft",
            literal_escaped: "has \"quotes\"",
            literal_true: true,
            literal_false: false
        )
        XCTAssertEqual(try Baml.literals.round_trip_literals(l: lit), lit)
    }
}
