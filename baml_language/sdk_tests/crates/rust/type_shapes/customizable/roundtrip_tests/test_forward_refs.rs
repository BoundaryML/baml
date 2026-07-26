//! Roundtrip coverage for `baml_sdk::forward_refs` — forward references.
//!
//! `round_trip_node` is intentionally NOT exercised: `class Node { next Node }`
//! has a *required* (non-optional) self-reference, so no finite value can be
//! constructed from the host side. It still emits and type-checks; the
//! import below proves the symbol exists.

use baml_sdk::forward_refs::round_trip_node as _; // uninhabitable (required self-ref); import-only
use baml_sdk::forward_refs::{Other, round_trip_other};

#[test]
fn test_forward_refs_round_trip_other() {
    let o = Other { v: 7 };
    assert_eq!(round_trip_other(o.clone()).unwrap(), o);
}

#[test]
fn test_forward_refs_round_trip_rec_list() {
    // DIVERGENCE(rust): `RecList = int | RecList[]` is a *recursive* alias —
    // not representable as a plain Rust `type`, so codegen skips it (fail
    // closed) and this round trip is inexpressible. Intended body if a
    // dedicated recursive-alias representation ever lands (`RecList` as the
    // generated recursive union):
    //
    //     let r = RecList::List(vec![
    //         RecList::Int(1),
    //         RecList::List(vec![RecList::Int(2), RecList::Int(3)]),
    //     ]);
    //     assert_eq!(round_trip_rec_list(r.clone()).unwrap(), r);
}

#[test]
fn test_forward_refs_round_trip_rec_list_with_other() {
    // RecListWithOther = int | Other | RecListWithOther[]
    //
    // DIVERGENCE(rust): a recursive alias — same story as
    // `test_round_trip_rec_list` above. Intended body should a recursive
    // -alias representation land:
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
fn test_forward_refs_round_trip_g_node_int() {
    // DIVERGENCE(rust): `GNode<T> { children: GNode<T>[] }` uses its type
    // parameter ONLY inside its recursive self-reference, which Rust rejects
    // outright ("type parameter `T` is only used recursively" — the param's
    // variance is undeterminable, and the only escape is a `PhantomData`
    // field leaking into the public struct literal). Codegen skips the class
    // (fail closed) and `round_trip_g_node_int` with it, so this round trip
    // is permanently inexpressible from Rust. Python's body:
    //
    //     g = GNode[int](children=[GNode[int](children=[])])
    //     assert round_trip_g_node_int(g) == g
}
