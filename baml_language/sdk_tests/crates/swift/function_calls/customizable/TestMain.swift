// Smoke tests for plain (non-LLM) expression functions — port of
// python_pydantic2 `test_main.py`.
import XCTest
import Baml

final class TestMain: XCTestCase {
    func test_main_hello_world_returns_literal() throws {
        XCTAssertEqual(try Baml.hello_world(), "hello world")
    }

    func test_main_single_required_arg_round_trips() throws {
        // The next step up from the nullary case: one required argument
        // round-trips through the engine unchanged.
        XCTAssertEqual(try Baml.single_required_arg(value: "hi"), "hi")
    }
}
