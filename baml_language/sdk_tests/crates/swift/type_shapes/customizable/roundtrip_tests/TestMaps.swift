// Roundtrip coverage for `Baml.maps` — port of python_pydantic2
// `roundtrip_tests/test_maps.py`.
//
// Not yet ported (arrive with Phase 2): test_round_trip_sentiment
// (enums), test_round_trip_resume (classes).
import XCTest
import Baml

final class TestMaps: XCTestCase {
    func test_round_trip_simple_map() throws {
        XCTAssertEqual(
            try Baml.maps.round_trip_simple_map(m: ["a": 1, "b": 2]),
            ["a": 1, "b": 2]
        )
    }

    func test_round_trip_list_valued_map() throws {
        XCTAssertEqual(
            try Baml.maps.round_trip_list_valued_map(m: ["k": [1, 2]]),
            ["k": [1, 2]]
        )
    }
}
