// Roundtrip coverage for `Baml.aliases` — port of python_pydantic2
// `roundtrip_tests/test_aliases.py`.
//
// The recursive alias `RecList = int | RecList[]` is a nominal enum
// with the BamlUnionN surface under the user's own name (a `typealias`
// can't self-reference).
import XCTest
import Baml

final class TestAliases: XCTestCase {
    func test_aliases_round_trip_rec_list() throws {
        // RecList = int | RecList[]; python values 1 and [1, [2, 3]].
        XCTAssertEqual(try Baml.aliases.round_trip_rec_list(r: .t0(1)), .t0(1))
        let nested: Baml.aliases.RecList = .t1([.t0(1), .t1([.t0(2), .t0(3)])])
        XCTAssertEqual(try Baml.aliases.round_trip_rec_list(r: nested), nested)
    }

    func test_aliases_round_trip_alias_container() throws {
        let c = Baml.aliases.AliasContainer(
            list_field: ["x"],
            rec_field: .t1([.t0(1), .t1([.t0(2)])])
        )
        XCTAssertEqual(try Baml.aliases.round_trip_alias_container(c: c), c)
    }

    func test_aliases_round_trip_string_list() throws {
        let s: Baml.aliases.StringList = ["a", "b"]
        XCTAssertEqual(try Baml.aliases.round_trip_string_list(s: s), ["a", "b"])
    }
}
