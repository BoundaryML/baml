use baml_type::{DeclarationName, TypeName, typetag::TypeTag};
use btel_snapshot::{DecodedRoot, SnapshotId, TypeDescription};
use serde_json::json;

use super::*;
use crate::{
    evidence::SlotName,
    value::{Root, navigate},
};

fn snapshot(root: DecodedValue, objects: Vec<DecodedObject>) -> DecodedSnapshot {
    DecodedSnapshot {
        id: SnapshotId::from_bytes([0; 16]),
        limited: false,
        root: DecodedRoot::Value(root),
        objects,
    }
}

fn ty() -> TypeDescription {
    TypeDescription {
        encoded: Box::new([]),
        decoded: None,
    }
}

fn list(items: Vec<DecodedValue>) -> DecodedObject {
    DecodedObject::List {
        element_type: ty(),
        original_len: items.len() as u64,
        items,
    }
}

fn map(entries: Vec<(&str, DecodedValue)>) -> DecodedObject {
    DecodedObject::Map {
        key_type: ty(),
        value_type: ty(),
        original_len: entries.len() as u64,
        entries: entries.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    }
}

fn capture(s: &DecodedSnapshot) -> Captured<'_> {
    Captured {
        snapshot: s,
        nav: navigate(s, Root::Value, None, &[]),
        names: None,
    }
}

fn eq(a: &DecodedSnapshot, b: &DecodedSnapshot) -> Result<Option<bool>, Error> {
    captured(capture(a), CmpOp::Eq, capture(b), &Limits::default())
}

fn declaration(name: &str, is_enum: bool) -> DecodedObject {
    DecodedObject::Declaration {
        name: DecodedName(DeclarationName::Declared(TypeName::from_dotted_path(name))),
        tag: TypeTag::from_i64(1),
        is_enum,
    }
}

#[test]
fn maps_ignore_order_and_lists_ignore_storage_sharing() {
    use DecodedValue::{Int, Object};
    let a = snapshot(
        Object(0),
        vec![
            list(vec![Object(1), Object(1)]),
            map(vec![("a", Int(1)), ("b", Int(2))]),
        ],
    );
    let mut b = snapshot(
        Object(0),
        vec![
            list(vec![Object(1), Object(2)]),
            map(vec![("b", Int(2)), ("a", Int(1))]),
            map(vec![("a", Int(1)), ("b", Int(2))]),
        ],
    );
    assert_eq!(eq(&a, &b), Ok(Some(true)));
    b.objects[2] = map(vec![("a", Int(9)), ("b", Int(2))]);
    assert_eq!(eq(&a, &b), Ok(Some(false)));
    assert_eq!(
        captured(capture(&a), CmpOp::NotEq, capture(&b), &Limits::default()),
        Ok(Some(true))
    );
    b.objects[0] = list(vec![Object(1)]);
    assert_eq!(eq(&a, &b), Ok(Some(false)));
}

#[test]
fn class_enum_identity_and_field_presence_are_semantic() {
    let a = snapshot(
        DecodedValue::Object(0),
        vec![
            DecodedObject::Instance {
                type_arguments: vec![],
                declaration: 1,
                fields: vec![("x".into(), DecodedValue::Null)],
                original_len: 1,
            },
            declaration("user.A", false),
        ],
    );
    let mut b = a.clone();
    assert_eq!(eq(&a, &b), Ok(Some(true)));
    b.objects[1] = declaration("user.B", false);
    assert_eq!(eq(&a, &b), Ok(Some(false)));
    b.objects[1] = a.objects[1].clone();
    let DecodedObject::Instance { fields, .. } = &mut b.objects[0] else {
        panic!()
    };
    fields[0].1 = DecodedValue::OmittedArg;
    assert_eq!(eq(&a, &b), Ok(Some(false)));
    assert_eq!(
        super::json(
            capture(&a),
            CmpOp::Eq,
            &json!({"x": null}),
            &Limits::default()
        ),
        Ok(Some(false)),
        "plain JSON is a map, not a class"
    );
    let a = snapshot(
        DecodedValue::Enum {
            declaration: 0,
            variant: 0,
            name: "Ready".into(),
        },
        vec![declaration("user.State", true)],
    );
    let mut b = a.clone();
    assert_eq!(eq(&a, &b), Ok(Some(true)));
    b.objects[0] = declaration("user.Other", true);
    assert_eq!(eq(&a, &b), Ok(Some(false)));
    assert_eq!(
        super::json(capture(&a), CmpOp::Eq, &json!("Ready"), &Limits::default()),
        Ok(Some(false))
    );
}

#[test]
fn numeric_equality_preserves_precision_nan_and_signed_zero() {
    let scalar = |v| snapshot(v, vec![]);
    assert_eq!(
        eq(
            &scalar(DecodedValue::Int(9_007_199_254_740_993)),
            &scalar(DecodedValue::Float(9_007_199_254_740_992.0))
        ),
        Ok(Some(false))
    );
    assert_eq!(
        eq(
            &scalar(DecodedValue::Int(7)),
            &scalar(DecodedValue::Bigint(Box::new(7.into())))
        ),
        Ok(Some(true))
    );
    assert_eq!(
        eq(
            &scalar(DecodedValue::Float(f64::NAN)),
            &scalar(DecodedValue::Float(-f64::NAN))
        ),
        Ok(Some(true))
    );
    assert_eq!(
        eq(
            &scalar(DecodedValue::Float(0.0)),
            &scalar(DecodedValue::Float(-0.0))
        ),
        Ok(Some(false))
    );
    assert_eq!(
        eq(
            &scalar(DecodedValue::Int(0)),
            &scalar(DecodedValue::Float(-0.0))
        ),
        Ok(Some(true))
    );
    let large = scalar(DecodedValue::Bigint(Box::new(u64::MAX.into())));
    assert_eq!(
        super::json(
            capture(&large),
            CmpOp::Eq,
            &json!(u64::MAX),
            &Limits::default()
        ),
        Ok(Some(true))
    );
    assert_eq!(
        super::json(
            capture(&large),
            CmpOp::Eq,
            &json!(u64::MAX - 1),
            &Limits::default()
        ),
        Ok(Some(false))
    );
}

#[test]
fn bytes_cells_and_json_literals_compare_without_rendering() {
    let a = snapshot(
        DecodedValue::Object(0),
        vec![DecodedObject::Bytes {
            data: vec![1, 2, 3],
            original_len: 3,
        }],
    );
    assert_eq!(eq(&a, &a), Ok(Some(true)));
    let b = snapshot(
        DecodedValue::Object(0),
        vec![list(vec![DecodedValue::Int(1), DecodedValue::Int(2)])],
    );
    assert_eq!(
        super::json(capture(&b), CmpOp::Eq, &json!([1, 2]), &Limits::default()),
        Ok(Some(true))
    );
    let cell = snapshot(
        DecodedValue::Object(0),
        vec![
            DecodedObject::Cell(DecodedValue::Object(1)),
            map(vec![("answer", DecodedValue::Int(42))]),
        ],
    );
    assert_eq!(
        super::json(
            capture(&cell),
            CmpOp::Eq,
            &json!({"answer": 42}),
            &Limits::default()
        ),
        Ok(Some(true))
    );
}

#[test]
fn cycles_opaque_and_truncated_evidence_never_equal_even_themselves() {
    let cycle = snapshot(
        DecodedValue::Object(0),
        vec![list(vec![DecodedValue::Object(0)])],
    );
    assert_eq!(eq(&cycle, &cycle), Err(Error::Cycle));
    let cells = snapshot(
        DecodedValue::Object(0),
        vec![DecodedObject::Cell(DecodedValue::Object(0))],
    );
    assert_eq!(
        eq(&cells, &cells),
        Err(Error::Evidence(Unavailable::CellCycle))
    );
    let opaque = snapshot(
        DecodedValue::Object(0),
        vec![DecodedObject::NonSnapshotable],
    );
    assert_eq!(eq(&opaque, &opaque), Err(Error::Unsupported));
    let mut cut = snapshot(
        DecodedValue::Object(0),
        vec![list(vec![DecodedValue::Int(1)])],
    );
    let DecodedObject::List { original_len, .. } = &mut cut.objects[0] else {
        panic!()
    };
    *original_len = 2;
    assert_eq!(
        eq(&cut, &cut),
        Err(Error::Evidence(Unavailable::Truncated(
            btel_snapshot::Limit::Values
        )))
    );
    // Unknown siblings are examined even if another field proves inequality.
    let a = snapshot(
        DecodedValue::Object(0),
        vec![
            map(vec![
                ("a", DecodedValue::Int(1)),
                ("b", DecodedValue::Object(1)),
            ]),
            DecodedObject::NonSnapshotable,
        ],
    );
    let b = snapshot(
        DecodedValue::Object(0),
        vec![map(vec![("a", DecodedValue::Int(2))])],
    );
    assert_eq!(eq(&a, &b), Err(Error::Unsupported));
    assert_eq!(eq(&b, &a), Err(Error::Unsupported));
}

#[test]
fn limits_bound_shared_expansion_depth_and_bytes() {
    let a = snapshot(
        DecodedValue::Object(0),
        vec![
            list(vec![DecodedValue::Object(1), DecodedValue::Object(1)]),
            list(vec![DecodedValue::Int(1)]),
        ],
    );
    for limits in [
        Limits {
            max_nodes: 3,
            ..Limits::default()
        },
        Limits {
            max_depth: 1,
            ..Limits::default()
        },
    ] {
        assert_eq!(
            captured(capture(&a), CmpOp::Eq, capture(&a), &limits),
            Err(Error::Limit)
        );
    }
    let text = snapshot(DecodedValue::String("long".into()), vec![]);
    assert_eq!(
        captured(
            capture(&text),
            CmpOp::Eq,
            capture(&text),
            &Limits {
                max_bytes: 7,
                ..Limits::default()
            }
        ),
        Err(Error::Limit)
    );
}

#[test]
fn whole_arguments_require_complete_names_and_preserve_omitted_slots() {
    let s = DecodedSnapshot {
        root: DecodedRoot::FunctionArgs {
            parameter_count: 1,
            slots: vec![DecodedValue::Int(7)],
        },
        ..snapshot(DecodedValue::Null, vec![])
    };
    let names = ArgumentNames {
        slots: vec![SlotName {
            name: Some("n".into()),
            receiver: false,
        }],
    };
    let mut a = Captured {
        snapshot: &s,
        nav: Nav::Arguments,
        names: Some(&names),
    };
    assert_eq!(
        super::json(a, CmpOp::Eq, &json!({"n":7}), &Limits::default()),
        Ok(Some(true))
    );
    a.names = None;
    assert_eq!(
        super::json(a, CmpOp::Eq, &json!({"n":7}), &Limits::default()),
        Err(Error::Evidence(Unavailable::ArgumentNamesUnknown))
    );
}
