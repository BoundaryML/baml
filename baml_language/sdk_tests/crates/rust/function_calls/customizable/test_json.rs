//! Canonical `baml.json.json` through the generated Rust SDK.
//!
//! `sdkgen_rust` projects the recursive stdlib alias onto
//! `baml_bridge::Json` (`serde_json::Value`, the analogue of Go's `any` and
//! Python's native values), so every entry point in `go_json_tests` that
//! touches json alone is emitted. Rust does not carry BAML `bigint` inside
//! json (Go does): a number with no exact BAML `int`/`float` form fails the
//! call before dispatch instead.
//!
//! Inbound maps/lists from the Rust bridge carry no element-type annotation
//! on the wire; the engine must re-annotate them with the `baml.json.json`
//! alias so typed narrowing inside BAML — `match (j) { let m: map<string,
//! json> => ... }`, and therefore `baml.json.path` / `path_or` — treats
//! them exactly like BAML-born `baml.json.parse` values.

use baml_bridge::{
    Json, Map, OptionalArg,
    serde_json::{self, json},
};
use baml_sdk::go_json_tests::{
    JsonAlias, JsonBox, call_json_callback, default_json, json_callback_kind, json_kind,
    json_path_string, json_path_string_or, round_trip_json, round_trip_json_alias,
    round_trip_json_box, round_trip_json_list, round_trip_json_map,
};

/// Every JSON arm the Rust projection carries, nested.
fn canonical_json_fixture() -> Json {
    json!({
        "null": null,
        "bool": true,
        "int": 42,
        "int_bounds": [4_611_686_018_427_387_903_i64, -4_611_686_018_427_387_904_i64],
        "float": 1.5,
        "integral_float": 2.0,
        "string": "value",
        "list": [null, false, -3, 2.25, "nested"],
        "map": { "child": [], "empty": {} },
    })
}

fn narrowing_object() -> Json {
    json!({
        "type": "ok",
        "nested": { "list": [1, { "deep": "found" }] },
    })
}

#[test]
fn test_canonical_json_round_trips_at_top_level_and_through_alias() {
    let want = canonical_json_fixture();
    let got = round_trip_json(want.clone()).unwrap();
    assert_eq!(got, want);
    // Integral floats stay floats, ints stay ints, and object keys keep
    // their order.
    assert!(got["integral_float"].is_f64());
    assert!(got["int"].is_i64());
    assert_eq!(
        got.as_object().unwrap().keys().collect::<Vec<_>>(),
        want.as_object().unwrap().keys().collect::<Vec<_>>()
    );

    let alias: JsonAlias = want.clone();
    assert_eq!(round_trip_json_alias(alias).unwrap(), want);

    for scalar in [json!(null), json!(false), json!(0), json!(-0.25), json!("")] {
        assert_eq!(round_trip_json(scalar.clone()).unwrap(), scalar);
    }
}

#[test]
fn test_canonical_json_composes_through_containers_and_classes() {
    let list = vec![canonical_json_fixture(), json!([]), json!(null)];
    assert_eq!(round_trip_json_list(list.clone()).unwrap(), list);

    let mut mapping = Map::new();
    mapping.insert("z".to_string(), json!({ "nested": [1, 2] }));
    mapping.insert("a".to_string(), json!(null));
    let got = round_trip_json_map(mapping.clone()).unwrap();
    assert_eq!(got, mapping);
    assert_eq!(got.keys().collect::<Vec<_>>(), ["z", "a"]);

    let full = JsonBox {
        payload: canonical_json_fixture(),
        list: vec![json!("a"), json!({ "b": [true] })],
        mapping,
        nullable: Some(json!([1, "two"])),
    };
    assert_eq!(round_trip_json_box(full.clone()).unwrap(), full);

    // `json?` is `Option<Json>`: BAML null decodes as `None`, including a
    // `Some(Json::Null)` the host sent (the same BAML value).
    let empty = JsonBox {
        payload: Json::Null,
        list: Vec::new(),
        mapping: Map::new(),
        nullable: None,
    };
    assert_eq!(round_trip_json_box(empty.clone()).unwrap(), empty);
    let explicit_null = JsonBox {
        nullable: Some(Json::Null),
        ..empty.clone()
    };
    assert_eq!(round_trip_json_box(explicit_null).unwrap(), empty);
}

#[test]
fn test_canonical_json_defaults_and_callbacks() {
    assert_eq!(default_json(OptionalArg::Unset).unwrap(), Json::Null);
    assert_eq!(default_json(json!({ "x": 1 })).unwrap(), json!({ "x": 1 }));

    let result = call_json_callback(
        |value: Json| json!({ "seen": value, "extra": [1.5, null] }),
        json!(["arg"]),
    )
    .unwrap();
    assert_eq!(result, json!({ "seen": ["arg"], "extra": [1.5, null] }));
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

    // `json_path_string` declares `throws baml.json.PathError`, so the
    // missing-field throw surfaces typed like every declared throw; the
    // python port pins the same case as `BamlError` matching
    // "missing field".
    let err = json_path_string(object, ".absent".to_string())
        .expect_err("the missing-field throw must surface to the caller");
    let baml_bridge::Error::Thrown { value, .. } = err else {
        panic!("expected the typed PathError throw, got {err}");
    };
    assert!(value.message.contains("missing field"), "{}", value.message);
}

#[test]
fn test_json_returned_from_host_callback_supports_typed_narrowing() {
    // json returned from a host callback converts on the host-return path
    // (no argument coercion pass); it must narrow identically.
    let result =
        json_callback_kind(|value: Json| json!({ "wrapped": value }), json!("payload")).unwrap();
    assert_eq!(result, "object");
}

#[test]
fn test_canonical_json_rejects_extensions_before_dispatch() {
    // `serde_json::Value` can hold numbers BAML json cannot carry exactly:
    // BAML `int` is 63-bit and bigint is not carried inside json, so these
    // integers have no exact form; they must never round into a float.
    let wide: Json = serde_json::from_str("123456789012345678901234567890").unwrap();
    let huge_float: Json = serde_json::from_str("1e400").unwrap();
    for (value, expected) in [
        (
            json!({ "id": u64::MAX }),
            "at $[\"id\"]: integer 18446744073709551615 is outside the BAML int range",
        ),
        (
            json!([i64::MAX]),
            "at $[0]: integer 9223372036854775807 is outside the BAML int range",
        ),
        (
            wide,
            "at $: integer 123456789012345678901234567890 is outside the BAML int range",
        ),
        (huge_float, "at $: number 1e+400 is not a finite float"),
    ] {
        match round_trip_json(value.clone()) {
            Err(baml_bridge::Error::Sdk(error)) => {
                assert!(error.to_string().contains(expected), "{error}");
            }
            other => panic!("{value}: expected an encode error, got {other:?}"),
        }
    }

    // Nested in a class field, the failure is the same.
    let boxed = JsonBox {
        payload: json!({ "ok": true }),
        list: vec![json!(u64::MAX)],
        mapping: Map::new(),
        nullable: None,
    };
    match round_trip_json_box(boxed) {
        Err(baml_bridge::Error::Sdk(error)) => {
            // The path is relative to the json value itself.
            assert!(
                error
                    .to_string()
                    .contains("at $: integer 18446744073709551615"),
                "{error}"
            );
        }
        other => panic!("expected an encode error, got {other:?}"),
    }
}
