// Generic instance methods over FFI — port of python_pydantic2
// `test_generic.py`. `WrapperMethods<String>` is minted engine-side by
// `make_wrapper_methods`; its methods bind the class TypeVar from the
// receiver.
import XCTest
import Baml
import BamlBridge

final class TestGenericMethods: XCTestCase {
    func test_generic_generic() throws {
        let w = try Baml.generics.make_wrapper_methods(text: "hello")
        // `T | WrapperMarker` with T = String.
        XCTAssertEqual(try w.get_value_or_marker(), .t0("hello"))
    }

    func test_generic_generic_wrapper_get_value() throws {
        let w = try Baml.generics.make_wrapper_methods(text: "hello")
        XCTAssertEqual(try w.get_value(), "hello")
    }
}
