// BamlError / BamlPanic delivery contract — port of python_pydantic2
// `test_errors.py`.
//
// Contract: a thrown BAML value surfaces as a `BamlError` (or
// `BamlPanic`) wrapper carrying the decoded value — Swift spells
// Python's `exc.value` + `isinstance` as `err.value(as: T.self)` —
// plus `className` (FQN, peeled through union-throws wrappers) and
// `bamlTrace`.
//
// Not ported (Python-specific): the kwargs-injection InvalidArgument
// case (compile error in Swift), asyncio cancellation mapping (Phase
// 5), traceback splicing, and the subprocess exit-code cases
// (`baml.sys.exit` hard-exits the process; covered manually).
import XCTest
import Baml
import BamlBridge

private let badJSON = "{not valid json"

final class TestErrors: XCTestCase {
    private func expectBamlError(_ body: () throws -> Void) -> BamlError? {
        do {
            try body()
            XCTFail("expected BamlError")
            return nil
        } catch let error as BamlError {
            return error
        } catch {
            XCTFail("expected BamlError, got \(error)")
            return nil
        }
    }

    func test_errors_stdlib_error_surfaces_as_baml_error() throws {
        let err = expectBamlError { _ = try Baml.throws_test.ParseJson(s: badJSON) }
        // Typed decode succeeding IS the isinstance assertion.
        _ = try XCTUnwrap(err).value(as: Baml.baml.json.ParseError.self)
    }

    func test_errors_user_throw_surfaces_declared_instance() throws {
        let err = expectBamlError { _ = try Baml.throws_test.ThrowMyError() }
        _ = try XCTUnwrap(err).value(as: Baml.throws_test.MyError.self)
    }

    func test_errors_union_throws_preserves_class_name() throws {
        let single = try XCTUnwrap(expectBamlError { _ = try Baml.raises_test.Reparse(s: "x") })
        let union = try XCTUnwrap(expectBamlError { _ = try Baml.raises_test.LoadDoc(path: "x") })

        XCTAssertEqual(single.className, "user.raises_test.ParseError")
        XCTAssertEqual(union.className, single.className)
        _ = try union.value(as: Baml.raises_test.ParseError.self)
    }

    func test_errors_user_panic_surfaces_as_baml_panic() throws {
        do {
            try Baml.throws_test.DoPanic(message: "user-initiated boom")
            XCTFail("expected BamlPanic")
        } catch let panic as BamlPanic {
            _ = try panic.value(as: Baml.baml.panics.UserPanic.self)
        }
    }

    func test_errors_str_is_non_empty() throws {
        let err = expectBamlError { _ = try Baml.throws_test.ParseJson(s: badJSON) }
        XCTAssertFalse(try XCTUnwrap(err).message.isEmpty)
    }

    func test_errors_baml_error_carries_baml_trace() throws {
        let err = try XCTUnwrap(expectBamlError { _ = try Baml.throws_test.ThrowMyError() })
        XCTAssertFalse(err.bamlTrace.isEmpty)
        let last = try XCTUnwrap(err.bamlTrace.last)
        // Last frame: `File "<...types.baml>", line N, in user.throws_test.ThrowMyError`
        XCTAssertTrue(last.contains("types.baml"), "unexpected trace frame: \(last)")
        XCTAssertTrue(last.contains("user.throws_test.ThrowMyError"), "unexpected trace frame: \(last)")
    }
}
