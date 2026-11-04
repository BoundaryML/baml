//! Roundtrip coverage for `baml_sdk::optional` — optional Ty variants.

use baml_sdk::optional::{
    IntOrString, OptionalContainer, Resume, round_trip_optional_container_sync,
    round_trip_optional_int_sync, round_trip_optional_resume_sync, round_trip_optional_union_sync,
    round_trip_resume_sync,
};

#[test]
fn test_optional_round_trip_optional_int() {
    assert_eq!(round_trip_optional_int_sync(Some(5)).unwrap(), Some(5));
    assert_eq!(round_trip_optional_int_sync(None).unwrap(), None);
}

#[test]
fn test_optional_round_trip_optional_resume() {
    let r = Resume {
        name: "ada".to_string(),
    };
    assert_eq!(
        round_trip_optional_resume_sync(Some(r.clone())).unwrap(),
        Some(r)
    );
    assert_eq!(round_trip_optional_resume_sync(None).unwrap(), None);
}

#[test]
fn test_optional_round_trip_optional_union() {
    assert_eq!(
        round_trip_optional_union_sync(Some(IntOrString::Int(3))).unwrap(),
        Some(IntOrString::Int(3))
    );
    assert_eq!(
        round_trip_optional_union_sync(Some(IntOrString::String("s".to_string()))).unwrap(),
        Some(IntOrString::String("s".to_string()))
    );
    assert_eq!(round_trip_optional_union_sync(None).unwrap(), None);
}

#[test]
fn test_optional_round_trip_resume() {
    let r = Resume {
        name: "grace".to_string(),
    };
    assert_eq!(round_trip_resume_sync(r.clone()).unwrap(), r);
}

#[test]
fn test_optional_round_trip_optional_container() {
    let c = OptionalContainer {
        optional_int: None,
        optional_class: Some(Resume {
            name: "x".to_string(),
        }),
        optional_union: Some(IntOrString::String("y".to_string())),
    };
    assert_eq!(round_trip_optional_container_sync(c.clone()).unwrap(), c);
}
