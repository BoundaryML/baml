//! Live-engine tests for class / enum / map conversions.
//!
//! The `BamlValue` impls here are written by hand in exactly the shape the
//! Rust SDK generator emits — this file is the executable spec for the
//! emitted impl blocks, proven against a real engine before the generator
//! produces them.

use std::{collections::HashMap, convert::Infallible, sync::OnceLock};

use baml_rs::{
    BamlValue, DecodeError, Error, Map, baml_value::internal::__BamlValuePrivate, decode, encode,
    runtime, wire,
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
"#;

fn ensure_runtime() {
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        let files = HashMap::from([("main.baml".to_string(), BAML_SRC.to_string())]);
        // The vfs rejects trailing slashes (macOS's temp_dir ends in one).
        let root = std::env::temp_dir();
        let root = root.to_string_lossy();
        let root = root.trim_end_matches('/');
        runtime::initialize_from_files(root, files).expect("runtime initializes");
    });
}

fn call<R: BamlValue>(fqn: &str, kwargs: Vec<(&str, wire::InboundValue)>) -> R {
    ensure_runtime();
    let kwargs = kwargs.into_iter().map(|(k, v)| (k, Some(v))).collect();
    runtime::invoke_sync::<R, Infallible>(fqn, encode::kwargs(kwargs))
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
}

// ---------------------------------------------------------------------------
// Round trips through the live engine.
// ---------------------------------------------------------------------------

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
    );
    assert!(matches!(
        result,
        Err(Error::Decode(DecodeError::WrongType { .. }))
    ));
}
