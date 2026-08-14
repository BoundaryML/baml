// Roundtrip coverage for `Baml.maps` — port of python_pydantic2
// `roundtrip_tests/test_maps.py`.
//
// Like Python, round_trip_enum_keyed_map / round_trip_map_container
// stay unported (enum-keyed outbound map keys don't round-trip yet —
// see the NOTE in test_maps.py).
import XCTest
import Baml

final class TestMaps: XCTestCase {
    func test_maps_round_trip_sentiment() throws {
        XCTAssertEqual(try Baml.maps.round_trip_sentiment(s: .Positive), .Positive)
    }

    func test_maps_round_trip_resume() throws {
        let r = Baml.maps.Resume(name: "n")
        XCTAssertEqual(try Baml.maps.round_trip_resume(r: r), r)
    }

    func test_maps_round_trip_simple_map() throws {
        XCTAssertEqual(
            try Baml.maps.round_trip_simple_map(m: ["a": 1, "b": 2]),
            ["a": 1, "b": 2]
        )
    }

    func test_maps_round_trip_list_valued_map() throws {
        XCTAssertEqual(
            try Baml.maps.round_trip_list_valued_map(m: ["k": [1, 2]]),
            ["k": [1, 2]]
        )
    }
}
