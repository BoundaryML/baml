//! Roundtrip coverage for `baml_sdk::literals` — literal Ty variants.
//!
//! Float literals are intentionally absent (Python `Literal` rejects
//! floats). The negative-literal-as-field case is still parser-blocked, but
//! the function-return form `return_literal_neg_one() -> -1` emits and is
//! exercised here.

// PROVISIONAL: literal types have no Rust SDK design yet. This port assumes
// they surface as their base primitive types (i64 / String / bool) in
// generated signatures and struct fields.
use baml_sdk::literals::{
    Literals, return_literal_draft_sync, return_literal_escaped_sync, return_literal_false_sync,
    return_literal_neg_one_sync, return_literal_true_sync, return_literal42_sync,
    round_trip_literal_draft_sync, round_trip_literal_escaped_sync, round_trip_literal_false_sync,
    round_trip_literal_true_sync, round_trip_literal42_sync, round_trip_literals_sync,
};

#[test]
fn test_literals_return_literals() {
    assert_eq!(return_literal42_sync().unwrap(), 42);
    assert_eq!(return_literal_neg_one_sync().unwrap(), -1);
    assert_eq!(return_literal_draft_sync().unwrap(), "draft");
    assert_eq!(return_literal_escaped_sync().unwrap(), "has \"quotes\"");
    assert!(return_literal_true_sync().unwrap());
    assert!(!return_literal_false_sync().unwrap());
}

#[test]
fn test_literals_round_trip_literal42() {
    assert_eq!(round_trip_literal42_sync(42).unwrap(), 42);
}

#[test]
fn test_literals_round_trip_literal_draft() {
    assert_eq!(
        round_trip_literal_draft_sync("draft".to_string()).unwrap(),
        "draft"
    );
}

#[test]
fn test_literals_round_trip_literal_escaped() {
    assert_eq!(
        round_trip_literal_escaped_sync("has \"quotes\"".to_string()).unwrap(),
        "has \"quotes\""
    );
}

#[test]
fn test_literals_round_trip_literal_true() {
    assert!(round_trip_literal_true_sync(true).unwrap());
}

#[test]
fn test_literals_round_trip_literal_false() {
    assert!(!round_trip_literal_false_sync(false).unwrap());
}

#[test]
fn test_literals_round_trip_literals() {
    let lit = Literals {
        literal_42: 42,
        literal_draft: "draft".to_string(),
        literal_escaped: "has \"quotes\"".to_string(),
        literal_true: true,
        literal_false: false,
    };
    assert_eq!(round_trip_literals_sync(lit.clone()).unwrap(), lit);
}
