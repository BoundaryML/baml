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
    // An integral input to a `-> float` function comes back as a genuine
    // float. Rust widens at the call site (`f64::from`), so the wire already
    // carries `7.0`, as Java's and C++'s call sites do.
    let result = round_trip_float(f64::from(7_i32)).unwrap();
    assert_eq!(result.to_bits(), 7.0_f64.to_bits());
}

// SDK_PARITY_LINT(skip): only the low-level Rust entry point can put an int on the wire where a float is declared
#[test]
fn test_primitives_round_trip_float_refuses_an_int_on_the_wire() {
    // The engine converts nothing at the boundary: what a host means by `7`
    // where a `float` is declared is the host's to say before the value is
    // sent. An int that reaches the wire as an int is no float, and the
    // call is refused.
    //
    // The low-level entry point does not load the generated SDK's program;
    // a generated binding does.
    assert_eq!(round_trip_float(2.5).unwrap().to_bits(), 2.5_f64.to_bits());
    let result = baml_bridge::runtime::invoke_sync::<f64, core::convert::Infallible>(
        "user.primitives.round_trip_float",
        |arguments| {
            arguments.value(
                "x",
                &7_i64,
                &<f64 as baml_bridge::baml_value::internal::__BamlValuePrivate>::baml_ty(),
            )?;
            Ok(vec![])
        },
    );
    match result {
        Err(baml_bridge::error::Error::Runtime {
            class_name,
            message,
            ..
        }) => {
            assert_eq!(class_name.as_deref(), Some("baml.errors.TypeMismatch"));
            assert!(
                message.contains("does not match the declared type `float`"),
                "{message}"
            );
        }
        other => panic!("an int on the wire is no float: {other:?}"),
    }
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
