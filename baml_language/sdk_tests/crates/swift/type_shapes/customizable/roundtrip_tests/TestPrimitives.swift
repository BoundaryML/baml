// Roundtrip coverage for `Baml.primitives` — port of
// python_pydantic2 `roundtrip_tests/test_primitives.py`.
//
// Ported 1:1 where the capability exists. Not yet ported (arrive with
// their phases): test_round_trip_primitives + the float_field variant
// (classes, Phase 2). test_round_trip_float_accepts_int is Python-only
// (Swift's type system forbids passing Int where Double is declared).
import XCTest
import Foundation
import Baml
import BamlBridge

final class TestPrimitives: XCTestCase {
    func test_return_int() throws {
        XCTAssertEqual(try Baml.primitives.return_int(), 42)
    }

    func test_return_float() throws {
        XCTAssertEqual(try Baml.primitives.return_float(), 3.14)
    }

    func test_return_string() throws {
        XCTAssertEqual(try Baml.primitives.return_string(), "hello")
    }

    func test_return_bool() throws {
        XCTAssertEqual(try Baml.primitives.return_bool(), true)
    }

    func test_return_null() throws {
        XCTAssertEqual(try Baml.primitives.return_null(), BamlNull())
    }

    func test_round_trip_int() throws {
        XCTAssertEqual(try Baml.primitives.round_trip_int(x: 7), 7)
    }

    func test_round_trip_float() throws {
        XCTAssertEqual(try Baml.primitives.round_trip_float(x: 2.5), 2.5)
    }

    func test_round_trip_string() throws {
        XCTAssertEqual(try Baml.primitives.round_trip_string(x: "hi"), "hi")
    }

    func test_round_trip_bool() throws {
        XCTAssertEqual(try Baml.primitives.round_trip_bool(x: false), false)
    }

    func test_round_trip_null() throws {
        XCTAssertEqual(try Baml.primitives.round_trip_null(x: BamlNull()), BamlNull())
    }

    func test_round_trip_uint8_array() throws {
        XCTAssertEqual(
            try Baml.primitives.round_trip_uint8_array(b: Data([0x00, 0x01, 0x02])),
            Data([0x00, 0x01, 0x02])
        )
    }

    // Swift-specific: exercises the async completion-callback path,
    // which the sync-only Python module covers elsewhere via
    // pytest-asyncio suites.
    func test_round_trip_int_async() async throws {
        let result = try await Baml.primitives.round_trip_int_async(x: 7)
        XCTAssertEqual(result, 7)
    }
}
