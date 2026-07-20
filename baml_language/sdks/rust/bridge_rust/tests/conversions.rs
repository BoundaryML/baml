//! Live-engine tests for class / enum / map conversions.
//!
//! The `BamlValue` impls here are written by hand in exactly the shape the
//! Rust SDK generator emits — this file is the executable spec for the
//! emitted impl blocks, proven against a real engine before the generator
//! produces them.

use std::{collections::HashMap, convert::Infallible, sync::OnceLock};

mod common;

use baml_bridge::{
    BamlValue, DecodeError, Error, Map,
    baml_value::internal::{__BamlValuePrivate, class_ty, enum_ty, literal_string_ty, union_ty},
    decode, encode, runtime, wire,
};

const BAML_SRC: &str = r#"
enum Color {
  Red,
  Green,
}

class Point {
  x int
  y int
  tag string?
}

class Other {
  x int
}

class TreeNode {
  value int
  next TreeNode?
}

class Wrapper<T> {
  inner T
}

class Mixed<T> {
  choice T | string | null
}

function rt_color(c: Color) -> Color { c }
function rt_point(p: Point) -> Point { p }
function make_point(x: int, y: int) -> Point { Point { x: x, y: y, tag: null } }
function make_other(x: int) -> Other { Other { x: x } }
function point_tag(p: Point) -> string? { p.tag }
function rt_map(m: map<string, int>) -> map<string, int> { m }
function rt_tree(t: TreeNode) -> TreeNode { t }
function rt_int_or_string(u: int | string) -> int | string { u }
function rt_point_or_string(u: Point | string) -> Point | string { u }
function rt_opt_union(u: int | string | null) -> int | string | null { u }
function rt_status(s: "draft" | "sent") -> "draft" | "sent" { s }
function rt_wrapper<T>(w: Wrapper<T>) -> Wrapper<T> { w }
function rt_mixed<T>(m: Mixed<T>) -> Mixed<T> { m }
function hc_call(callback: (int) -> string, x: int) -> string {
    callback(x)
}
function hc_optionals(callback: (x: int, y?: int, z?: int) -> int, x: int) -> int[] {
    [callback(x), callback(x, y = 2), callback(x, z = 3), callback(x, y = 2, z = 3)]
}
function hc_typed_throws(
    callback: (int) -> string throws Point,
    x: int,
) -> string throws Point {
    callback(x)
}
function hc_catches(callback: (int) -> string throws baml.errors.HostCallable, x: int) -> string {
    callback(x) catch (e) {
        _ => "caught:" + e.class_name
    }
}
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

fn call<R: BamlValue>(fqn: &str, kwargs: Vec<(&str, wire::InboundValue)>) -> R {
    ensure_runtime();
    let kwargs = kwargs.into_iter().map(|(k, v)| (k, Some(v))).collect();
    runtime::invoke_sync::<R, Infallible>(fqn, encode::kwargs(kwargs), vec![])
        .unwrap_or_else(|e| panic!("{fqn} failed: {e}"))
}

// ---------------------------------------------------------------------------
// Hand-written impls in the exact shape the generator emits.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Color {
    Red,
    Green,
}

impl __BamlValuePrivate for Color {
    fn to_baml(&self) -> wire::InboundValue {
        encode::enum_value(
            "user.Color",
            match self {
                Color::Red => "Red",
                Color::Green => "Green",
            },
        )
    }

    fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        match decode::enum_variant(v, "user.Color")?.as_str() {
            "Red" => Ok(Color::Red),
            "Green" => Ok(Color::Green),
            other => Err(DecodeError::UnknownEnumVariant {
                enum_fqn: "user.Color",
                got: other.to_string(),
            }),
        }
    }

    fn baml_ty() -> wire::BamlTy {
        enum_ty("user.Color")
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Point {
    x: i64,
    y: i64,
    tag: Option<String>,
}

impl __BamlValuePrivate for Point {
    fn to_baml(&self) -> wire::InboundValue {
        encode::class(
            "user.Point",
            vec![],
            vec![
                ("x", self.x.to_baml()),
                ("y", self.y.to_baml()),
                ("tag", self.tag.to_baml()),
            ],
        )
    }

    fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        let mut fields = decode::ClassFields::new(v, "user.Point")?;
        Ok(Point {
            x: fields.take("x")?,
            y: fields.take("y")?,
            tag: fields.take("tag")?,
        })
    }

    fn baml_ty() -> wire::BamlTy {
        class_ty("user.Point", vec![])
    }
}

#[derive(Debug, Clone, PartialEq)]
struct TreeNode {
    value: i64,
    next: Option<Box<TreeNode>>,
}

impl __BamlValuePrivate for TreeNode {
    fn to_baml(&self) -> wire::InboundValue {
        encode::class(
            "user.TreeNode",
            vec![],
            vec![
                ("value", self.value.to_baml()),
                ("next", self.next.to_baml()),
            ],
        )
    }

    fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        let mut fields = decode::ClassFields::new(v, "user.TreeNode")?;
        Ok(TreeNode {
            value: fields.take("value")?,
            next: fields.take("next")?,
        })
    }

    fn baml_ty() -> wire::BamlTy {
        class_ty("user.TreeNode", vec![])
    }
}

/// Generic class shape: a `<T: BamlValue>` struct whose instance carries
/// its concrete type argument on the wire (`class_ty.type_args`), exactly
/// as the Rust SDK generator emits it.
#[derive(Debug, Clone, PartialEq)]
struct Wrapper<T: BamlValue> {
    inner: T,
}

impl<T: BamlValue> __BamlValuePrivate for Wrapper<T> {
    fn to_baml(&self) -> wire::InboundValue {
        encode::class(
            "user.Wrapper",
            vec![T::baml_ty()],
            vec![("inner", self.inner.to_baml())],
        )
    }

    fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        let mut fields = decode::ClassFields::new(v, "user.Wrapper")?;
        Ok(Wrapper {
            inner: fields.take("inner")?,
        })
    }

    fn baml_ty() -> wire::BamlTy {
        class_ty("user.Wrapper", vec![T::baml_ty()])
    }
}

/// The wire `BamlTy` for a concrete `T` — a generated SDK reaches it via
/// this same private trait.
fn ty_of<T: BamlValue>() -> wire::BamlTy {
    <T as __BamlValuePrivate>::baml_ty()
}

/// Generic union enum shape (`T | string` → `TOrString<T>`): the `TypeVar`
/// arm holds a bare `T` and carries no `From` (a blanket `From<T>` would
/// overlap the concrete arm's `From<String>`); the concrete arm keeps its
/// `From`.
#[derive(Debug, Clone, PartialEq)]
enum TOrString<T: BamlValue> {
    T(T),
    String(String),
}

impl<T: BamlValue> From<String> for TOrString<T> {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl<T: BamlValue> __BamlValuePrivate for TOrString<T> {
    fn to_baml(&self) -> wire::InboundValue {
        match self {
            Self::T(value) => value.to_baml(),
            Self::String(value) => value.to_baml(),
        }
    }

    fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        let v = decode::unwrap(v);
        if let Ok(value) = T::from_baml(v.clone()) {
            return Ok(Self::T(value));
        }
        if let Ok(value) = String::from_baml(v.clone()) {
            return Ok(Self::String(value));
        }
        Err(decode::no_union_arm("TOrString", &v))
    }

    fn baml_ty() -> wire::BamlTy {
        union_ty(vec![T::baml_ty(), String::baml_ty()])
    }
}

/// A generic class carrying a generic-union field (`Mixed<T> { choice: T |
/// string | null }` → `Option<TOrString<T>>`).
#[derive(Debug, Clone, PartialEq)]
struct Mixed<T: BamlValue> {
    choice: Option<TOrString<T>>,
}

impl<T: BamlValue> __BamlValuePrivate for Mixed<T> {
    fn to_baml(&self) -> wire::InboundValue {
        encode::class(
            "user.Mixed",
            vec![T::baml_ty()],
            vec![("choice", self.choice.to_baml())],
        )
    }

    fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        let mut fields = decode::ClassFields::new(v, "user.Mixed")?;
        Ok(Mixed {
            choice: fields.take("choice")?,
        })
    }

    fn baml_ty() -> wire::BamlTy {
        class_ty("user.Mixed", vec![T::baml_ty()])
    }
}

/// Synthesized union enum shape: one variant per (null-stripped) arm,
/// `From` per payload arm, inbound encodes the bare arm value, decode
/// trial-matches arms in declared order (wire kinds / FQNs / literal
/// values discriminate every supported combination).
#[derive(Debug, Clone, PartialEq)]
enum IntOrString {
    Int(i64),
    String(String),
}

impl From<i64> for IntOrString {
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl From<String> for IntOrString {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl __BamlValuePrivate for IntOrString {
    fn to_baml(&self) -> wire::InboundValue {
        match self {
            Self::Int(value) => value.to_baml(),
            Self::String(value) => value.to_baml(),
        }
    }

    fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        let v = decode::unwrap(v);
        if let Ok(value) = i64::from_baml(v.clone()) {
            return Ok(Self::Int(value));
        }
        if let Ok(value) = String::from_baml(v.clone()) {
            return Ok(Self::String(value));
        }
        Err(decode::no_union_arm("IntOrString", &v))
    }

    fn baml_ty() -> wire::BamlTy {
        union_ty(vec![i64::baml_ty(), String::baml_ty()])
    }
}

#[derive(Debug, Clone, PartialEq)]
enum PointOrString {
    Point(Point),
    String(String),
}

impl From<Point> for PointOrString {
    fn from(value: Point) -> Self {
        Self::Point(value)
    }
}

impl From<String> for PointOrString {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl __BamlValuePrivate for PointOrString {
    fn to_baml(&self) -> wire::InboundValue {
        match self {
            Self::Point(value) => value.to_baml(),
            Self::String(value) => value.to_baml(),
        }
    }

    fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        let v = decode::unwrap(v);
        if let Ok(value) = Point::from_baml(v.clone()) {
            return Ok(Self::Point(value));
        }
        if let Ok(value) = String::from_baml(v.clone()) {
            return Ok(Self::String(value));
        }
        Err(decode::no_union_arm("PointOrString", &v))
    }

    fn baml_ty() -> wire::BamlTy {
        union_ty(vec![Point::baml_ty(), String::baml_ty()])
    }
}

/// String-literal arms become unit variants carrying their wire value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum DraftOrSent {
    Draft,
    Sent,
}

impl __BamlValuePrivate for DraftOrSent {
    fn to_baml(&self) -> wire::InboundValue {
        match self {
            Self::Draft => "draft".to_string().to_baml(),
            Self::Sent => "sent".to_string().to_baml(),
        }
    }

    fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        let v = decode::unwrap(v);
        if let Ok(value) = String::from_baml(v.clone()) {
            match value.as_str() {
                "draft" => return Ok(Self::Draft),
                "sent" => return Ok(Self::Sent),
                _ => {}
            }
        }
        Err(decode::no_union_arm("DraftOrSent", &v))
    }

    fn baml_ty() -> wire::BamlTy {
        union_ty(vec![literal_string_ty("draft"), literal_string_ty("sent")])
    }
}

// ---------------------------------------------------------------------------
// Round trips through the live engine.
// ---------------------------------------------------------------------------

#[test]
fn round_trips_union_arms() {
    assert_eq!(
        call::<IntOrString>(
            "user.rt_int_or_string",
            vec![("u", IntOrString::Int(7).to_baml())]
        ),
        IntOrString::Int(7)
    );
    assert_eq!(
        call::<IntOrString>(
            "user.rt_int_or_string",
            vec![("u", IntOrString::String("s".to_string()).to_baml())]
        ),
        IntOrString::String("s".to_string())
    );
}

#[test]
fn round_trips_class_arm_union() {
    let point = Point {
        x: 1,
        y: 2,
        tag: None,
    };
    assert_eq!(
        call::<PointOrString>(
            "user.rt_point_or_string",
            vec![("u", PointOrString::Point(point.clone()).to_baml())]
        ),
        PointOrString::Point(point)
    );
    assert_eq!(
        call::<PointOrString>(
            "user.rt_point_or_string",
            vec![("u", PointOrString::String("s".to_string()).to_baml())]
        ),
        PointOrString::String("s".to_string())
    );
}

#[test]
fn round_trips_nullable_union_as_option() {
    assert_eq!(
        call::<Option<IntOrString>>(
            "user.rt_opt_union",
            vec![("u", Some(IntOrString::Int(3)).to_baml())]
        ),
        Some(IntOrString::Int(3))
    );
    assert_eq!(
        call::<Option<IntOrString>>(
            "user.rt_opt_union",
            vec![("u", None::<IntOrString>.to_baml())]
        ),
        None
    );
}

#[test]
fn round_trips_literal_union_variants() {
    assert_eq!(
        call::<DraftOrSent>("user.rt_status", vec![("s", DraftOrSent::Sent.to_baml())]),
        DraftOrSent::Sent
    );
}

#[test]
fn round_trips_enum() {
    assert_eq!(
        call::<Color>("user.rt_color", vec![("c", Color::Green.to_baml())]),
        Color::Green
    );
}

#[test]
fn round_trips_class_with_optional_field() {
    let point = Point {
        x: 3,
        y: -4,
        tag: Some("origin-ish".to_string()),
    };
    assert_eq!(
        call::<Point>("user.rt_point", vec![("p", point.to_baml())]),
        point
    );

    let untagged = Point {
        x: 0,
        y: 0,
        tag: None,
    };
    assert_eq!(
        call::<Point>("user.rt_point", vec![("p", untagged.to_baml())]),
        untagged
    );
}

#[test]
fn decodes_engine_constructed_class() {
    assert_eq!(
        call::<Point>(
            "user.make_point",
            vec![("x", 7i64.to_baml()), ("y", 8i64.to_baml())]
        ),
        Point {
            x: 7,
            y: 8,
            tag: None
        }
    );
}

#[test]
fn projects_optional_field_through_the_engine() {
    let point = Point {
        x: 1,
        y: 2,
        tag: Some("t".to_string()),
    };
    assert_eq!(
        call::<Option<String>>("user.point_tag", vec![("p", point.to_baml())]),
        Some("t".to_string())
    );
}

#[test]
fn round_trips_map_preserving_order() {
    let map = Map::from([
        ("zebra".to_string(), 1i64),
        ("aardvark".to_string(), 2),
        ("mongoose".to_string(), 3),
    ]);
    let result: Map<String, i64> = call("user.rt_map", vec![("m", map.to_baml())]);
    assert_eq!(result, map);
    // IndexMap equality is order-insensitive; entry order is part of the
    // contract, so pin it separately.
    assert_eq!(
        result.keys().collect::<Vec<_>>(),
        ["zebra", "aardvark", "mongoose"]
    );
}

#[test]
fn hash_map_is_accepted_and_decodable() {
    let map = HashMap::from([("a".to_string(), 1i64), ("b".to_string(), 2)]);
    let result: HashMap<String, i64> = call("user.rt_map", vec![("m", map.to_baml())]);
    assert_eq!(result, map);
}

#[test]
fn round_trips_recursive_class_through_box() {
    let tree = TreeNode {
        value: 1,
        next: Some(Box::new(TreeNode {
            value: 2,
            next: Some(Box::new(TreeNode {
                value: 3,
                next: None,
            })),
        })),
    };
    assert_eq!(
        call::<TreeNode>("user.rt_tree", vec![("t", tree.to_baml())]),
        tree
    );
}

#[test]
fn round_trips_generic_class_carrying_its_type_arg() {
    ensure_runtime();
    // `rt_wrapper<T>(w: Wrapper<T>) -> Wrapper<T>`: the instance carries its
    // concrete `T` in `class_ty.type_args`, and the generic call binds `T`
    // explicitly — the full generic-class-through-generic-function path.
    let int_wrapped = Wrapper { inner: 5i64 };
    let out = runtime::invoke_sync::<Wrapper<i64>, Infallible>(
        "user.rt_wrapper",
        encode::kwargs(vec![("w", Some(int_wrapped.to_baml()))]),
        encode::type_args(vec![("T", ty_of::<i64>())]),
    )
    .expect("rt_wrapper<int> succeeds");
    assert_eq!(out, int_wrapped);

    // A nested generic instance (`Wrapper<Wrapper<string>>`) exercises the
    // recursive `class_ty.type_args` construction.
    let nested = Wrapper {
        inner: Wrapper {
            inner: "hi".to_string(),
        },
    };
    let out = runtime::invoke_sync::<Wrapper<Wrapper<String>>, Infallible>(
        "user.rt_wrapper",
        encode::kwargs(vec![("w", Some(nested.to_baml()))]),
        encode::type_args(vec![("T", ty_of::<Wrapper<String>>())]),
    )
    .expect("rt_wrapper<Wrapper<string>> succeeds");
    assert_eq!(out, nested);
}

#[test]
fn round_trips_generic_union_field() {
    ensure_runtime();
    // `rt_mixed<T>(m: Mixed<T>) -> Mixed<T>`: the union field `T | string |
    // null` round-trips each arm, with `T` bound to int.
    let cases = [
        Mixed {
            choice: Some(TOrString::T(7i64)),
        },
        Mixed {
            choice: Some(TOrString::String("hi".to_string())),
        },
        Mixed { choice: None },
    ];
    for m in cases {
        let out = runtime::invoke_sync::<Mixed<i64>, Infallible>(
            "user.rt_mixed",
            encode::kwargs(vec![("m", Some(m.to_baml()))]),
            encode::type_args(vec![("T", ty_of::<i64>())]),
        )
        .expect("rt_mixed<int> succeeds");
        assert_eq!(out, m);
    }
}

// ---------------------------------------------------------------------------
// Host callables: closures crossing into BAML, exactly as the Rust SDK
// generator will emit them (the executable spec for that emission).
// ---------------------------------------------------------------------------

/// The param spec the generated binding describes a `(int) -> string`
/// callable with: one required (positional) parameter.
static HC_ONE_INT: &[baml_bridge::HostParam] = &[baml_bridge::HostParam {
    name: "x",
    optional: false,
}];

#[test]
fn host_callable_round_trips_sync_and_async() {
    ensure_runtime();
    let call = |handle: wire::InboundValue, x: i64| {
        runtime::invoke_sync::<String, Infallible>(
            "user.hc_call",
            encode::kwargs(vec![("callback", Some(handle)), ("x", Some(x.to_baml()))]),
            vec![],
        )
    };

    let sync_cb = |x: i64| format!("got {x}");
    assert_eq!(
        call(
            baml_bridge::host_value::callable_handle(sync_cb, HC_ONE_INT),
            5
        )
        .unwrap(),
        "got 5"
    );

    // A future-returning closure is driven on the bridge's dispatch
    // runtime, even on the sync call path.
    let async_cb = |x: i64| async move { format!("async {x}") };
    assert_eq!(
        call(
            baml_bridge::host_value::callable_handle(async_cb, HC_ONE_INT),
            7
        )
        .unwrap(),
        "async 7"
    );
}

#[test]
fn host_callable_optionals_deliver_by_name() {
    static PARAMS: &[baml_bridge::HostParam] = &[
        baml_bridge::HostParam {
            name: "x",
            optional: false,
        },
        baml_bridge::HostParam {
            name: "y",
            optional: true,
        },
        baml_bridge::HostParam {
            name: "z",
            optional: true,
        },
    ];
    ensure_runtime();
    // An omitted optional arrives as `None`; the host default fills it.
    let cb =
        |x: i64, y: Option<i64>, z: Option<i64>| x * 100 + y.unwrap_or(8) * 10 + z.unwrap_or(9);
    let result = runtime::invoke_sync::<Vec<i64>, Infallible>(
        "user.hc_optionals",
        encode::kwargs(vec![
            (
                "callback",
                Some(baml_bridge::host_value::callable_handle(cb, PARAMS)),
            ),
            ("x", Some(5i64.to_baml())),
        ]),
        vec![],
    )
    .expect("hc_optionals succeeds");
    assert_eq!(result, [589, 529, 583, 523]);
}

#[test]
fn host_callable_typed_throw_propagates_as_the_declared_class() {
    ensure_runtime();
    // The closure's error type IS the declared BAML throws class — it
    // crosses as that real class and lands in `Error::Thrown`.
    let cb = |_x: i64| -> Result<String, Point> {
        Err(Point {
            x: 1,
            y: 2,
            tag: Some("thrown".to_string()),
        })
    };
    let err = runtime::invoke_sync::<String, Point>(
        "user.hc_typed_throws",
        encode::kwargs(vec![
            (
                "callback",
                Some(baml_bridge::host_value::callable_handle(cb, HC_ONE_INT)),
            ),
            ("x", Some(1i64.to_baml())),
        ]),
        vec![],
    )
    .expect_err("the typed host throw must propagate");
    match err {
        Error::Thrown { value, .. } => {
            assert_eq!(
                *value,
                Point {
                    x: 1,
                    y: 2,
                    tag: Some("thrown".to_string()),
                }
            );
        }
        other => panic!("expected the typed throw, got {other}"),
    }
}

/// An arbitrary host error with no BAML representation.
#[derive(Debug, Clone, PartialEq)]
struct Boom(String);

impl std::fmt::Display for Boom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for Boom {}

#[test]
fn host_callable_opaque_throw_rehydrates_the_original() {
    ensure_runtime();
    let raised = Boom("nope".to_string());
    let cb = {
        let raised = raised.clone();
        move |_x: i64| -> Result<String, Boom> { Err(raised.clone()) }
    };
    // `baml.errors.HostCallable` is a normal throws member: decode with
    // `E = HostCallable` and the throw arrives as `Error::Thrown` like any
    // other declared error class — metadata 1:1 with the BAML instance,
    // plus the rehydrated original the BAML side holds only as a handle.
    let err = runtime::invoke_sync::<String, baml_bridge::HostCallable>(
        "user.hc_call",
        encode::kwargs(vec![
            (
                "callback",
                Some(baml_bridge::host_value::callable_handle(cb, HC_ONE_INT)),
            ),
            ("x", Some(1i64.to_baml())),
        ]),
        vec![],
    )
    .expect_err("the opaque host throw must propagate");
    let Error::Thrown { value, .. } = err else {
        panic!("expected the HostCallable throw, got {err}");
    };
    assert_eq!(value.message, "nope");
    // Erased in BAML; retained on the Rust side. The original comes back
    // both by borrow (`downcast_ref`) and owned (`original` → `Arc::downcast`)
    // — the concrete type stands in for the erased `class_name` metadata.
    assert_eq!(value.downcast_ref::<Boom>(), Some(&raised));
    assert_eq!(*value.original().downcast::<Boom>().unwrap(), raised);
}

/// A second host-error type, distinct from [`Boom`].
#[derive(Debug)]
struct Splat;

impl std::fmt::Display for Splat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "splat")
    }
}

impl std::error::Error for Splat {}

#[test]
fn host_callable_opaque_throw_decodes_into_typed_hostcallable() {
    ensure_runtime();
    let raised = Boom("nope".to_string());
    let cb = {
        let raised = raised.clone();
        move |_x: i64| -> Result<String, Boom> { Err(raised.clone()) }
    };
    // A caller that statically knows the host-error type decodes straight
    // into `HostCallable<Boom>`: `from_baml` validates the rehydrated
    // original really is a `Boom`, so `original()` is a `Arc<Boom>` with no
    // runtime downcast at the use site.
    let err = runtime::invoke_sync::<String, baml_bridge::HostCallable<Boom>>(
        "user.hc_call",
        encode::kwargs(vec![
            (
                "callback",
                Some(baml_bridge::host_value::callable_handle(cb, HC_ONE_INT)),
            ),
            ("x", Some(1i64.to_baml())),
        ]),
        vec![],
    )
    .expect_err("the opaque host throw must propagate");
    let Error::Thrown { value, .. } = err else {
        panic!("expected the typed HostCallable throw, got {err}");
    };
    assert_eq!(value.message, "nope");
    let original: std::sync::Arc<Boom> = value.original();
    assert_eq!(*original, raised);
}

#[test]
fn host_callable_typed_decode_rejects_wrong_type() {
    ensure_runtime();
    // The callback throws a `Boom`, but the caller optimistically decodes as
    // `HostCallable<Splat>`. The rehydrated original is not a `Splat`, so
    // `from_baml`'s validating downcast fails and `decode_result` folds it
    // into `Error::Runtime` — the same fallback as any non-declared throw.
    let cb = |_x: i64| -> Result<String, Boom> { Err(Boom("wrong type".to_string())) };
    let err = runtime::invoke_sync::<String, baml_bridge::HostCallable<Splat>>(
        "user.hc_call",
        encode::kwargs(vec![
            (
                "callback",
                Some(baml_bridge::host_value::callable_handle(cb, HC_ONE_INT)),
            ),
            ("x", Some(1i64.to_baml())),
        ]),
        vec![],
    )
    .expect_err("the opaque host throw must propagate");
    match err {
        Error::Runtime { class_name, .. } => {
            assert_eq!(class_name.as_deref(), Some("baml.errors.HostCallable"));
        }
        other => panic!("expected the Runtime fallback for a wrong-typed decode, got {other}"),
    }
}

#[test]
fn host_callable_opaque_throw_is_catchable_in_baml() {
    ensure_runtime();
    // BAML's `catch (e)` intercepts the `baml.errors.HostCallable` throw
    // and can read its `class_name` metadata (`hc_catches` returns
    // `"caught:" + e.class_name`).
    let cb = |_x: i64| -> Result<String, Boom> { Err(Boom("boom from host".to_string())) };
    let result = runtime::invoke_sync::<String, Infallible>(
        "user.hc_catches",
        encode::kwargs(vec![
            (
                "callback",
                Some(baml_bridge::host_value::callable_handle(cb, HC_ONE_INT)),
            ),
            ("x", Some(1i64.to_baml())),
        ]),
        vec![],
    )
    .expect("the BAML catch must recover");
    // `class_name` is the full, compiler-dependent `type_name` — assert the
    // catch fired and read the field, not the exact string.
    assert!(
        result.starts_with("caught:") && result.contains("Boom"),
        "{result}"
    );
}

#[test]
fn class_fqn_drift_fails_loudly() {
    // Decoding an `Other` as `Point` must be a hard FQN mismatch, never a
    // positional coercion.
    ensure_runtime();
    let result = runtime::invoke_sync::<Point, Infallible>(
        "user.make_other",
        encode::kwargs(vec![("x", Some(1i64.to_baml()))]),
        vec![],
    );
    match result {
        Err(Error::Decode(DecodeError::FqnMismatch { expected, got })) => {
            assert_eq!(expected, "user.Point");
            assert_eq!(got, "user.Other");
        }
        other => panic!("expected an FQN mismatch, got {other:?}"),
    }
}

#[test]
fn wrong_wire_kind_fails_loudly() {
    // Decoding a class value as an enum is a wire-kind mismatch.
    ensure_runtime();
    let result = runtime::invoke_sync::<Color, Infallible>(
        "user.make_point",
        encode::kwargs(vec![
            ("x", Some(1i64.to_baml())),
            ("y", Some(2i64.to_baml())),
        ]),
        vec![],
    );
    assert!(matches!(
        result,
        Err(Error::Decode(DecodeError::WrongType { .. }))
    ));
}
