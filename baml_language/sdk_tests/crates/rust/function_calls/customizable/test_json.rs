//! Host-supplied json must materialize with `json` container typing.
//!
//! Inbound maps/lists from the Rust bridge carry no element-type annotation
//! on the wire; the engine must re-annotate them with the `baml.json.json`
//! alias so typed narrowing inside BAML — `match (j) { let m: map<string,
//! json> => ... }`, and therefore `baml.json.path` / `path_or` — treats
//! them exactly like BAML-born `baml.json.parse` values.
//!
//! This module stays gated off until `sdkgen_rust` projects the canonical
//! `baml.json.json` recursive alias: today every symbol touching it is
//! skipped as unsupported, so `baml_sdk::go_json_tests` emits no entry
//! points (Go projects the alias as `any`; the Rust projection is not
//! pinned yet). Once it compiles, each case below asserts the same
//! contract the Go and Python ports pin.

// SPECULATIVE: `serde_json::Value` as the Rust projection of canonical
// JSON (the analogue of Go's `any` and Python's native values) is a
// provisional choice until the generator pins one, as is the presence of
// `serde_json` in the generated crate's dev-dependencies.
use baml_sdk::go_json_tests::{
    json_callback_kind, json_kind, json_path_string, json_path_string_or,
};
use serde_json::json;

fn narrowing_object() -> serde_json::Value {
    json!({
        "type": "ok",
        "nested": { "list": [1, { "deep": "found" }] },
    })
}

#[test]
fn test_host_supplied_json_supports_typed_narrowing() {
    let object = narrowing_object();

    assert_eq!(json_kind(object.clone()).unwrap(), "object");
    assert_eq!(json_kind(json!([1])).unwrap(), "array");
    assert_eq!(json_kind(json!("text")).unwrap(), "string");
    assert_eq!(json_kind(json!(3)).unwrap(), "other");

    assert_eq!(
        json_path_string(object.clone(), ".type".to_string()).unwrap(),
        "ok"
    );
    assert_eq!(
        json_path_string(object.clone(), ".nested.list[1].deep".to_string()).unwrap(),
        "found"
    );
    assert_eq!(
        json_path_string_or(
            object.clone(),
            ".missing".to_string(),
            "fallback".to_string()
        )
        .unwrap(),
        "fallback"
    );

    // `json_path_string` declares `throws baml.json.JsonPathError`, so the
    // missing-field throw surfaces typed like every declared throw; the
    // python port pins the same case as `BamlError` matching
    // "missing field".
    let err = json_path_string(object, ".absent".to_string())
        .expect_err("the missing-field throw must surface to the caller");
    let baml_bridge::Error::Thrown { value, .. } = err else {
        panic!("expected the typed JsonPathError throw, got {err}");
    };
    assert!(value.message.contains("missing field"), "{}", value.message);
}

#[test]
fn test_json_returned_from_host_callback_supports_typed_narrowing() {
    // json returned from a host callback converts on the host-return path
    // (no argument coercion pass); it must narrow identically.
    let result = json_callback_kind(
        |value: serde_json::Value| json!({ "wrapped": value }),
        json!("payload"),
    )
    .unwrap();
    assert_eq!(result, "object");
}
