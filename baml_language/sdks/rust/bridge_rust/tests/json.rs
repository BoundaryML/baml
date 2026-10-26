//! Live-engine tests for `baml.json.json` ↔ `baml_bridge::Json`.
//!
//! Calls go through `runtime::invoke_sync_with_options`, the entry point
//! generated function bindings use.

use std::{collections::HashMap, convert::Infallible, sync::OnceLock};

mod common;

use baml_bridge::{
    ArgumentError, BamlValue, DecodeError, EncodeError, Error, Json, Map,
    baml_value::internal::{__BamlValuePrivate, Encoder, class_ty, union_ty},
    decode, encode,
    host_value::CallableTy,
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
function callback_kind(cb: () -> json) -> string { json_kind(cb()) }
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
    call_with(fqn, move |arguments| {
        for (name, value) in &args {
            arguments.value(name, value, &Json::baml_ty())?;
        }
        Ok(())
    })
}

fn call_with<R: BamlValue>(
    fqn: &str,
    encode: impl FnOnce(&mut encode::Arguments) -> Result<(), ArgumentError>,
) -> Result<R, Error> {
    ensure_runtime();
    runtime::invoke_sync_with_options::<R, Infallible>(
        fqn,
        |arguments| encode(arguments).map(|()| vec![]),
        InvocationOptions::default(),
    )
}

/// The one argument `x`, stated as its own type.
fn one<T: BamlValue>(
    value: &T,
) -> impl FnOnce(&mut encode::Arguments) -> Result<(), ArgumentError> + '_ {
    move |arguments| arguments.value("x", value, &T::baml_ty())
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
    fn to_baml(&self, encoder: &mut Encoder) -> Result<wire::InboundValue, EncodeError> {
        encode::class("user.JsonBox", vec![], encoder, |encoder| {
            Ok(vec![
                ("payload", self.payload.to_baml(encoder)?),
                ("list", self.list.to_baml(encoder)?),
                ("mapping", self.mapping.to_baml(encoder)?),
                ("nullable", self.nullable.to_baml(encoder)?),
            ])
        })
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
    fn to_baml(&self, encoder: &mut Encoder) -> Result<wire::InboundValue, EncodeError> {
        // Each arm encodes as the value filling a slot of its own type: a
        // list states `json[]`, a string implies its type.
        match self {
            Self::JsonList(value) => value.to_baml_in(&<Vec<Json>>::baml_ty(), encoder),
            Self::String(value) => value.to_baml_in(&String::baml_ty(), encoder),
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
    let out: Vec<Json> = call_with("user.rt_json_list", one(&list)).expect("rt_json_list succeeds");
    assert_eq!(out, list);

    let mut map = Map::new();
    map.insert("z".to_string(), json!([1, 2]));
    map.insert("a".to_string(), json!({ "nested": "v" }));
    let out: Map<String, Json> =
        call_with("user.rt_json_map", one(&map)).expect("rt_json_map succeeds");
    assert_eq!(out, map);
    assert_eq!(out.keys().collect::<Vec<_>>(), ["z", "a"]);
}

#[test]
fn optional_json_maps_null_to_none() {
    let some: Option<Json> =
        call_with("user.rt_opt_json", one(&Some(json!({ "k": 1 })))).expect("rt_opt_json succeeds");
    assert_eq!(some, Some(json!({ "k": 1 })));

    for sent in [None, Some(Json::Null)] {
        let out: Option<Json> =
            call_with("user.rt_opt_json", one(&sent)).expect("rt_opt_json succeeds");
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
    let out: JsonBox = call_with("user.rt_box", one(&value)).expect("rt_box succeeds");
    assert_eq!(out, value);

    let empty = JsonBox {
        payload: Json::Null,
        list: Vec::new(),
        mapping: Map::new(),
        nullable: None,
    };
    let out: JsonBox = call_with("user.rt_box", one(&empty)).expect("rt_box succeeds");
    assert_eq!(out, empty);
}

#[test]
fn unions_with_json_container_arms_select_by_arm() {
    for value in [
        JsonListOrString::JsonList(vec![json!("text"), json!({ "a": 1 })]),
        JsonListOrString::JsonList(Vec::new()),
        JsonListOrString::String("text".to_string()),
    ] {
        let out: JsonListOrString =
            call_with("user.rt_list_or_string", one(&value)).expect("rt_list_or_string succeeds");
        assert_eq!(out, value);
    }
}

#[test]
fn json_binds_a_generic_type_parameter() {
    ensure_runtime();
    let value = json!({ "a": [1, "two", null] });
    let out = runtime::invoke_sync_with_options::<Json, Infallible>(
        "user.gid",
        |arguments| {
            arguments.value("x", &value, &Json::baml_ty())?;
            Ok(encode::type_args(vec![("T", Json::baml_ty())]))
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
    let out: Json = call_with(
        "user.parsed",
        one(&"{\"b\": [1, 2.5, \"x\", null], \"a\": {}}".to_string()),
    )
    .expect("parsed succeeds");
    assert_eq!(out, json!({ "b": [1, 2.5, "x", null], "a": {} }));
    let out: Json = call_with("user.int_max", |_| Ok(())).expect("int_max succeeds");
    assert_eq!(out, json!(4_611_686_018_427_387_903_i64));
}

#[test]
fn baml_bigints_decode_when_they_fit_i64_or_u64() {
    let out: Json = call_with("user.bigint_u64_max", |_| Ok(())).expect("bigint_u64_max succeeds");
    assert_eq!(out, json!(u64::MAX));
    let out: Json = call_with(
        "user.parsed",
        one(&"[9223372036854775807, -9223372036854775808]".to_string()),
    )
    .expect("parsed succeeds");
    assert_eq!(out, json!([i64::MAX, i64::MIN]));
    for fqn in ["user.bigint_top_level", "user.bigint_nested"] {
        match call_with::<Json>(fqn, |_| Ok(())) {
            Err(Error::Decode(DecodeError::NotJson {
                got: "an integer outside the i64/u64 range",
            })) => {}
            other => panic!("{fqn}: expected a bigint decode error, got {other:?}"),
        }
    }
}

#[test]
fn baml_nan_inside_json_fails_to_decode() {
    match call_with::<Json>("user.nan_float", |_| Ok(())) {
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
    let out: JsonBox = call_with("user.rt_box", one(&value)).expect("rt_box succeeds");
    assert_eq!(out, value);
}

/// `1` wrapped in `depth` single-element arrays.
fn nested_list(depth: usize) -> Json {
    (0..depth).fold(json!(1), |inner, _| Json::Array(vec![inner]))
}

/// `1` wrapped in `depth` single-entry objects.
fn nested_object(depth: usize) -> Json {
    (0..depth).fold(json!(1), |inner, _| json!({ "k": inner }))
}

/// A list level costs the wire two messages and an object level three, plus
/// the innermost container's statement: as an argument (below
/// `CallFunctionArgs` → `InboundMapEntry`) 48 list levels or 32 object
/// levels cross, and one more is refused before the call is sent, naming
/// the argument.
#[test]
fn json_arguments_are_measured_at_the_wire_boundary() {
    for (value, crosses_as) in [
        (nested_list(48), Some("array")),
        (nested_list(49), None),
        (nested_object(32), Some("object")),
        (nested_object(33), None),
    ] {
        let kind = call::<String>("user.json_kind", vec![("x", value)]);
        match (crosses_as, kind) {
            (Some(expected), Ok(kind)) => assert_eq!(kind, expected),
            (
                None,
                Err(Error::Argument(ArgumentError {
                    name,
                    error: EncodeError::TooDeep { limit: 100 },
                })),
            ) => assert_eq!(name, "x"),
            (expected, other) => panic!("expected {expected:?}, got {other:?}"),
        }
    }
}

/// A callback's result sits one message below the completion: 48 list
/// levels or 32 object levels cross, and one more is the bridge's own
/// failure, which the engine surfaces as a `baml.panics.SdkPanic`. A result
/// far deeper is refused the same way after a bounded descent, never by
/// exhausting the dispatch thread's stack.
#[test]
fn json_callback_results_are_measured_at_the_wire_boundary() {
    for (value, crosses_as) in [
        (nested_list(48), Some("array")),
        (nested_list(49), None),
        (nested_object(32), Some("object")),
        (nested_object(33), None),
        (nested_list(5_000), None),
    ] {
        let kind = call_with::<String>("user.callback_kind", move |arguments| {
            arguments.callable(
                "cb",
                move || value.clone(),
                &[],
                CallableTy::new(vec![], Json::baml_ty(), unknown_ty()),
            )
        });
        match (crosses_as, kind) {
            (Some(expected), Ok(kind)) => assert_eq!(kind, expected),
            (None, Err(Error::Panic { class_name, .. })) => {
                assert_eq!(class_name.as_deref(), Some("baml.panics.SdkPanic"));
            }
            (expected, other) => panic!("expected {expected:?}, got {other:?}"),
        }
    }
}

fn unknown_ty() -> wire::BamlTy {
    baml_bridge::baml_value::internal::leaf_ty(wire::baml_ty::Ty::Unknown(wire::BamlTyUnknown {}))
}
