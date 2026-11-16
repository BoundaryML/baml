//! Roundtrip coverage for the symbol-collision suite — three distinct
//! `Bar` classes at different namespace depths plus the consumers
//! (`Ipsum`, `Deep`) that compose all three.

use baml_sdk::symbol_collisions::a::b::c::d::{make_deep_sync, round_trip_deep_sync};
use baml_sdk::symbol_collisions::fizz::buzz::foo::{
    make_fizz_buzz_foo_bar_sync, round_trip_fizz_buzz_foo_bar_sync,
};
use baml_sdk::symbol_collisions::fizz::foo::{
    make_fizz_foo_bar_sync, round_trip_fizz_foo_bar_sync,
};
use baml_sdk::symbol_collisions::foo::{make_foo_bar_sync, round_trip_foo_bar_sync};
use baml_sdk::symbol_collisions::lorem::{make_ipsum_sync, round_trip_ipsum_sync};

#[test]
fn test_symbol_collisions_round_trip_foo_bar() {
    let bar = make_foo_bar_sync("hi".to_string(), 2).unwrap();
    assert_eq!(round_trip_foo_bar_sync(bar.clone()).unwrap(), bar);
}

#[test]
fn test_symbol_collisions_round_trip_fizz_foo_bar() {
    let bar = make_fizz_foo_bar_sync("t".to_string(), 1.5).unwrap();
    assert_eq!(round_trip_fizz_foo_bar_sync(bar.clone()).unwrap(), bar);
}

#[test]
fn test_symbol_collisions_round_trip_fizz_buzz_foo_bar() {
    let bar = make_fizz_buzz_foo_bar_sync("f".to_string(), 2.5, true).unwrap();
    assert_eq!(round_trip_fizz_buzz_foo_bar_sync(bar.clone()).unwrap(), bar);
}

#[test]
fn test_symbol_collisions_round_trip_ipsum() {
    let ipsum = make_ipsum_sync(
        make_foo_bar_sync("a".to_string(), 1).unwrap(),
        make_fizz_foo_bar_sync("b".to_string(), 2.0).unwrap(),
        make_fizz_buzz_foo_bar_sync("c".to_string(), 3.0, false).unwrap(),
    )
    .unwrap();
    assert_eq!(round_trip_ipsum_sync(ipsum.clone()).unwrap(), ipsum);
}

#[test]
fn test_symbol_collisions_round_trip_deep() {
    let ipsum = make_ipsum_sync(
        make_foo_bar_sync("a".to_string(), 1).unwrap(),
        make_fizz_foo_bar_sync("b".to_string(), 2.0).unwrap(),
        make_fizz_buzz_foo_bar_sync("c".to_string(), 3.0, false).unwrap(),
    )
    .unwrap();
    let deep = make_deep_sync(
        make_foo_bar_sync("h".to_string(), 9).unwrap(),
        make_fizz_foo_bar_sync("th".to_string(), 4.0).unwrap(),
        make_fizz_buzz_foo_bar_sync("fu".to_string(), 5.0, true).unwrap(),
        ipsum,
    )
    .unwrap();
    assert_eq!(round_trip_deep_sync(deep.clone()).unwrap(), deep);
}
