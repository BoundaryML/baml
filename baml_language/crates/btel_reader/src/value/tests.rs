use baml_type::{DeclarationName, TypeName, typetag::TypeTag};
use btel_snapshot::{CasId, DecodedName, DecodedRoot, TypeDescription};
use serde_json::json;

use super::*;
use crate::{cas::CasUnavailable, evidence::SlotName};

/// Blobs by ID, as a CAS directory would hold them.
#[derive(Default)]
struct Blobs(HashMap<CasId, Result<Arc<DecodedSnapshot>, CasUnavailable>>);
impl BlobSource for Blobs {
    fn load(&self, id: CasId) -> Result<Arc<DecodedSnapshot>, CasUnavailable> {
        self.0
            .get(&id)
            .cloned()
            .unwrap_or(Err(CasUnavailable::Missing))
    }
}

/// What navigation found, when it found a value.
fn found(nav: &Nav) -> Option<DecodedValue> {
    match nav {
        Nav::Value(found) => Some(found.value().into()),
        Nav::Arguments(_) | Nav::Missing | Nav::Unavailable(_) => None,
    }
}

#[test]
fn typed_map_keys_render_as_pairs_and_navigate_without_collisions() {
    use DecodedValue as V;
    let snapshot = Arc::new(DecodedSnapshot {
        id: CasId::from_bytes([7; 16]),
        encoded_len: 0,
        children: vec![],
        root: DecodedRoot::Value(V::Object(NodeId(0))),
        objects: vec![DecodedObject::Map {
            key_type: ty(),
            value_type: ty(),
            original_len: 3,
            entries: vec![
                (V::Int(1), s("integer")),
                (s("1"), s("string")),
                (V::Object(NodeId(0)), s("self")),
            ],
        }],
    });
    let blobs = Blobs::default();
    for (segment, expected) in [
        (Segment::Index(1), "integer"),
        (Segment::Key("1".into()), "string"),
    ] {
        assert_eq!(
            found(&navigate(
                &blobs,
                &snapshot,
                Root::Value,
                None,
                &[segment],
                0
            )),
            Some(s(expected))
        );
    }
    let Nav::Value(value) = navigate(&blobs, &snapshot, Root::Value, None, &[], 0) else {
        panic!()
    };
    assert_eq!(
        render_value(&blobs, &value, &RenderLimits::default()).json,
        json!({"$id": 0, "$map": [[1, "integer"], ["1", "string"], [{"$ref": 0}, "self"]]})
    );
}

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
fn snapshot() -> Arc<DecodedSnapshot> {
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
            declaration: NodeId(0),
            fields: vec![
                ("name".into(), s("Ann")),
                ("age".into(), V::Int(30)),
                ("items".into(), V::Object(NodeId(2))),
                ("meta".into(), V::Object(NodeId(4))),
                ("nothing".into(), V::Null),
                ("cell".into(), V::Object(NodeId(5))),
                ("cut".into(), V::Truncated(Limit::Bytes)),
            ],
            original_len: 7,
        },
        // 2: items list, second element shared with meta.first
        O::List {
            element_type: ty(),
            items: vec![V::Object(NodeId(3)), V::Object(NodeId(3))],
            original_len: 3,
        },
        // 3: item map
        O::Map {
            key_type: ty(),
            value_type: ty(),
            entries: vec![(s("name"), s("widget")), (s("$price"), V::Float(1.5))],
            original_len: 2,
        },
        // 4: meta map that contains itself
        O::Map {
            key_type: ty(),
            value_type: ty(),
            entries: vec![
                (s("self"), V::Object(NodeId(4))),
                (s("big"), V::Bigint(Arc::new(BigInt::from(1) << 80))),
            ],
            original_len: 2,
        },
        // 5: cell around a scalar
        O::Cell(V::Int(99)),
    ];
    Arc::new(DecodedSnapshot {
        id: CasId::from_bytes([0; 16]),
        encoded_len: 0,
        children: Vec::new(),
        root: DecodedRoot::FunctionArgs {
            parameter_count: 2,
            slots: vec![V::Object(NodeId(1)), V::OmittedArg],
        },
        objects,
    })
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

fn nav(snapshot: &Arc<DecodedSnapshot>, names: Option<&ArgumentNames>, p: &[&str]) -> Nav {
    navigate(
        &Blobs::default(),
        snapshot,
        Root::Arguments,
        names,
        &path(p),
        0,
    )
}

#[test]
fn named_arguments_and_nested_paths_distinguish_null_missing_and_unavailable() {
    let snap = snapshot();
    let n = names();
    assert_eq!(
        found(&nav(&snap, Some(&n), &["customer", "age"])),
        Some(DecodedValue::Int(30))
    );
    assert_eq!(
        found(&nav(&snap, Some(&n), &["customer", "items", "0", "name"])),
        Some(s("widget"))
    );
    assert_eq!(
        found(&nav(&snap, Some(&n), &["customer", "nothing"])),
        Some(DecodedValue::Null)
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
        found(&nav(&snap, Some(&n), &["customer", "cell"])),
        Some(DecodedValue::Int(99))
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
        found(&nav(&snap, None, &["0", "age"])),
        Some(DecodedValue::Int(30))
    );
    let mut mismatch = names();
    mismatch.slots.pop();
    assert_eq!(
        nav(&snap, Some(&mismatch), &["customer"]),
        Nav::Unavailable(Unavailable::ArgumentLayoutMismatch)
    );
    assert_eq!(nav(&snap, Some(&n), &[]), Nav::Arguments(Arc::clone(&snap)));
    assert_eq!(
        navigate(&Blobs::default(), &snap, Root::Value, None, &[], 0),
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
    assert_eq!(is_null(&Nav::Missing), Some(true));
    assert_eq!(
        is_null(&Nav::Unavailable(Unavailable::ArgumentNamesUnknown)),
        None
    );
}

#[test]
fn rendering_preserves_sharing_cycles_truncation_and_names() {
    let snap = snapshot();
    let limits = RenderLimits::default();
    let blobs = Blobs::default();
    let rendered = render_arguments(&blobs, &snap, Some(&names()), &limits);
    assert_eq!(rendered.incomplete, None);
    let args = rendered.json;
    let customer = &args["customer"];
    assert_eq!(customer["$class"], json!("user.Customer"));
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
    let positional = render_arguments(&blobs, &snap, None, &limits).json;
    assert_eq!(positional["$args"][1], json!({"$omitted": true}));
    // Depth and size bounds.
    let shallow = render_arguments(
        &blobs,
        &snap,
        Some(&names()),
        &RenderLimits {
            max_depth: 2,
            ..limits
        },
    )
    .json;
    assert_eq!(
        shallow["customer"]["items"],
        json!({"$truncated": "render_depth"})
    );
}

#[test]
fn shared_labels_follow_output_order_and_skip_truncated_visits() {
    use DecodedObject as O;
    use DecodedValue as V;
    let map = |value: i64| O::Map {
        key_type: ty(),
        value_type: ty(),
        entries: vec![(s("v"), V::Int(value))],
        original_len: 1,
    };
    let list = |items: Vec<DecodedValue>| O::List {
        element_type: ty(),
        original_len: items.len() as u64,
        items,
    };
    // Argument `deep` reaches map 0 two lists down; `a` and `b` reach it and
    // map 1 directly.
    let snap = Arc::new(DecodedSnapshot {
        id: CasId::from_bytes([0; 16]),
        encoded_len: 0,
        children: Vec::new(),
        root: DecodedRoot::FunctionArgs {
            parameter_count: 3,
            slots: vec![
                V::Object(NodeId(2)),
                V::Object(NodeId(1)),
                V::Object(NodeId(0)),
            ],
        },
        objects: vec![
            map(0),
            map(1),
            list(vec![V::Object(NodeId(3)), V::Object(NodeId(1))]),
            list(vec![V::Object(NodeId(0))]),
        ],
    });
    let names = ArgumentNames {
        slots: ["deep", "a", "b"]
            .into_iter()
            .map(|name| SlotName {
                name: Some(name.into()),
                receiver: false,
            })
            .collect(),
    };
    let limits = RenderLimits {
        max_depth: 3,
        ..RenderLimits::default()
    };
    let args = render_arguments(&Blobs::default(), &snap, Some(&names), &limits).json;
    // Map 0 is first reached at the depth limit: no label is spent there, so
    // its later full rendering carries the `$id`.
    assert_eq!(args["deep"][0][0], json!({"$truncated": "render_depth"}));
    // Map 1 is printed first, so it takes label 0.
    assert_eq!(args["deep"][1], json!({"$id": 0, "$map": {"v": 1}}));
    assert_eq!(args["a"], json!({"$ref": 0}));
    assert_eq!(args["b"], json!({"$id": 1, "$map": {"v": 0}}));
}

#[test]
fn a_number_too_long_to_show_is_cut() {
    // 4001 bits print as 1205 digits.
    let number = BigInt::from(1) << 4000_u32;
    let digits = number.to_string();
    assert_eq!(digits.len(), 1205);
    let snap = Arc::new(DecodedSnapshot {
        id: CasId::from_bytes([0; 16]),
        encoded_len: 0,
        children: Vec::new(),
        root: DecodedRoot::FunctionArgs {
            parameter_count: 1,
            slots: vec![DecodedValue::Bigint(Arc::new(number))],
        },
        objects: Vec::new(),
    });
    let render = |max_text_bytes| {
        let limits = RenderLimits {
            max_text_bytes,
            ..RenderLimits::default()
        };
        render_arguments(&Blobs::default(), &snap, None, &limits)
    };
    let whole = render(1205);
    assert_eq!(whole.json, json!({"$args": [{"$bigint": digits}]}));
    assert!(!whole.cut);
    let cut = render(1204);
    assert_eq!(
        cut.json,
        json!({"$args": [{"$bigint": {"$truncated": "render_size"}}]})
    );
    assert!(cut.cut);

    // Printing takes time quadratic in a number's length, so one past the
    // limit is not printed at all, alone or inside a value.
    let present = |max_bigint_bits| {
        let limits = RenderLimits {
            max_bigint_bits,
            ..RenderLimits::default()
        };
        let found = nav(&snap, None, &["0"]);
        (
            to_scalar(&Blobs::default(), &found, None, &limits),
            render_arguments(&Blobs::default(), &snap, None, &limits),
        )
    };
    let (alone, inside) = present(4001);
    assert_eq!(
        (alone.scalar, alone.kind, alone.cut),
        (Scalar::Text(digits.clone()), Kind::Bigint, false)
    );
    assert_eq!(inside.json, json!({"$args": [{"$bigint": digits}]}));
    let (alone, inside) = present(4000);
    let cut = json!({"$bigint": {"$truncated": "render_size"}});
    assert_eq!(
        (alone.scalar, alone.kind, alone.cut),
        (Scalar::Text(cut.to_string()), Kind::Json, true)
    );
    assert_eq!(inside.json, json!({"$args": [cut]}));
    assert!(inside.cut);
}

#[test]
fn arguments_end_where_the_node_limit_is_reached() {
    let snap = snapshot();
    let limits = RenderLimits {
        max_nodes: 0,
        ..RenderLimits::default()
    };
    let named = render_arguments(&Blobs::default(), &snap, Some(&names()), &limits);
    assert_eq!(named.json, json!({"$truncated": "render_size"}));
    assert!(named.cut);
    let positional = render_arguments(&Blobs::default(), &snap, None, &limits);
    assert_eq!(
        positional.json,
        json!({"$args": [], "$truncated": "render_size"})
    );
    assert!(positional.cut);
}

#[test]
fn scalar_projection_keeps_leaf_kinds() {
    let snap = snapshot();
    let n = names();
    let limits = RenderLimits::default();
    let scalar = |p: &[&str]| {
        let presented = to_scalar(
            &Blobs::default(),
            &nav(&snap, Some(&n), p),
            Some(&n),
            &limits,
        );
        assert_eq!(presented.incomplete, None);
        (presented.scalar, presented.kind)
    };
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
