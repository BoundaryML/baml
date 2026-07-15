// Roundtrip coverage for `Baml.lists` — port of python_pydantic2
// `roundtrip_tests/test_lists.py`.
//
// Not yet ported (arrive with their phases): test_round_trip_union_list
// (generated union enums, Phase 3), test_round_trip_list_container
// (classes, Phase 2).
import XCTest
import Baml

final class TestLists: XCTestCase {
    func test_round_trip_ints() throws {
        XCTAssertEqual(try Baml.lists.round_trip_ints(xs: [1, 2, 3]), [1, 2, 3])
    }

    func test_round_trip_empty_list() throws {
        // Mirrors Python's Bug A regression: an empty list must encode
        // as a present-but-empty list_value, not an unset oneof (which
        // the engine reads as null).
        XCTAssertEqual(try Baml.lists.round_trip_ints(xs: []), [])
    }

    func test_round_trip_optional_strings() throws {
        XCTAssertEqual(
            try Baml.lists.round_trip_optional_strings(xs: ["a", nil, "b"]),
            ["a", nil, "b"]
        )
    }
}
