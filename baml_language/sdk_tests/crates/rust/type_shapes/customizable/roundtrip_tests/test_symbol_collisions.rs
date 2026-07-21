//! Roundtrip coverage for the symbol-collision suite — three distinct
//! `Bar` classes at different namespace depths plus the consumers
//! (`Ipsum`, `Deep`) that compose all three.

use baml_sdk::symbol_collisions::a::b::c::d::{make_deep, round_trip_deep};
use baml_sdk::symbol_collisions::fizz::buzz::foo::{
    make_fizz_buzz_foo_bar, round_trip_fizz_buzz_foo_bar,
};
use baml_sdk::symbol_collisions::fizz::foo::{make_fizz_foo_bar, round_trip_fizz_foo_bar};
use baml_sdk::symbol_collisions::foo::{make_foo_bar, round_trip_foo_bar};
use baml_sdk::symbol_collisions::lorem::{make_ipsum, round_trip_ipsum};

#[test]
fn test_symbol_collisions_round_trip_foo_bar() {
    let bar = make_foo_bar("hi".to_string(), 2).unwrap();
    assert_eq!(round_trip_foo_bar(bar.clone()).unwrap(), bar);
}

#[test]
fn test_symbol_collisions_round_trip_fizz_foo_bar() {
    let bar = make_fizz_foo_bar("t".to_string(), 1.5).unwrap();
    assert_eq!(round_trip_fizz_foo_bar(bar.clone()).unwrap(), bar);
}

#[test]
fn test_symbol_collisions_round_trip_fizz_buzz_foo_bar() {
    let bar = make_fizz_buzz_foo_bar("f".to_string(), 2.5, true).unwrap();
    assert_eq!(round_trip_fizz_buzz_foo_bar(bar.clone()).unwrap(), bar);
}

#[test]
fn test_symbol_collisions_round_trip_ipsum() {
    let ipsum = make_ipsum(
        make_foo_bar("a".to_string(), 1).unwrap(),
        make_fizz_foo_bar("b".to_string(), 2.0).unwrap(),
        make_fizz_buzz_foo_bar("c".to_string(), 3.0, false).unwrap(),
    )
    .unwrap();
    assert_eq!(round_trip_ipsum(ipsum.clone()).unwrap(), ipsum);
}

#[test]
fn test_symbol_collisions_round_trip_deep() {
    let ipsum = make_ipsum(
        make_foo_bar("a".to_string(), 1).unwrap(),
        make_fizz_foo_bar("b".to_string(), 2.0).unwrap(),
        make_fizz_buzz_foo_bar("c".to_string(), 3.0, false).unwrap(),
    )
    .unwrap();
    let deep = make_deep(
        make_foo_bar("h".to_string(), 9).unwrap(),
        make_fizz_foo_bar("th".to_string(), 4.0).unwrap(),
        make_fizz_buzz_foo_bar("fu".to_string(), 5.0, true).unwrap(),
        ipsum,
    )
    .unwrap();
    assert_eq!(round_trip_deep(deep.clone()).unwrap(), deep);
}
