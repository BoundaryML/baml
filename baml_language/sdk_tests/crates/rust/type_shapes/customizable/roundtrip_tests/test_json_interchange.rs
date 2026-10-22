//! The generated SDK's optional `serde` impls agree with BAML's own JSON.
//!
//! `to_json` is `baml.json.to_string` and `from_json` is
//! `baml.json.from_string<T>`, so each check runs serde_json and BAML on the
//! same value or the same text. Rust-only: the other SDKs have no `serde`
//! feature.

use std::fmt::Debug;

use baml_bridge::{
    BamlValue, Map,
    serde_json::{self, json},
};
use baml_sdk::json_interchange::{
    FastOrSlow, IntOrFloat, IntOrLabel, JsonDoc, JsonFields, Label, Shade, from_json, to_json,
};
use serde::{Serialize, de::DeserializeOwned};

fn label(text: &str, note: Option<&str>) -> Label {
    Label {
        text: text.to_string(),
        note: note.map(str::to_string),
    }
}

fn doc() -> JsonDoc {
    JsonDoc {
        title: "t".to_string(),
        count: 3,
        ratio: 1.0,
        done: true,
        shade: Shade::Dark,
        subtitle: None,
        tags: vec!["a".to_string(), "b".to_string()],
        scores: Map::from_iter([("z".to_string(), 1), ("a".to_string(), 2)]),
        label: label("x", Some("n")),
        labels: vec![label("y", None)],
        amount: IntOrFloat::Float(2.5),
        pick: IntOrLabel::Label(label("p", None)),
        speed: FastOrSlow::Slow,
        max_speed: Some(FastOrSlow::Fast),
        r#type: "kind".to_string(),
        crate_: 7,
    }
}

/// A JSON object for `JsonDoc` with every optional field absent.
fn sparse_doc_json() -> serde_json::Value {
    serde_json::json!({
        "title": "t", "count": 3, "ratio": 1, "done": false, "shade": "Light",
        "tags": [], "scores": {}, "label": {"text": "x"}, "labels": [],
        "amount": 1, "pick": {"text": "p"}, "speed": "fast", "type": "k", "crate": 0,
    })
}

/// serde_json writes the same text as BAML, and each reads the other's back.
fn assert_serialize_agrees<T>(value: &T) -> String
where
    T: BamlValue + Clone + PartialEq + Debug + Serialize + DeserializeOwned,
{
    let baml = to_json(value.clone()).unwrap();
    let serde = serde_json::to_string(value).unwrap();
    assert_eq!(serde, baml);
    assert_eq!(&serde_json::from_str::<T>(&baml).unwrap(), value);
    assert_eq!(&from_json::<T>(serde.clone()).unwrap(), value);
    serde
}

/// serde_json reads `json` to the same value BAML does.
fn assert_deserialize_agrees<T>(json: &str) -> T
where
    T: BamlValue + PartialEq + Debug + DeserializeOwned,
{
    let baml = from_json::<T>(json.to_string()).unwrap();
    let serde = serde_json::from_str::<T>(json).unwrap();
    assert_eq!(serde, baml);
    serde
}

// SDK_PARITY_LINT(skip): validates the Rust SDK's optional serde impls
#[test]
fn test_json_interchange_serialize_matches_baml() {
    // BAML field names (the keyword ones too), the enum variant name, an
    // untagged union, `null` for `None`, and map insertion order.
    assert_eq!(
        assert_serialize_agrees(&doc()),
        r#"{"title":"t","count":3,"ratio":1.0,"done":true,"shade":"Dark","subtitle":null,"tags":["a","b"],"scores":{"z":1,"a":2},"label":{"text":"x","note":"n"},"labels":[{"text":"y","note":null}],"amount":2.5,"pick":{"text":"p","note":null},"speed":"slow","max_speed":"fast","type":"kind","crate":7}"#
    );
    // The other union arms, and an absent optional union.
    assert_serialize_agrees(&JsonDoc {
        amount: IntOrFloat::Int(4),
        pick: IntOrLabel::Int(9),
        speed: FastOrSlow::Fast,
        max_speed: None,
        ..doc()
    });
}

// SDK_PARITY_LINT(skip): validates the Rust SDK's optional serde impls
#[test]
fn test_json_interchange_serialize_matches_baml_outside_a_class() {
    assert_eq!(assert_serialize_agrees(&Shade::Light), r#""Light""#);
    assert_eq!(assert_serialize_agrees(&FastOrSlow::Fast), r#""fast""#);
    assert_eq!(assert_serialize_agrees(&IntOrFloat::Float(0.5)), "0.5");
    assert_serialize_agrees(&vec![label("a", None), label("b", Some("c"))]);
    assert_serialize_agrees(&Map::from_iter([
        ("k".to_string(), label("v", None)),
        ("j".to_string(), label("w", Some("x"))),
    ]));
    assert_eq!(assert_serialize_agrees(&None::<Label>), "null");
}

// SDK_PARITY_LINT(skip): validates the Rust SDK's optional serde impls
#[test]
fn test_json_interchange_optional_fields_may_be_absent_or_null() {
    let absent = assert_deserialize_agrees::<JsonDoc>(&sparse_doc_json().to_string());
    assert_eq!(absent.subtitle, None);
    assert_eq!(absent.max_speed, None);
    assert_eq!(absent.label.note, None);

    let mut nulls = sparse_doc_json();
    nulls["subtitle"] = serde_json::Value::Null;
    nulls["max_speed"] = serde_json::Value::Null;
    nulls["label"]["note"] = serde_json::Value::Null;
    assert_eq!(
        assert_deserialize_agrees::<JsonDoc>(&nulls.to_string()),
        absent
    );

    let mut present = sparse_doc_json();
    present["subtitle"] = "s".into();
    present["max_speed"] = "slow".into();
    let present = assert_deserialize_agrees::<JsonDoc>(&present.to_string());
    assert_eq!(present.subtitle.as_deref(), Some("s"));
    assert_eq!(present.max_speed, Some(FastOrSlow::Slow));
}

// SDK_PARITY_LINT(skip): validates the Rust SDK's optional serde impls
#[test]
fn test_json_interchange_deserialize_matches_baml() {
    let mut json = sparse_doc_json();
    json["scores"] = serde_json::json!({"z": 1, "a": 2});
    json["labels"] = serde_json::json!([{"text": "y"}, {"text": "w", "note": "v"}]);
    json["unknown_key"] = "ignored".into();
    let value = assert_deserialize_agrees::<JsonDoc>(&json.to_string());
    assert_eq!(value.ratio, 1.0);
    assert_eq!(value.shade, Shade::Light);
    assert_eq!(value.scores.keys().collect::<Vec<_>>(), ["z", "a"]);
    assert_eq!(value.labels, [label("y", None), label("w", Some("v"))]);
    assert_eq!(value.r#type, "k");
    assert_eq!(value.crate_, 0);
}

// SDK_PARITY_LINT(skip): validates the Rust SDK's optional serde impls
#[test]
fn test_json_interchange_unions_take_the_first_arm_that_fits() {
    assert_eq!(
        assert_deserialize_agrees::<IntOrFloat>("1"),
        IntOrFloat::Int(1)
    );
    assert_eq!(
        assert_deserialize_agrees::<IntOrFloat>("1.5"),
        IntOrFloat::Float(1.5)
    );
    assert_eq!(
        assert_deserialize_agrees::<IntOrLabel>("5"),
        IntOrLabel::Int(5)
    );
    assert_eq!(
        assert_deserialize_agrees::<IntOrLabel>(r#"{"text":"p","extra":1}"#),
        IntOrLabel::Label(label("p", None))
    );
    assert_eq!(
        assert_deserialize_agrees::<FastOrSlow>(r#""slow""#),
        FastOrSlow::Slow
    );
}

// SDK_PARITY_LINT(skip): validates the Rust SDK's optional serde impls
#[test]
fn test_json_interchange_json_fields_match_baml() {
    // Object keys are written sorted: `serde_json::Map` sorts them unless
    // `serde_json`'s `preserve_order` feature is on, and BAML keeps the
    // order it received.
    let value = JsonFields {
        payload: json!({
            "a": [null, true, -3, 2.5, 2.0, "s"],
            "b": {"c": {}},
            "wide": [i64::MIN, u64::MAX],
        }),
        extra: None,
        items: vec![json!(1), json!("x"), json!([])],
    };
    assert_eq!(
        assert_serialize_agrees(&value),
        r#"{"payload":{"a":[null,true,-3,2.5,2.0,"s"],"b":{"c":{}},"wide":[-9223372036854775808,18446744073709551615]},"extra":null,"items":[1,"x",[]]}"#
    );
    assert_serialize_agrees(&JsonFields {
        extra: Some(json!({"k": "v"})),
        ..value
    });

    // A `json?` field reads `null` and an absent key as `None`.
    for text in [
        r#"{"payload":7,"items":[]}"#,
        r#"{"payload":7,"extra":null,"items":[]}"#,
    ] {
        let read = assert_deserialize_agrees::<JsonFields>(text);
        assert_eq!(read.extra, None);
    }
    // A `json` field is required even though `null` is a json value.
    let missing = r#"{"items":[]}"#;
    assert!(from_json::<JsonFields>(missing.to_string()).is_err());
    assert!(serde_json::from_str::<JsonFields>(missing).is_err());
}

// SDK_PARITY_LINT(skip): validates the Rust SDK's optional serde impls
#[test]
fn test_json_interchange_both_reject_what_baml_rejects() {
    let mut cases = Vec::new();
    for (key, bad) in [
        ("count", serde_json::json!("3")),
        ("count", serde_json::json!(1.5)),
        ("shade", serde_json::json!("Grey")),
        ("speed", serde_json::json!("medium")),
        ("amount", serde_json::json!("1")),
        ("pick", serde_json::json!({"note": "no text"})),
        ("tags", serde_json::json!("a")),
        ("scores", serde_json::json!({"a": "1"})),
    ] {
        let mut json = sparse_doc_json();
        json[key] = bad;
        cases.push(json);
    }
    let mut missing = sparse_doc_json();
    missing.as_object_mut().unwrap().remove("title");
    cases.push(missing);

    for json in cases {
        let text = json.to_string();
        assert!(
            from_json::<JsonDoc>(text.clone()).is_err(),
            "BAML read {text}"
        );
        assert!(
            serde_json::from_str::<JsonDoc>(&text).is_err(),
            "serde read {text}"
        );
    }
}
