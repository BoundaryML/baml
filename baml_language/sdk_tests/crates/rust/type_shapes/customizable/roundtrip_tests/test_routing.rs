//! Roundtrip coverage for the cross-namespace routing-rules suite:
//! root (`baml_sdk`), `a`, `a::b`, `lorem`, and `ipsum` leaves.
//!
//! The `baml.http.Response`-typed round trips in `lorem` are covered in
//! `test_streams.rs` (they need an engine-minted handle and can't be built
//! host-side).

use baml_sdk::a::b::{Thing, round_trip_root_foo_from_ab, round_trip_thing_from_ab};
use baml_sdk::a::round_trip_deep_thing_from_a;
use baml_sdk::ipsum::round_trip_lorem_resume_from_ipsum;
use baml_sdk::lorem::{
    Resume, round_trip_deep_thing_from_lorem, round_trip_resume, round_trip_root_foo,
};
use baml_sdk::{Foo, make_foo, round_trip_foo};

#[test]
fn test_routing_make_foo() {
    assert_eq!(make_foo(3).unwrap().v, 3);
}

#[test]
fn test_routing_round_trip_foo() {
    let f = Foo { v: 10 };
    assert_eq!(round_trip_foo(f.clone()).unwrap(), f);
}

#[test]
fn test_routing_round_trip_thing_from_ab() {
    let t = Thing { v: 1 };
    assert_eq!(round_trip_thing_from_ab(t.clone()).unwrap(), t);
}

#[test]
fn test_routing_round_trip_root_foo_from_ab() {
    let f = Foo { v: 2 };
    assert_eq!(round_trip_root_foo_from_ab(f.clone()).unwrap(), f);
}

#[test]
fn test_routing_round_trip_deep_thing_from_a() {
    let t = Thing { v: 4 };
    assert_eq!(round_trip_deep_thing_from_a(t.clone()).unwrap(), t);
}

#[test]
fn test_routing_round_trip_deep_thing_from_lorem() {
    let t = Thing { v: 5 };
    assert_eq!(round_trip_deep_thing_from_lorem(t.clone()).unwrap(), t);
}

#[test]
fn test_routing_round_trip_resume() {
    let r = Resume {
        name: "ada".to_string(),
        email: None,
    };
    assert_eq!(round_trip_resume(r.clone()).unwrap(), r);
}

#[test]
fn test_routing_round_trip_root_foo() {
    let f = Foo { v: 6 };
    assert_eq!(round_trip_root_foo(f.clone()).unwrap(), f);
}

#[test]
fn test_routing_round_trip_lorem_resume_from_ipsum() {
    let r = Resume {
        name: "grace".to_string(),
        email: Some("g@x.com".to_string()),
    };
    assert_eq!(round_trip_lorem_resume_from_ipsum(r.clone()).unwrap(), r);
}
