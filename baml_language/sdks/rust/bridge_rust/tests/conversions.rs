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
        class_ty("user.Point")
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
        class_ty("user.TreeNode")
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
