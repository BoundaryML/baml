// Roundtrip coverage for `Baml.lists` — port of python_pydantic2
// `roundtrip_tests/test_lists.py`.
//
import XCTest
import Baml
import BamlBridge

final class TestLists: XCTestCase {
    func test_lists_round_trip_union_list() throws {
        // `(int | string)[]` → `[BamlUnion2<Int, String>]`.
        let xs: [BamlUnion2<Int, String>] = [.t0(1), .t1("two"), .t0(3)]
        XCTAssertEqual(try Baml.lists.round_trip_union_list(xs: xs), xs)
    }

    func test_lists_round_trip_list_container() throws {
        let c = Baml.lists.ListContainer(
            ints: [1, 2],
            optional_strings: [nil, "z"],
            union_list: [.t0(1), .t1("x")]
        )
        XCTAssertEqual(try Baml.lists.round_trip_list_container(c: c), c)
    }

    func test_lists_round_trip_ints() throws {
        XCTAssertEqual(try Baml.lists.round_trip_ints(xs: [1, 2, 3]), [1, 2, 3])
    }

    func test_lists_round_trip_empty_list() throws {
        // Mirrors Python's Bug A regression: an empty list must encode
        // as a present-but-empty list_value, not an unset oneof (which
        // the engine reads as null).
        XCTAssertEqual(try Baml.lists.round_trip_ints(xs: []), [])
    }

    func test_lists_round_trip_optional_strings() throws {
        XCTAssertEqual(
            try Baml.lists.round_trip_optional_strings(xs: ["a", nil, "b"]),
            ["a", nil, "b"]
        )
    }
}
