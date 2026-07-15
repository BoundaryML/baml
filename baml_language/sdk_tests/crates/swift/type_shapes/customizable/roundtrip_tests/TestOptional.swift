// Roundtrip coverage for `Baml.optional` — port of python_pydantic2
// `roundtrip_tests/test_optional.py`.
//
// Not yet ported (arrive with Phase 3 union enums):
// test_round_trip_optional_union and test_round_trip_optional_container
// (`OptionalContainer.optional_union` is `(int | string)?`).
import XCTest
import Baml

final class TestOptional: XCTestCase {
    func test_round_trip_optional_int() throws {
        XCTAssertEqual(try Baml.optional.round_trip_optional_int(x: 5), 5)
        XCTAssertNil(try Baml.optional.round_trip_optional_int(x: nil))
    }

    func test_round_trip_optional_resume() throws {
        let r = Baml.optional.Resume(name: "ada")
        XCTAssertEqual(try Baml.optional.round_trip_optional_resume(r: r), r)
        XCTAssertNil(try Baml.optional.round_trip_optional_resume(r: nil))
    }

    func test_round_trip_resume() throws {
        let r = Baml.optional.Resume(name: "grace")
        XCTAssertEqual(try Baml.optional.round_trip_resume(r: r), r)
    }
}
