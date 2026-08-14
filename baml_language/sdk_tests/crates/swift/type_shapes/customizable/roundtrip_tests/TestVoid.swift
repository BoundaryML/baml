// Roundtrip coverage for `Baml.void` — port of python_pydantic2
// `roundtrip_tests/test_void.py`. `void` return lowers to Swift `Void`.
import XCTest
import Baml

final class TestVoid: XCTestCase {
    func test_void_no_op() throws {
        try Baml.void.no_op()
    }
}
