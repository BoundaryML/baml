//! Roundtrip coverage for `baml_sdk::aliases` — type aliases (incl. recursive).

use baml_sdk::aliases::round_trip_string_list;

#[test]
fn test_aliases_round_trip_string_list() {
    assert_eq!(
        round_trip_string_list(vec!["a".to_string(), "b".to_string()]).unwrap(),
        vec!["a".to_string(), "b".to_string()]
    );
}

#[test]
fn test_aliases_round_trip_rec_list() {
    // RecList = int | RecList[]
    //
    // DIVERGENCE(rust): `RecList` is a recursive *union* alias — unions have
    // no generated Rust representation yet, and recursive aliases are
    // skipped by codegen initially, so this round trip is inexpressible.
    // Intended body once both exist (`RecList` as the generated recursive
    // union):
    //
    //     assert_eq!(round_trip_rec_list(RecList::Int(1)).unwrap(), RecList::Int(1));
    //     let r = RecList::List(vec![
    //         RecList::Int(1),
    //         RecList::List(vec![RecList::Int(2), RecList::Int(3)]),
    //     ]);
    //     assert_eq!(round_trip_rec_list(r.clone()).unwrap(), r);
}

#[test]
fn test_aliases_round_trip_alias_container() {
    // DIVERGENCE(rust): `AliasContainer.rec_field` is the recursive union
    // alias `RecList` (see above), so the struct cannot be constructed yet.
    // Intended body once unions and recursive aliases exist:
    //
    //     let c = AliasContainer {
    //         list_field: vec!["x".to_string()],
    //         rec_field: RecList::List(vec![
    //             RecList::Int(1),
    //             RecList::List(vec![RecList::Int(2)]),
    //         ]),
    //     };
    //     assert_eq!(round_trip_alias_container(c.clone()).unwrap(), c);
}
