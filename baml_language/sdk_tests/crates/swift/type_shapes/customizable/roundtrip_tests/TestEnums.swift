// Roundtrip coverage for `Baml.enums` — port of python_pydantic2
// `roundtrip_tests/test_enums.py`.
import XCTest
import Baml

final class TestEnums: XCTestCase {
    func test_enums_pick_sentiment() throws {
        XCTAssertEqual(try Baml.enums.pick_sentiment(b: true), .Positive)
        XCTAssertEqual(try Baml.enums.pick_sentiment(b: false), .Negative)
    }

    func test_enums_round_trip_sentiment() throws {
        XCTAssertEqual(try Baml.enums.round_trip_sentiment(s: .Negative), .Negative)
    }
}
