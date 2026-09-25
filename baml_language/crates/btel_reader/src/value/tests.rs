use baml_type::{DeclarationName, TypeName, typetag::TypeTag};
use btel_snapshot::{DecodedName, DecodedRoot, SnapshotId, TypeDescription};
use serde_json::json;

use super::*;
use crate::evidence::SlotName;

fn ty() -> TypeDescription {
    TypeDescription {
        encoded: Box::new([]),
        decoded: None,
    }
}

fn s(text: &str) -> DecodedValue {
    DecodedValue::String(text.into())
}

/// args: (customer: Customer{name, age, items: [ {name}, shared ], tags}, note: omitted)
fn snapshot() -> DecodedSnapshot {
    use DecodedObject as O;
    use DecodedValue as V;
    let objects = vec![
        // 0: declaration
        O::Declaration {
            name: DecodedName(DeclarationName::Declared(TypeName::from_dotted_path(
                "user.Customer",
            ))),
            tag: TypeTag::from_i64(1),
            is_enum: false,
        },
        // 1: customer instance
        O::Instance {
            type_arguments: vec![],
            declaration: 0,
            fields: vec![
                ("name".into(), s("Ann")),
                ("age".into(), V::Int(30)),
                ("items".into(), V::Object(2)),
                ("meta".into(), V::Object(4)),
                ("nothing".into(), V::Null),
                ("cell".into(), V::Object(5)),
                ("cut".into(), V::Truncated(Limit::Bytes)),
            ],
            original_len: 7,
        },
        // 2: items list, second element shared with meta.first
        O::List {
            element_type: ty(),
            items: vec![V::Object(3), V::Object(3)],
            original_len: 3,
        },
        // 3: item map
        O::Map {
            key_type: ty(),
            value_type: ty(),
            entries: vec![
                ("name".into(), s("widget")),
                ("$price".into(), V::Float(1.5)),
            ],
            original_len: 2,
        },
        // 4: meta map that contains itself
        O::Map {
            key_type: ty(),
            value_type: ty(),
            entries: vec![
                ("self".into(), V::Object(4)),
                ("big".into(), V::Bigint(Box::new(BigInt::from(1) << 80))),
            ],
            original_len: 2,
        },
        // 5: cell around a scalar
        O::Cell(V::Int(99)),
    ];
    DecodedSnapshot {
        id: SnapshotId::from_bytes([0; 16]),
        limited: true,
        root: DecodedRoot::FunctionArgs {
            parameter_count: 2,
            slots: vec![V::Object(1), V::OmittedArg],
        },
        objects,
    }
}

fn names() -> ArgumentNames {
    ArgumentNames {
        slots: vec![
            SlotName {
                name: Some("customer".into()),
                receiver: false,
            },
            SlotName {
                name: Some("note".into()),
                receiver: false,
            },
        ],
    }
}

fn path(segments: &[&str]) -> Vec<Segment> {
    segments
        .iter()
        .map(|segment| match segment.parse::<i64>() {
            Ok(index) => Segment::Index(index),
            Err(_) => Segment::Key((*segment).to_string()),
        })
        .collect()
}

fn nav<'a>(snapshot: &'a DecodedSnapshot, names: Option<&ArgumentNames>, p: &[&str]) -> Nav<'a> {
    navigate(snapshot, Root::Arguments, names, &path(p))
}

#[test]
fn named_arguments_and_nested_paths_distinguish_null_missing_and_unavailable() {
    let snap = snapshot();
    let n = names();
    assert_eq!(
        nav(&snap, Some(&n), &["customer", "age"]),
        Nav::Value(&DecodedValue::Int(30))
    );
    assert_eq!(
        nav(&snap, Some(&n), &["customer", "items", "0", "name"]),
        Nav::Value(&s("widget"))
    );
    assert_eq!(
        nav(&snap, Some(&n), &["customer", "nothing"]),
        Nav::Value(&DecodedValue::Null)
    );
    assert_eq!(nav(&snap, Some(&n), &["customer", "absent"]), Nav::Missing);
    assert_eq!(
        nav(&snap, Some(&n), &["customer", "age", "deeper"]),
        Nav::Missing
    );
    assert_eq!(
        nav(&snap, Some(&n), &["customer", "items", "-1"]),
        Nav::Missing
    );
    // Index 2 existed but the capture kept two items.
    assert_eq!(
        nav(&snap, Some(&n), &["customer", "items", "2"]),
        Nav::Unavailable(Unavailable::Truncated(Limit::Values))
    );
    assert_eq!(
        nav(&snap, Some(&n), &["customer", "items", "3"]),
        Nav::Missing
    );
    assert_eq!(
        nav(&snap, Some(&n), &["customer", "cut"]),
        Nav::Unavailable(Unavailable::Truncated(Limit::Bytes))
    );
    assert_eq!(
        nav(&snap, Some(&n), &["customer", "cell"]),
        Nav::Value(&DecodedValue::Int(99))
    );
    // Omitted argument: absent, not null.
    assert_eq!(nav(&snap, Some(&n), &["note"]), Nav::Missing);
    assert_eq!(nav(&snap, Some(&n), &["unknown"]), Nav::Missing);
    // Without recorded names only positions work.
    assert_eq!(
        nav(&snap, None, &["customer", "age"]),
        Nav::Unavailable(Unavailable::ArgumentNamesUnknown)
    );
    assert_eq!(
        nav(&snap, None, &["0", "age"]),
        Nav::Value(&DecodedValue::Int(30))
    );
    let mut mismatch = names();
    mismatch.slots.pop();
    assert_eq!(
        nav(&snap, Some(&mismatch), &["customer"]),
        Nav::Unavailable(Unavailable::ArgumentLayoutMismatch)
    );
    assert_eq!(nav(&snap, Some(&n), &[]), Nav::Arguments);
    assert_eq!(
        navigate(&snap, Root::Value, None, &[]),
        Nav::Unavailable(Unavailable::WrongRoot)
    );
}

#[test]
fn comparisons_follow_baml_semantics_not_sqlite_coercion() {
    let int = |n| Leaf::Int(n);
    assert_eq!(compare(int(30), CmpOp::GtEq, Leaf::Int(30)), Some(true));
    // Exact int/float comparison past 2^53.
    let big = (1_i64 << 53) + 1;
    assert_eq!(
        compare(int(big), CmpOp::Eq, Leaf::Float(9_007_199_254_740_992.0)),
        Some(false)
    );
    assert_eq!(
        compare(int(big), CmpOp::Gt, Leaf::Float(9_007_199_254_740_992.0)),
        Some(true)
    );
    assert_eq!(compare(int(2), CmpOp::Lt, Leaf::Float(2.5)), Some(true));
    let huge = BigInt::from(1) << 80;
    assert_eq!(
        compare(Leaf::Bigint(&huge), CmpOp::Gt, int(i64::MAX)),
        Some(true)
    );
    // Text never coerces to a number: '30' is not 30.
    assert_eq!(compare(int(30), CmpOp::Eq, Leaf::Text("30")), Some(false));
    assert_eq!(compare(int(30), CmpOp::Lt, Leaf::Text("31")), None);
    assert_eq!(
        compare(Leaf::Text("abc"), CmpOp::Lt, Leaf::Text("abd")),
        Some(true)
    );
    assert_eq!(
        compare(Leaf::Enum("Active"), CmpOp::Eq, Leaf::Text("Active")),
        Some(true)
    );
    assert_eq!(
        compare(Leaf::Enum("Active"), CmpOp::Lt, Leaf::Text("B")),
        None
    );
    assert_eq!(
        compare(Leaf::Bool(true), CmpOp::Eq, Leaf::Bool(true)),
        Some(true)
    );
    assert_eq!(compare(Leaf::Bool(true), CmpOp::Eq, int(1)), Some(false));
    assert_eq!(
        compare(Leaf::Float(f64::NAN), CmpOp::Eq, Leaf::Float(f64::NAN)),
        Some(true)
    );
    assert_eq!(
        compare(Leaf::Float(f64::NAN), CmpOp::Lt, Leaf::Float(1.0)),
        None
    );
    assert_eq!(compare(Leaf::Null, CmpOp::Eq, Leaf::Null), Some(true));
    assert_eq!(compare(Leaf::Null, CmpOp::NotEq, int(1)), Some(true));
    assert_eq!(compare(Leaf::Structured, CmpOp::Eq, int(1)), Some(false));
    assert_eq!(compare(Leaf::Structured, CmpOp::Eq, Leaf::Structured), None);
    assert!(
        leaf_of_operand(Operand::Null).is_none(),
        "SQL NULL is unknown"
    );
    assert_eq!(is_null(Nav::Missing), Some(true));
    assert_eq!(
        is_null(Nav::Unavailable(Unavailable::ArgumentNamesUnknown)),
        None
    );
}

#[test]
fn rendering_preserves_sharing_cycles_truncation_and_names() {
    let snap = snapshot();
    let limits = RenderLimits::default();
    let args = render_arguments(&snap, Some(&names()), &limits);
    let customer = &args["customer"];
    assert_eq!(customer["$class"], json!("Customer"));
    assert_eq!(customer["age"], json!(30));
    assert_eq!(customer["nothing"], Json::Null);
    assert_eq!(customer["cell"], json!(99));
    assert_eq!(customer["cut"], json!({"$truncated": "bytes"}));
    assert_eq!(args["note"], json!({"$omitted": true}));
    // The list lost one item; the shared map is rendered once then referenced.
    let items = &customer["items"];
    assert_eq!(items["$original_len"], json!(3));
    let first = &items["$list"][0];
    assert!(
        first["$id"].is_u64(),
        "shared object carries an id: {first}"
    );
    assert!(
        first["$map"]["$price"].is_number(),
        "`$` keys use the $map form: {first}"
    );
    assert_eq!(items["$list"][1], json!({"$ref": first["$id"]}));
    // A self-referencing map renders its own reference.
    let meta = &customer["meta"];
    assert_eq!(meta["$map"]["self"], json!({"$ref": meta["$id"]}));
    let big = (BigInt::from(1) << 80_u32).to_string();
    assert_eq!(meta["$map"]["big"], json!({ "$bigint": big }));
    // Without names: positional envelope.
    let positional = render_arguments(&snap, None, &limits);
    assert_eq!(positional["$args"][1], json!({"$omitted": true}));
    // Depth and size bounds.
    let shallow = render_arguments(
        &snap,
        Some(&names()),
        &RenderLimits {
            max_depth: 2,
            ..limits
        },
    );
    assert_eq!(
        shallow["customer"]["items"],
        json!({"$truncated": "render_depth"})
    );
}

#[test]
fn scalar_projection_keeps_leaf_kinds() {
    let snap = snapshot();
    let n = names();
    let limits = RenderLimits::default();
    let scalar = |p: &[&str]| to_scalar(&snap, nav(&snap, Some(&n), p), Some(&n), &limits);
    assert_eq!(
        scalar(&["customer", "age"]),
        (Scalar::Integer(30), Kind::Int)
    );
    assert_eq!(
        scalar(&["customer", "name"]),
        (Scalar::Text("Ann".into()), Kind::String)
    );
    assert_eq!(scalar(&["customer", "nothing"]), (Scalar::Null, Kind::Null));
    assert_eq!(
        scalar(&["customer", "absent"]),
        (Scalar::Null, Kind::Missing)
    );
    assert_eq!(
        scalar(&["customer", "cut"]),
        (Scalar::Null, Kind::Unavailable)
    );
    let (Scalar::Text(json), Kind::Json) = scalar(&["customer", "items", "0"]) else {
        panic!("structured value renders as JSON text")
    };
    assert!(json.contains("widget"));
}
