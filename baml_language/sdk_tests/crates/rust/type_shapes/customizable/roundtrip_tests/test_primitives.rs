//! Roundtrip coverage for `baml_sdk::primitives`.
//!
//! Calls each emitted BAML function for the primitive Ty variants from
//! Rust and asserts the value survives the encode → FFI → decode round
//! trip. `return_*` functions exercise decode-only; `round_trip_*` exercise
//! the full encode/decode pair.

use baml_sdk::primitives::{
    Primitives, return_bool, return_float, return_int, return_null, return_string, round_trip_bool,
    round_trip_float, round_trip_int, round_trip_null, round_trip_primitives, round_trip_string,
    round_trip_uint8_array,
};

#[test]
fn test_primitives_return_int() {
    assert_eq!(return_int().unwrap(), 42);
}

#[test]
#[expect(
    clippy::approx_constant,
    reason = "3.14 is the fixture's literal, not a stand-in for PI"
)]
fn test_primitives_return_float() {
    assert_eq!(return_float().unwrap(), 3.14);
}

#[test]
fn test_primitives_return_string() {
    assert_eq!(return_string().unwrap(), "hello");
}

#[test]
fn test_primitives_return_bool() {
    assert!(return_bool().unwrap());
}

#[test]
fn test_primitives_return_null() {
    // `-> null` lowers to `()`; the successful unwrap is the `is None`
    // assertion.
    return_null().unwrap();
}

#[test]
fn test_primitives_round_trip_int() {
    assert_eq!(round_trip_int(7).unwrap(), 7);
}

#[test]
fn test_primitives_round_trip_float() {
    assert_eq!(round_trip_float(2.5).unwrap(), 2.5);
}

#[test]
fn test_primitives_round_trip_float_accepts_int() {
    // A python int into a float param widens at the FFI boundary (the wire
    // encoder is value-shaped, so `7` arrives engine-side as an int). The
    // declared `-> float` must hand back a genuine float, not the int riding
    // through unconverted.
    //
    // DIVERGENCE(rust): the typed wrapper's parameter is `f64`, so an int on
    // the wire is inexpressible through it. Go through the low-level invoke
    // API instead so the wire-level intent (int out, float back) is
    // preserved.
    let int_on_the_wire = baml_bridge::wire::InboundValue {
        value_type: None,
        value: Some(baml_bridge::wire::inbound_value::Value::IntValue(7)),
    };
    let result = baml_bridge::runtime::invoke_sync::<f64, core::convert::Infallible>(
        "user.primitives.round_trip_float",
        baml_bridge::encode::kwargs(vec![("x", Some(int_on_the_wire))]),
        vec![],
    )
    .unwrap();
    // Python's `isinstance(result, float)` collapses into the static `f64`
    // return type.
    assert_eq!(result, 7.0);
}

#[test]
fn test_primitives_round_trip_string() {
    assert_eq!(round_trip_string("hi".to_string()).unwrap(), "hi");
}

#[test]
fn test_primitives_round_trip_bool() {
    assert!(!round_trip_bool(false).unwrap());
}

#[test]
fn test_primitives_round_trip_null() {
    // Explicit `None` for the `null`-typed param is `()`; the successful
    // unwrap is the `is None` assertion.
    round_trip_null(()).unwrap();
}

#[test]
fn test_primitives_round_trip_uint8_array() {
    assert_eq!(
        round_trip_uint8_array(b"\x00\x01\x02".to_vec()).unwrap(),
        b"\x00\x01\x02".to_vec()
    );
}

#[test]
fn test_primitives_round_trip_primitives() {
    let p = Primitives {
        int_field: 1,
        float_field: 1.5,
        string_field: "s".to_string(),
        bool_field: true,
        null_field: (),
        uint8array_field: b"ab".to_vec(),
    };
    assert_eq!(round_trip_primitives(p.clone()).unwrap(), p);
}

#[test]
fn test_primitives_round_trip_primitives_float_field_accepts_int() {
    // An int into a float *field* is coerced by pydantic at construction, so
    // it reaches the wire as a float already — pin that contract alongside
    // the param-level widening above.
    //
    // DIVERGENCE(rust): construction-time coercion is meaningless here — the
    // struct field is statically `f64` and an integer literal would not
    // compile. Pin only what is expressible: constructing the struct with a
    // whole-valued `f64` field and round-tripping it.
    let p = Primitives {
        int_field: 1,
        float_field: 2.0,
        string_field: "s".to_string(),
        bool_field: true,
        null_field: (),
        uint8array_field: b"ab".to_vec(),
    };
    let result = round_trip_primitives(p).unwrap();
    assert_eq!(result.float_field, 2.0);
}
