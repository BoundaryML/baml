// Roundtrip coverage for `Baml.class_refs` — port of python_pydantic2
// `roundtrip_tests/test_class_refs.py`.
import XCTest
import Baml

final class TestClassRefs: XCTestCase {
    func test_class_refs_make_outer() throws {
        let o = try Baml.class_refs.make_outer(value: 5)
        XCTAssertEqual(o.inner.value, 5)
    }

    func test_class_refs_round_trip_inner() throws {
        let i = Baml.class_refs.Inner(value: 3)
        XCTAssertEqual(try Baml.class_refs.round_trip_inner(i: i), i)
    }

    func test_class_refs_round_trip_outer() throws {
        let o = Baml.class_refs.Outer(inner: Baml.class_refs.Inner(value: 9))
        XCTAssertEqual(try Baml.class_refs.round_trip_outer(o: o), o)
    }
}
