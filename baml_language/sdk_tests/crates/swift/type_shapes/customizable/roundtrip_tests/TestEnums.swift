// Roundtrip coverage for `Baml.enums` — port of python_pydantic2
// `roundtrip_tests/test_enums.py`.
import XCTest
import Baml

final class TestEnums: XCTestCase {
    func test_pick_sentiment() throws {
        XCTAssertEqual(try Baml.enums.pick_sentiment(b: true), .Positive)
        XCTAssertEqual(try Baml.enums.pick_sentiment(b: false), .Negative)
    }

    func test_pick_positive() throws {
        XCTAssertEqual(try Baml.enums.pick_positive(), .Positive)
    }

    func test_round_trip_sentiment() throws {
        XCTAssertEqual(try Baml.enums.round_trip_sentiment(s: .Negative), .Negative)
    }

    func test_round_trip_sentiment_positive() throws {
        // EnumVariant-as-type: the variant tag is dropped during
        // TIR→codegen, so the Swift type is just `Sentiment`.
        XCTAssertEqual(try Baml.enums.round_trip_sentiment_positive(s: .Positive), .Positive)
    }

    func test_round_trip_enums() throws {
        let e = Baml.enums.Enums(bare_enum: .Positive, variant_as_type: .Positive)
        XCTAssertEqual(try Baml.enums.round_trip_enums(e: e), e)
    }
}
