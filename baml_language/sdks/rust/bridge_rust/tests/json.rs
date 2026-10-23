//! Live-engine tests for `baml.json.json` ↔ `baml_bridge::Json`.
//!
//! Calls go through `runtime::invoke_sync_with_options`, the entry point
//! generated function bindings use.

use std::{collections::HashMap, convert::Infallible, sync::OnceLock};

mod common;

use baml_bridge::{
    BamlValue, DecodeError, Error, Json, Map,
    baml_value::internal::{__BamlValuePrivate, class_ty, union_ty},
    decode, encode,
    invocation::InvocationOptions,
    runtime,
    serde_json::json,
    wire,
};

const BAML_SRC: &str = r#"
class JsonBox {
  payload json
  list json[]
  mapping map<string, json>
  nullable json?
}

function rt_json(x: json) -> json { x }
function rt_json_list(x: json[]) -> json[] { x }
function rt_json_map(x: map<string, json>) -> map<string, json> { x }
function rt_opt_json(x: json?) -> json? { x }
function rt_box(x: JsonBox) -> JsonBox { x }
function rt_list_or_string(x: json[] | string) -> json[] | string { x }
function gid<T>(x: T) -> T { x }

// Host-supplied containers must narrow like BAML-born json.
function json_kind(x: json) -> string {
  match (x) {
    let m: map<string, json> => "object",
    let a: json[] => "array",
    let s: string => "string",
    let i: int => "int",
    let b: bigint => "bigint",
    let f: float => "float",
    _ => "other",
  }
}

function parsed(x: string) -> json throws baml.json.ParseError { baml.json.parse(x) }
function int_max() -> json { 4611686018427387903 }
function bigint_u64_max() -> json { (2n).pow(64n) - 1n }
function bigint_top_level() -> json { (2n).pow(70n) }
function bigint_nested() -> json { { "ok": [1, 2], "big": [(2n).pow(70n)] } }
function nan_float() -> json { float.nan() }
"#;

fn ensure_runtime() {
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        common::locate_dev_engine();
        let files = HashMap::from([("main.baml".to_string(), BAML_SRC.to_string())]);
        // The vfs rejects trailing slashes (macOS's temp_dir ends in one).
        let root = std::env::temp_dir();
        let root = root.to_string_lossy();
        let root = root.trim_end_matches('/');
        runtime::initialize_from_files(root, &files).expect("runtime initializes");
    });
}

/// Call `fqn` the way a generated binding does: arguments encoded inside
/// the bridge's call entry point.
fn call<R: BamlValue>(fqn: &str, args: Vec<(&'static str, Json)>) -> Result<R, Error> {
    call_with(fqn, move || {
        args.iter()
            .map(|(name, value)| (*name, Some(value.to_baml())))
            .collect()
    })
}

fn call_with<R: BamlValue>(
    fqn: &str,
    kwargs: impl FnOnce() -> Vec<(&'static str, Option<wire::InboundValue>)>,
) -> Result<R, Error> {
    ensure_runtime();
    runtime::invoke_sync_with_options::<R, Infallible>(
        fqn,
        || (encode::kwargs(kwargs()), vec![]),
        InvocationOptions::default(),
    )
}

fn rt(value: &Json) -> Json {
    call("user.rt_json", vec![("x", value.clone())]).expect("rt_json succeeds")
}

#[derive(Debug, Clone, PartialEq)]
struct JsonBox {
    payload: Json,
    list: Vec<Json>,
    mapping: Map<String, Json>,
    nullable: Option<Json>,
}

impl __BamlValuePrivate for JsonBox {
    fn to_baml(&self) -> wire::InboundValue {
        encode::class(
            "user.JsonBox",
            vec![],
            vec![
                ("payload", self.payload.to_baml()),
                ("list", self.list.to_baml()),
                ("mapping", self.mapping.to_baml()),
                ("nullable", self.nullable.to_baml()),
            ],
        )
    }

    fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        let mut fields = decode::ClassFields::new(v, "user.JsonBox")?;
        Ok(JsonBox {
            payload: fields.take("payload")?,
            list: fields.take("list")?,
            mapping: fields.take("mapping")?,
            nullable: fields.take("nullable")?,
        })
    }

    fn baml_ty() -> wire::BamlTy {
        class_ty("user.JsonBox", vec![])
    }
}

/// `json[] | string` in the shape the generator emits for a union: arms
/// that contain json (but are not bare json) stay discriminable.
#[derive(Debug, Clone, PartialEq)]
enum JsonListOrString {
    JsonList(Vec<Json>),
    String(String),
}

impl __BamlValuePrivate for JsonListOrString {
    fn to_baml(&self) -> wire::InboundValue {
        // Each arm encodes as the value filling a slot of its own type: a
        // list states `json[]`, a string implies its type.
        match self {
            Self::JsonList(value) => value.to_baml_in(&<Vec<Json>>::baml_ty()),
            Self::String(value) => value.to_baml_in(&String::baml_ty()),
        }
    }

    fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        let (selected_type, v) = decode::union_selected(v)?;
        if let Some(selected_type) = selected_type {
            if decode::selected_type_matches(&<Vec<Json>>::baml_ty(), &selected_type) {
                return <Vec<Json>>::from_baml(v).map(Self::JsonList);
            }
            if decode::selected_type_matches(&String::baml_ty(), &selected_type) {
                return String::from_baml(v).map(Self::String);
            }
            return Err(decode::no_union_arm("JsonListOrString", &v));
        }
        let v = decode::unwrap(v);
        if let Ok(value) = <Vec<Json>>::from_baml(v.clone()) {
            return Ok(Self::JsonList(value));
        }
        if let Ok(value) = String::from_baml(v.clone()) {
            return Ok(Self::String(value));
        }
        Err(decode::no_union_arm("JsonListOrString", &v))
    }

    fn baml_ty() -> wire::BamlTy {
        union_ty(vec![<Vec<Json>>::baml_ty(), String::baml_ty()])
    }
}

#[test]
fn nested_values_round_trip_in_map_order() {
    let value = json!({
        "zeta": null,
        "alpha": true,
        "int": -3,
        "float": 2.25,
        "integral_float": 2.0,
        "string": "text",
        "list": [null, false, 1, 0.5, "s", [], {}],
        "object": { "b": [{ "deep": ["found"] }], "a": {} },
    });
    let out = rt(&value);
    assert_eq!(out, value);
    let keys = |value: &Json| {
        value
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(keys(&out), keys(&value));
    // An integral float comes back a float, not an int.
    assert!(out["integral_float"].is_f64());
    assert!(out["int"].is_i64());
}

#[test]
fn scalars_round_trip_at_the_top_level() {
    for value in [
        json!(null),
        json!(false),
        json!(0),
        json!(4_611_686_018_427_387_903_i64),
        json!(-4_611_686_018_427_387_904_i64),
        json!(i64::MAX),
        json!(i64::MIN),
        json!(u64::MAX),
        json!(-0.5),
        json!(1e300),
        json!(""),
    ] {
        assert_eq!(rt(&value), value);
    }
}

#[test]
fn containers_of_json_round_trip() {
    let list = vec![json!(1), json!({ "a": [true] }), json!(null), json!([])];
    let out: Vec<Json> = call_with("user.rt_json_list", || vec![("x", Some(list.to_baml()))])
        .expect("rt_json_list succeeds");
    assert_eq!(out, list);

    let mut map = Map::new();
    map.insert("z".to_string(), json!([1, 2]));
    map.insert("a".to_string(), json!({ "nested": "v" }));
    let out: Map<String, Json> = call_with("user.rt_json_map", || vec![("x", Some(map.to_baml()))])
        .expect("rt_json_map succeeds");
    assert_eq!(out, map);
    assert_eq!(out.keys().collect::<Vec<_>>(), ["z", "a"]);
}

#[test]
fn optional_json_maps_null_to_none() {
    let some: Option<Json> = call_with("user.rt_opt_json", || {
        vec![("x", Some(Some(json!({ "k": 1 })).to_baml()))]
    })
    .expect("rt_opt_json succeeds");
    assert_eq!(some, Some(json!({ "k": 1 })));

    for sent in [None, Some(Json::Null)] {
        let out: Option<Json> = call_with("user.rt_opt_json", || vec![("x", Some(sent.to_baml()))])
            .expect("rt_opt_json succeeds");
        assert_eq!(out, None, "{sent:?}");
    }
}

#[test]
fn class_fields_of_json_round_trip() {
    let value = JsonBox {
        payload: json!({ "event": "created", "ids": [1, 2] }),
        list: vec![json!("a"), json!(null)],
        mapping: Map::from([("k".to_string(), json!(1.5))]),
        nullable: Some(json!([false])),
    };
    let out: JsonBox =
        call_with("user.rt_box", || vec![("x", Some(value.to_baml()))]).expect("rt_box succeeds");
    assert_eq!(out, value);

    let empty = JsonBox {
        payload: Json::Null,
        list: Vec::new(),
        mapping: Map::new(),
        nullable: None,
    };
    let out: JsonBox =
        call_with("user.rt_box", || vec![("x", Some(empty.to_baml()))]).expect("rt_box succeeds");
    assert_eq!(out, empty);
}

#[test]
fn unions_with_json_container_arms_select_by_arm() {
    for value in [
        JsonListOrString::JsonList(vec![json!("text"), json!({ "a": 1 })]),
        JsonListOrString::JsonList(Vec::new()),
        JsonListOrString::String("text".to_string()),
    ] {
        let out: JsonListOrString = call_with("user.rt_list_or_string", || {
            vec![("x", Some(value.to_baml()))]
        })
        .expect("rt_list_or_string succeeds");
        assert_eq!(out, value);
    }
}

#[test]
fn json_binds_a_generic_type_parameter() {
    ensure_runtime();
    let value = json!({ "a": [1, "two", null] });
    let out = runtime::invoke_sync_with_options::<Json, Infallible>(
        "user.gid",
        || {
            (
                encode::kwargs(vec![("x", Some(value.to_baml()))]),
                encode::type_args(vec![("T", Json::baml_ty())]),
            )
        },
        InvocationOptions::default(),
    )
    .expect("gid<json> succeeds");
    assert_eq!(out, value);
}

#[test]
fn host_supplied_json_narrows_like_baml_json() {
    for (value, kind) in [
        (json!({ "a": 1 }), "object"),
        (json!([1]), "array"),
        (json!("s"), "string"),
        (json!(3), "int"),
        // BAML `int` is 63-bit: wider integers arrive as `bigint`, as
        // `baml.json.parse` reads them.
        (json!(4_611_686_018_427_387_904_i64), "bigint"),
        (json!(u64::MAX), "bigint"),
        (json!(0.5), "float"),
        (json!(null), "other"),
    ] {
        let got: String = call("user.json_kind", vec![("x", value.clone())]).unwrap();
        assert_eq!(got, kind, "{value}");
    }
}

#[test]
fn baml_parsed_json_decodes() {
    let out: Json = call_with("user.parsed", || {
        vec![(
            "x",
            Some(
                "{\"b\": [1, 2.5, \"x\", null], \"a\": {}}"
                    .to_string()
                    .to_baml(),
            ),
        )]
    })
    .expect("parsed succeeds");
    assert_eq!(out, json!({ "b": [1, 2.5, "x", null], "a": {} }));
    let out: Json = call_with("user.int_max", Vec::new).expect("int_max succeeds");
    assert_eq!(out, json!(4_611_686_018_427_387_903_i64));
}

#[test]
fn baml_bigints_decode_when_they_fit_i64_or_u64() {
    let out: Json = call_with("user.bigint_u64_max", Vec::new).expect("bigint_u64_max succeeds");
    assert_eq!(out, json!(u64::MAX));
    let out: Json = call_with("user.parsed", || {
        vec![(
            "x",
            Some(
                "[9223372036854775807, -9223372036854775808]"
                    .to_string()
                    .to_baml(),
            ),
        )]
    })
    .expect("parsed succeeds");
    assert_eq!(out, json!([i64::MAX, i64::MIN]));
    for fqn in ["user.bigint_top_level", "user.bigint_nested"] {
        match call_with::<Json>(fqn, Vec::new) {
            Err(Error::Decode(DecodeError::NotJson {
                got: "an integer outside the i64/u64 range",
            })) => {}
            other => panic!("{fqn}: expected a bigint decode error, got {other:?}"),
        }
    }
}

#[test]
fn baml_nan_inside_json_fails_to_decode() {
    match call_with::<Json>("user.nan_float", Vec::new) {
        Err(Error::Decode(DecodeError::NotJson {
            got: "a non-finite float",
        })) => {}
        other => panic!("expected a non-finite float decode error, got {other:?}"),
    }
}

#[test]
fn full_width_integers_round_trip_inside_classes() {
    let value = JsonBox {
        payload: json!({ "ids": [u64::MAX, i64::MIN] }),
        list: vec![json!(i64::MAX)],
        mapping: Map::from([("k".to_string(), json!(4_611_686_018_427_387_904_i64))]),
        nullable: Some(json!(u64::MAX)),
    };
    let out: JsonBox =
        call_with("user.rt_box", || vec![("x", Some(value.to_baml()))]).expect("rt_box succeeds");
    assert_eq!(out, value);
}
