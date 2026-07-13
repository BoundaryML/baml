//! Roundtrip coverage for `baml_sdk::optional` — optional Ty variants.

use baml_sdk::optional::{
    Resume, round_trip_optional_int, round_trip_optional_resume, round_trip_resume,
};

#[test]
fn test_round_trip_optional_int() {
    assert_eq!(round_trip_optional_int(Some(5)).unwrap(), Some(5));
    assert_eq!(round_trip_optional_int(None).unwrap(), None);
}

#[test]
fn test_round_trip_optional_resume() {
    let r = Resume {
        name: "ada".to_string(),
    };
    assert_eq!(
        round_trip_optional_resume(Some(r.clone())).unwrap(),
        Some(r)
    );
    assert_eq!(round_trip_optional_resume(None).unwrap(), None);
}

#[test]
fn test_round_trip_optional_union() {
    // DIVERGENCE(rust): `(int | string)?` — unions have no generated Rust
    // representation yet, so this round trip is inexpressible. Intended body
    // once unions exist (`IntOrString` stands in for the generated
    // `(int | string)` union):
    //
    //     assert_eq!(
    //         round_trip_optional_union(Some(IntOrString::Int(3))).unwrap(),
    //         Some(IntOrString::Int(3))
    //     );
    //     assert_eq!(
    //         round_trip_optional_union(Some(IntOrString::String("s".to_string()))).unwrap(),
    //         Some(IntOrString::String("s".to_string()))
    //     );
    //     assert_eq!(round_trip_optional_union(None).unwrap(), None);
}

#[test]
fn test_round_trip_resume() {
    let r = Resume {
        name: "grace".to_string(),
    };
    assert_eq!(round_trip_resume(r.clone()).unwrap(), r);
}

#[test]
fn test_round_trip_optional_container() {
    // DIVERGENCE(rust): `OptionalContainer.optional_union` is
    // `(int | string)?` and unions have no generated Rust representation
    // yet, so the struct cannot be constructed. Intended body once unions
    // exist:
    //
    //     let c = OptionalContainer {
    //         optional_int: None,
    //         optional_class: Some(Resume { name: "x".to_string() }),
    //         optional_union: Some(IntOrString::String("y".to_string())),
    //     };
    //     assert_eq!(round_trip_optional_container(c.clone()).unwrap(), c);
}
