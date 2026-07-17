//! Roundtrip coverage for `baml_sdk::forward_refs` — forward references.
//!
//! `round_trip_node` is intentionally NOT exercised: `class Node { next Node }`
//! has a *required* (non-optional) self-reference, so no finite value can be
//! constructed from the host side. It still emits and type-checks; the
//! import below proves the symbol exists.

use baml_sdk::forward_refs::round_trip_node as _; // uninhabitable (required self-ref); import-only
use baml_sdk::forward_refs::{GNode, Other, round_trip_g_node_int, round_trip_other};

#[test]
fn test_round_trip_other() {
    let o = Other { v: 7 };
    assert_eq!(round_trip_other(o.clone()).unwrap(), o);
}

#[test]
fn test_round_trip_rec_list() {
    // DIVERGENCE(rust): `RecList = int | RecList[]` is a recursive *union*
    // alias — unions have no generated Rust representation yet, and
    // recursive aliases are skipped by codegen initially, so this round trip
    // is inexpressible. Intended body once both exist (`RecList` as the
    // generated recursive union):
    //
    //     let r = RecList::List(vec![
    //         RecList::Int(1),
    //         RecList::List(vec![RecList::Int(2), RecList::Int(3)]),
    //     ]);
    //     assert_eq!(round_trip_rec_list(r.clone()).unwrap(), r);
}

#[test]
fn test_round_trip_rec_list_with_other() {
    // RecListWithOther = int | Other | RecListWithOther[]
    //
    // DIVERGENCE(rust): a recursive *union* alias — same story as
    // `test_round_trip_rec_list` above. Intended body once unions and
    // recursive aliases exist:
    //
    //     assert_eq!(
    //         round_trip_rec_list_with_other(RecListWithOther::Int(1)).unwrap(),
    //         RecListWithOther::Int(1)
    //     );
    //     let r = RecListWithOther::List(vec![
    //         RecListWithOther::Int(1),
    //         RecListWithOther::Int(2),
    //     ]);
    //     assert_eq!(round_trip_rec_list_with_other(r.clone()).unwrap(), r);
}

#[test]
fn test_round_trip_g_node_int() {
    // The leaf node carries `children: vec![]`; this exercises the empty-list
    // round trip (see test_lists::test_round_trip_empty_list).
    let g = GNode::<i64> {
        children: vec![GNode::<i64> { children: vec![] }],
    };
    assert_eq!(round_trip_g_node_int(g.clone()).unwrap(), g);
}
