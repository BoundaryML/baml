// Roundtrip coverage for `Baml.optional` — port of python_pydantic2
// `roundtrip_tests/test_optional.py`.
//
import XCTest
import Baml
import BamlBridge

final class TestOptional: XCTestCase {
    func test_optional_round_trip_optional_union() throws {
        XCTAssertEqual(try Baml.optional.round_trip_optional_union(u: .t0(3)), .t0(3))
        XCTAssertEqual(try Baml.optional.round_trip_optional_union(u: .t1("s")), .t1("s"))
        XCTAssertNil(try Baml.optional.round_trip_optional_union(u: nil))
    }

    func test_optional_round_trip_optional_container() throws {
        let c = Baml.optional.OptionalContainer(
            optional_int: nil,
            optional_class: Baml.optional.Resume(name: "x"),
            optional_union: .t1("y")
        )
        XCTAssertEqual(try Baml.optional.round_trip_optional_container(c: c), c)
    }

    func test_optional_round_trip_optional_int() throws {
        XCTAssertEqual(try Baml.optional.round_trip_optional_int(x: 5), 5)
        XCTAssertNil(try Baml.optional.round_trip_optional_int(x: nil))
    }

    func test_optional_round_trip_optional_resume() throws {
        let r = Baml.optional.Resume(name: "ada")
        XCTAssertEqual(try Baml.optional.round_trip_optional_resume(r: r), r)
        XCTAssertNil(try Baml.optional.round_trip_optional_resume(r: nil))
    }

    func test_optional_round_trip_resume() throws {
        let r = Baml.optional.Resume(name: "grace")
        XCTAssertEqual(try Baml.optional.round_trip_resume(r: r), r)
    }
}
