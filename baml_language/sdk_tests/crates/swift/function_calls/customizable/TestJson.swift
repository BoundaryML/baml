// Host-supplied json container typing — port of Go `test_json_test.go`
// (`Test_host_supplied_json_supports_typed_narrowing`) and
// python_pydantic2 `test_json.py`.
//
// The engine converts nothing and refines no statement from context, so
// every list and map the Swift bridge sends inside a json value states its
// own type by the alias: an array node is `json[]` and an object node
// `map<string, json>`. Typed narrowing inside BAML — `match (j) { let m:
// map<string, json> => ... }`, and therefore `baml.json.path` / `path_or`
// — then treats them exactly like BAML-born `baml.json.parse` values.
//
// Adaptation from Go/Python: Swift's json values are the nominal
// `Baml.baml.json.json` enum rather than untyped `any`/dicts, so the
// fixture wraps each node explicitly.
import XCTest
import Baml
import BamlBridge

private typealias Json = Baml.baml.json.json

/// A json value whose lists nest `depth` deep around a `1`.
private func nestedJson(_ depth: Int) -> Json {
    (0..<depth).reduce(Json(1)) { inner, _ in Json([inner]) }
}

/// The host refuses a value nested deeper than the wire carries before
/// sending it, with the error the engine raises for an argument it cannot
/// read, naming the bound.
private func assertRefusedAsTooDeep(
    _ error: any Error,
    file: StaticString = #filePath,
    line: UInt = #line
) {
    guard let error = error as? BamlError else {
        return XCTFail("expected BamlError, got \(error)", file: file, line: line)
    }
    XCTAssertEqual(error.className, "baml.errors.InvalidArgument", file: file, line: line)
    let message = (try? error.value(as: Baml.baml.errors.InvalidArgument.self))?.message ?? ""
    XCTAssertTrue(message.contains("more than 50 deep"), message, file: file, line: line)
}

private func jsonFixtureObject() -> Json {
    Json([
        "type": Json("ok"),
        "nested": Json([
            "list": Json([Json(1), Json(["deep": Json("found")])])
        ]),
    ])
}

final class TestJson: XCTestCase {
    func test_host_supplied_json_supports_typed_narrowing() throws {
        let object = jsonFixtureObject()

        XCTAssertEqual(try Baml.go_json_tests.json_kind(value: object), "object")
        XCTAssertEqual(try Baml.go_json_tests.json_kind(value: Json([Json(1)])), "array")
        XCTAssertEqual(try Baml.go_json_tests.json_kind(value: Json("text")), "string")
        XCTAssertEqual(try Baml.go_json_tests.json_kind(value: Json(3)), "other")
        XCTAssertEqual(
            try Baml.go_json_tests.json_kind(
                value: Json(try BamlBigInt(hexadecimal: "18ee90ff6c373e0ee4e3f0ad2"))
            ),
            "other"
        )

        XCTAssertEqual(
            try Baml.go_json_tests.json_path_string(value: object, selector: ".type"),
            "ok"
        )
        XCTAssertEqual(
            try Baml.go_json_tests.json_path_string(value: object, selector: ".nested.list[1].deep"),
            "found"
        )
        XCTAssertEqual(
            try Baml.go_json_tests.json_path_string_or(
                value: object,
                selector: ".missing",
                default: "fallback"
            ),
            "fallback"
        )

        do {
            _ = try Baml.go_json_tests.json_path_string(value: object, selector: ".absent")
            XCTFail("expected BamlError")
        } catch let error as BamlError {
            let decoded = try error.value(as: Baml.baml.json.PathError.self)
            XCTAssertTrue(
                decoded.message.contains("missing field"),
                "unexpected PathError message: \(decoded.message)"
            )
        }
    }

    func test_json_returned_from_host_callback_supports_typed_narrowing() throws {
        // json returned from a host callback is checked against the
        // callback's declared return, its containers stated the same way;
        // it must narrow identically.
        let result = try Baml.go_json_tests.json_callback_kind(
            callback: { value in Json(["wrapped": value]) },
            value: Json("payload")
        )
        XCTAssertEqual(result, "object")
    }

    func test_host_supplied_empty_and_null_bearing_json_containers_state_the_alias() throws {
        // An empty container has no element to read a type from, and a
        // `nil` element is the alias's own null: both still state `json[]`
        // / `map<string, json>`, never `(json | null)[]` or nothing.
        XCTAssertEqual(try Baml.go_json_tests.json_kind(value: Json([Json?]())), "array")
        XCTAssertEqual(try Baml.go_json_tests.json_kind(value: Json([String: Json?]())), "object")
        XCTAssertEqual(try Baml.go_json_tests.json_kind(value: Json([nil, Json(1)])), "array")
        XCTAssertEqual(try Baml.go_json_tests.json_kind(value: Json(["absent": nil])), "object")
    }

    func test_json_nested_deeper_than_the_wire_carries_is_refused_as_an_argument() throws {
        // Well inside the bound, the value crosses both ways.
        XCTAssertEqual(try Baml.go_json_tests.round_trip_json(value: nestedJson(30)), nestedJson(30))
        // Just past it, and far past it: refused before anything is sent,
        // never by exhausting the stack.
        for depth in [60, 5000] {
            XCTAssertThrowsError(try Baml.go_json_tests.round_trip_json(value: nestedJson(depth))) {
                assertRefusedAsTooDeep($0)
            }
        }
        // The async form encodes on a cooperative thread (512 KiB of stack).
        let refused = expectation(description: "the async call is refused")
        Task {
            do {
                _ = try await Baml.go_json_tests.round_trip_json_async(value: nestedJson(60))
                XCTFail("a value nested 60 deep crossed")
            } catch {
                assertRefusedAsTooDeep(error)
            }
            refused.fulfill()
        }
        wait(for: [refused], timeout: 60)
    }

    func test_json_nested_deeper_than_the_wire_carries_is_refused_as_a_callback_result() throws {
        // A callback runs on a cooperative thread (512 KiB of stack); its
        // result is refused there, and the refusal is what the call raises.
        for depth in [60, 200] {
            XCTAssertThrowsError(
                try Baml.go_json_tests.call_json_callback(
                    callback: { _ in nestedJson(depth) },
                    value: Json(1)
                )
            ) {
                assertRefusedAsTooDeep($0)
            }
        }
    }
}
