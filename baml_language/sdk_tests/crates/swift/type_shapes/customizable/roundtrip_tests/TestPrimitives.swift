// Roundtrip coverage for `Baml.primitives` — port of
// python_pydantic2 `roundtrip_tests/test_primitives.py`.
//
// Ported 1:1 where the capability exists.
// test_round_trip_float_accepts_int is Python-only (Swift's type
// system forbids passing Int where Double is declared); its class
// sibling test_round_trip_primitives_float_field_accepts_int maps to
// Swift's literal inference (an integer literal in Double position IS
// a Double — the coercion happens in the compiler, not pydantic).
import XCTest
import Foundation
import Baml
import BamlBridge

final class TestPrimitives: XCTestCase {
    func test_primitives_return_int() throws {
        XCTAssertEqual(try Baml.primitives.return_int(), 42)
    }

    func test_primitives_return_float() throws {
        XCTAssertEqual(try Baml.primitives.return_float(), 3.14)
    }

    func test_primitives_return_string() throws {
        XCTAssertEqual(try Baml.primitives.return_string(), "hello")
    }

    func test_primitives_return_bool() throws {
        XCTAssertEqual(try Baml.primitives.return_bool(), true)
    }

    func test_primitives_return_null() throws {
        XCTAssertEqual(try Baml.primitives.return_null(), BamlNull())
    }

    func test_primitives_round_trip_int() throws {
        XCTAssertEqual(try Baml.primitives.round_trip_int(x: 7), 7)
    }

    func test_primitives_round_trip_float() throws {
        XCTAssertEqual(try Baml.primitives.round_trip_float(x: 2.5), 2.5)
    }

    func test_primitives_round_trip_string() throws {
        XCTAssertEqual(try Baml.primitives.round_trip_string(x: "hi"), "hi")
    }

    func test_primitives_round_trip_bool() throws {
        XCTAssertEqual(try Baml.primitives.round_trip_bool(x: false), false)
    }

    func test_primitives_round_trip_null() throws {
        XCTAssertEqual(try Baml.primitives.round_trip_null(x: BamlNull()), BamlNull())
    }

    func test_primitives_round_trip_uint8_array() throws {
        XCTAssertEqual(
            try Baml.primitives.round_trip_uint8_array(b: Data([0x00, 0x01, 0x02])),
            Data([0x00, 0x01, 0x02])
        )
    }

    func test_primitives_round_trip_primitives() throws {
        let p = Baml.primitives.Primitives(
            int_field: 1,
            float_field: 1.5,
            string_field: "s",
            bool_field: true,
            null_field: BamlNull(),
            uint8array_field: Data([0x61, 0x62])
        )
        XCTAssertEqual(try Baml.primitives.round_trip_primitives(p: p), p)
    }

    func test_primitives_round_trip_primitives_float_field_accepts_int() throws {
        // Python pins pydantic's int→float coercion at construction;
        // Swift's equivalent contract is literal inference — `2` in a
        // Double position is a Double. The wire must carry a float and
        // hand back 2.0.
        let p = Baml.primitives.Primitives(
            int_field: 1,
            float_field: 2,
            string_field: "s",
            bool_field: true,
            null_field: BamlNull(),
            uint8array_field: Data([0x61, 0x62])
        )
        let result = try Baml.primitives.round_trip_primitives(p: p)
        XCTAssertEqual(result.float_field, 2.0)
    }

    // Swift-specific: exercises the async completion-callback path,
    // which the sync-only Python module covers elsewhere via
    // pytest-asyncio suites.
    func test_primitives_round_trip_int_async() async throws {
        let result = try await Baml.primitives.round_trip_int_async(x: 7)
        XCTAssertEqual(result, 7)
    }
}
