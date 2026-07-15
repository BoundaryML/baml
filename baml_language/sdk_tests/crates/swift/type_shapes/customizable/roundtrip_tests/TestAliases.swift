// Roundtrip coverage for `Baml.aliases` — port of python_pydantic2
// `roundtrip_tests/test_aliases.py`.
//
// Not yet ported (arrive with Phase 3 union enums): the recursive
// alias `RecList = int | RecList[]` and everything touching it
// (test_round_trip_rec_list, test_round_trip_alias_container — Swift
// `typealias` can't be recursive; the union representation carries it).
import XCTest
import Baml

final class TestAliases: XCTestCase {
    func test_round_trip_string_list() throws {
        let s: Baml.aliases.StringList = ["a", "b"]
        XCTAssertEqual(try Baml.aliases.round_trip_string_list(s: s), ["a", "b"])
    }
}
