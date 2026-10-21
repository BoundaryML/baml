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
}
