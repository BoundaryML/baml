//! `baml.json.to_string` / `baml.json.deserialize<T>` for native values.
//!
//! The VM renders a value to a `serde_json::Value` (class → object in declared
//! field order, `float` through `Number::from_f64` so NaN and the infinities
//! become `null`) and prints it with `serde_json::to_string`; it decodes by
//! parsing with `serde_json::from_str::<Value>` and walking the result against
//! the target type. This module does the same with the same crate and the
//! workspace's `preserve_order` + `arbitrary_precision` features, so the bytes
//! agree.
//!
//! Values use serde's own traits: [`Int63`](crate::Int63) (feature `serde` of `baml_type`),
//! [`Str`], [`Shared<T>`](crate::Shared), `bool`, `f64`, `Option<T>` and
//! `Vec<T>` all implement `Serialize` / `Deserialize`. A generated class
//! derives both with `#[serde(crate = "bex_lang::serde")]` (this crate
//! re-exports `serde` and `serde_json`), in declared field order; its `float`
//! fields carry `#[serde(serialize_with = "bex_lang::json::serialize_f64")]`
//! so that NaN and the infinities print as `null`. Nothing else is needed:
//! serde defaults an absent `Option<T>` field to `None`, as the VM decodes an
//! absent nullable field to `null`, and ignores unknown keys, as the VM does.
//!
//! ```ignore
//! #[derive(bex_lang::serde::Serialize, bex_lang::serde::Deserialize)]
//! #[serde(crate = "bex_lang::serde")]
//! pub struct State {
//!     pub count: Int63,
//!     #[serde(serialize_with = "bex_lang::json::serialize_f64")]
//!     pub ratio: f64,
//!     pub items: Shared<Vec<Str>>,
//!     pub note: Option<Str>,
//! }
//! ```
//!
//! # Errors
//!
//! Invalid JSON throws `baml.json.ParseError { message }` with `serde_json`'s
//! own diagnostic (identical to the VM, same parser). A parsed value that does
//! not fit the target throws `baml.json.DecodeError { message, path }`. The
//! VM's `deserialize<T>` driver re-enters the decoder per class field and per
//! array element, so the `path` it reports is always `""`; this module reports
//! `""` too (full `.field[0]` paths exist only on the VM's `from_string<T>`).
//!
//! Messages: `int` and `string` leaves use the VM's words (`expected
//! integer`, `expected integer in the BAML int range [-2^62, 2^62 - 1]`,
//! `expected string`) because their `Deserialize` impls are ours. Serde's own
//! wording for the other shapes is rewritten into the VM's by
//! [`vm_wording`]:
//!
//! | serde | VM |
//! |---|---|
//! | ``missing field `x` `` | ``missing required field `x` `` |
//! | `invalid type: <v>, expected struct Name` | ``expected JSON object for class `Name` `` |
//! | `invalid length <n>, expected struct Name with <m> elements` | ``expected JSON object for class `Name` `` |
//! | `invalid type: <v>, expected f64` | `expected number` |
//! | `invalid type: <v>, expected a boolean` | `expected boolean` |
//! | `invalid type: <v>, expected a sequence` | `expected array` |
//! | `invalid type: <v>, expected unit` | `expected null` |
//! | `invalid type: <v>, expected expected <text>` | `expected <text>` |
//!
//! `Name` is the Rust struct's identifier, which the emitter keeps equal to
//! the BAML class name. A class whose BAML display name is not a Rust
//! identifier (a namespaced `ns.Item`) adds
//! ``#[serde(expecting = "expected JSON object for class `ns.Item`")]``; the
//! last row passes that text through verbatim. Remaining differences: a
//! serde derive accepts a *complete* positional JSON array for a class
//! (`[true, []]` for a two-field class) where the VM rejects it, a `float`
//! accepts `1e400` as infinity where the VM rejects it, and serde has no
//! distinct wording for an enum variant's enum (enums are not in v2).

use std::fmt;

use serde::{Serialize, Serializer, de::DeserializeOwned};

use crate::{ErrorObject, Str, Thrown, render_object};

/// `baml.json.ParseError { message }`: the text is not JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// The parser's diagnostic, e.g.
    /// `"expected value at line 1 column 1"`.
    pub message: String,
}

impl ErrorObject for ParseError {
    fn class_fqn(&self) -> &'static str {
        "baml.json.ParseError"
    }

    fn render_readable(&self) -> String {
        render_object(
            self.class_fqn(),
            &[("message", format!("{:?}", self.message))],
        )
    }
}

/// `baml.json.DecodeError { message, path }`: valid JSON of the wrong shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError {
    /// What the decoder expected, e.g. `"expected integer"`.
    pub message: String,
    /// Location of the offending node in jq-ish notation; `""` from
    /// [`deserialize`], see the module docs.
    pub path: String,
}

impl DecodeError {
    fn at_root(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            path: String::new(),
        }
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl ErrorObject for DecodeError {
    fn class_fqn(&self) -> &'static str {
        "baml.json.DecodeError"
    }

    fn render_readable(&self) -> String {
        render_object(
            self.class_fqn(),
            &[
                ("message", format!("{:?}", self.message)),
                ("path", format!("{:?}", self.path)),
            ],
        )
    }
}

/// `baml.json.SerializationError { message, path, reason }`: a value with no
/// JSON form. Unreachable for the types this crate serializes; raised only
/// when a hand-written `Serialize` impl fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerializationError {
    /// Prose description.
    pub message: String,
    /// Location in jq-ish notation (`""` here).
    pub path: String,
    /// Stable tag for the construct that had no JSON form.
    pub reason: String,
}

impl ErrorObject for SerializationError {
    fn class_fqn(&self) -> &'static str {
        "baml.json.SerializationError"
    }

    fn render_readable(&self) -> String {
        render_object(
            self.class_fqn(),
            &[
                ("message", format!("{:?}", self.message)),
                ("path", format!("{:?}", self.path)),
                ("reason", format!("{:?}", self.reason)),
            ],
        )
    }
}

/// `#[serde(serialize_with = "bex_lang::json::serialize_f64")]` for every
/// `float` field: finite values as numbers, NaN and the infinities as `null`,
/// the VM's `Number::from_f64(..).unwrap_or(Null)`.
pub fn serialize_f64<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    if value.is_finite() {
        serializer.serialize_f64(*value)
    } else {
        serializer.serialize_unit()
    }
}

/// A `float` value on its own (not a struct field) with the `null` rule of
/// [`serialize_f64`], for `json.to_string(x)` where `x: float`.
#[derive(Debug, Clone, Copy)]
pub struct Float(pub f64);

impl Serialize for Float {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serialize_f64(&self.0, serializer)
    }
}

/// `baml.json.to_string(value)`: compact JSON, as `serde_json::to_string`.
pub fn to_string<T: Serialize + ?Sized>(value: &T) -> Result<Str, Thrown> {
    match serde_json::to_string(value) {
        Ok(text) => Ok(Str::from(text)),
        Err(error) => Err(Thrown::error(SerializationError {
            message: error.to_string(),
            path: String::new(),
            reason: "unserializable".to_owned(),
        })),
    }
}

/// `baml.json.deserialize<T>(text)`: parse, then decode against `T`. Throws
/// [`ParseError`] for invalid JSON and [`DecodeError`] for a shape mismatch.
pub fn deserialize<T: DeserializeOwned>(text: &Str) -> Result<T, Thrown> {
    let json: serde_json::Value = serde_json::from_str(text.as_str()).map_err(|error| {
        Thrown::error(ParseError {
            message: error.to_string(),
        })
    })?;
    validate_bigint_bounds(&json)
        .map_err(|message| Thrown::error(DecodeError::at_root(message)))?;
    decode(&json)
}

/// `baml.json.deserialize<T>` over an already parsed tree.
pub fn decode<T: DeserializeOwned>(json: &serde_json::Value) -> Result<T, Thrown> {
    T::deserialize(json)
        .map_err(|error| Thrown::error(DecodeError::at_root(vm_wording(&error.to_string()))))
}

/// Rewrite a serde(-derive) error message into the VM's `DecodeError`
/// wording; see the table in the [module docs](self). Messages this crate's
/// own visitors produce are already the VM's and pass through unchanged.
pub fn vm_wording(message: &str) -> String {
    if let Some(field) = message.strip_prefix("missing field ") {
        return format!("missing required field {field}");
    }
    let expected = if let Some(rest) = message.strip_prefix("invalid type: ") {
        // The unexpected value may itself contain `, expected ` (a string
        // literal does); the expectation never does.
        rest.rsplit_once(", expected ")
            .map(|(_, expected)| expected)
    } else if let Some(rest) = message.strip_prefix("invalid length ") {
        // `invalid length 0, expected struct Name with 6 elements`: a
        // positional array offered for a class.
        // (With an `expecting` attribute serde omits the element count.)
        rest.split_once(", expected ")
            .map(|(_, expected)| expected)
            .map(|expected| {
                expected
                    .strip_suffix(" elements")
                    .and_then(|head| head.rsplit_once(" with "))
                    .map_or(expected, |(head, _)| head)
            })
    } else {
        None
    };
    let Some(expected) = expected else {
        return message.to_owned();
    };
    if let Some(name) = expected.strip_prefix("struct ") {
        format!("expected JSON object for class `{name}`")
    } else if expected.starts_with("expected ") {
        expected.to_owned()
    } else {
        match expected {
            "f64" | "f32" => "expected number".to_owned(),
            "a boolean" => "expected boolean".to_owned(),
            "a sequence" => "expected array".to_owned(),
            "unit" => "expected null".to_owned(),
            _ => message.to_owned(),
        }
    }
}

/// The largest digit count an integral token can have and still be sure to
/// fit `baml_type::MAX_BIGINT_BITS` (2^28): `10^80807124 < 2^(2^28)`, since
/// `80807124 · log2(10) ≈ 268435455.7`. The VM parses the digits into a
/// bigint and tests the exact bit count; this crate has no bigint, so it
/// rejects every token with more digits. The two disagree only for an
/// 80 807 125-digit integer below `2^(2^28)`, which the VM accepts.
const MAX_SURELY_FITTING_DIGITS: usize = 80_807_124;

/// The VM's `validate_json_bigint_bounds`: reject integral tokens too large for
/// any BAML number before typed decoding sees them, with its message.
fn validate_bigint_bounds(json: &serde_json::Value) -> Result<(), String> {
    match json {
        serde_json::Value::Number(number) => {
            let text = number.to_string();
            let digits = text.strip_prefix('-').unwrap_or(&text);
            let integral = !digits
                .bytes()
                .any(|byte| matches!(byte, b'.' | b'e' | b'E'));
            if integral && digits.len() > MAX_SURELY_FITTING_DIGITS {
                return Err(format!(
                    "integral JSON number exceeds the {}-bit bigint limit",
                    baml_type::MAX_BIGINT_BITS
                ));
            }
            Ok(())
        }
        serde_json::Value::Array(items) => items.iter().try_for_each(validate_bigint_bounds),
        serde_json::Value::Object(entries) => entries.values().try_for_each(validate_bigint_bounds),
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::String(_) => {
            Ok(())
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use serde::Deserialize;

    use super::*;
    use crate::{Int63, Shared, array, shared, string::from_literal};

    const EXPECTED_INT_RANGE: &str = "expected integer in the BAML int range [-2^62, 2^62 - 1]";

    fn int(value: i64) -> Int63 {
        Int63::new(value).unwrap()
    }

    fn text(s: &str) -> Str {
        Str::from(s)
    }

    fn rendered<T: fmt::Debug>(result: Result<T, Thrown>) -> String {
        match result {
            Err(Thrown::Error(error)) => error.render_readable(),
            other => panic!("expected a thrown error, got {other:?}"),
        }
    }

    fn json<T: Serialize + ?Sized>(value: &T) -> String {
        to_string(value).unwrap().to_string()
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(crate = "serde")]
    struct State {
        count: Int63,
        #[serde(serialize_with = "serialize_f64")]
        ratio: f64,
        name: Str,
        items: Shared<Vec<Int63>>,
        note: Option<Str>,
        inner: Option<Shared<Inner>>,
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(crate = "serde")]
    struct Inner {
        flag: bool,
        tags: Shared<Vec<Str>>,
    }

    fn sample() -> State {
        State {
            count: int(-3),
            ratio: 0.5,
            name: from_literal("a \"quoted\" ☃"),
            items: array::new(vec![int(1), Int63::MAX]),
            note: None,
            inner: Some(shared(Inner {
                flag: true,
                tags: array::new(vec![from_literal("x")]),
            })),
        }
    }

    const SAMPLE_JSON: &str = r#"{"count":-3,"ratio":0.5,"name":"a \"quoted\" ☃","items":[1,4611686018427387903],"note":null,"inner":{"flag":true,"tags":["x"]}}"#;

    #[test]
    fn scalars_encode_like_the_vm() {
        assert_eq!(json(&int(42)), "42");
        assert_eq!(json(&Int63::MIN), "-4611686018427387904");
        assert_eq!(json(&true), "true");
        assert_eq!(json(&()), "null");
        assert_eq!(json(&1.0), "1.0");
        assert_eq!(json(&0.1), "0.1");
        assert_eq!(json(&-0.0), "-0.0");
        // Exponent spellings are whatever serde_json's float writer picks;
        // the VM goes through `Number::from_f64` on the same crate.
        for value in [1e21, 1e-7, 123_456_789.123_456_78] {
            let vm = serde_json::to_string(&serde_json::Number::from_f64(value).unwrap()).unwrap();
            assert_eq!(json(&value), vm);
            assert_eq!(json(&Float(value)), vm);
        }
        assert_eq!(json(&Float(f64::NAN)), "null");
        assert_eq!(json(&Float(f64::INFINITY)), "null");
        assert_eq!(json(&Float(f64::NEG_INFINITY)), "null");
        assert_eq!(json(&text("tab\t\"q\" ☃")), r#""tab\t\"q\" ☃""#);
        assert_eq!(json(&None::<Int63>), "null");
        assert_eq!(json(&Some(int(1))), "1");
        assert_eq!(
            json(&array::new(vec![Float(f64::NAN), Float(2.0)])),
            "[null,2.0]"
        );
        assert_eq!(json(&array::new(Vec::<Str>::new())), "[]");
        assert_eq!(json(&vec![vec![int(1)], vec![]]), "[[1],[]]");
    }

    #[test]
    fn structs_round_trip_in_declared_field_order() {
        let state = sample();
        assert_eq!(json(&state), SAMPLE_JSON);
        assert_eq!(json(&shared(sample())), SAMPLE_JSON);
        assert_eq!(
            json(&array::new(vec![shared(sample())])),
            format!("[{SAMPLE_JSON}]")
        );
        let decoded: State = deserialize(&text(SAMPLE_JSON)).unwrap();
        assert_eq!(decoded, state);
        let via_shared: Shared<State> = deserialize(&text(SAMPLE_JSON)).unwrap();
        assert_eq!(json(&via_shared), SAMPLE_JSON);
        let list: Vec<Shared<State>> = deserialize(&text(&format!("[{SAMPLE_JSON}]"))).unwrap();
        assert_eq!(list.len(), 1);
        // Absent nullable fields decode as null; unknown fields are ignored.
        let sparse: State = deserialize(&text(
            r#"{"count": 0, "ratio": 2, "name": "", "items": [], "extra": 1}"#,
        ))
        .unwrap();
        assert_eq!(sparse.note, None);
        assert_eq!(sparse.inner, None);
        assert_eq!(sparse.ratio, 2.0);
        // Field order on output is the declaration order, whatever the input had.
        let shuffled: State = deserialize(&text(
            r#"{"inner":null,"note":"n","items":[2],"name":"s","ratio":1.5,"count":7}"#,
        ))
        .unwrap();
        assert_eq!(
            json(&shuffled),
            r#"{"count":7,"ratio":1.5,"name":"s","items":[2],"note":"n","inner":null}"#
        );
        // A NaN field prints as null and reads back as null -> decode error.
        let mut nan = sample();
        nan.ratio = f64::NAN;
        assert!(json(&nan).contains(r#""ratio":null"#));
    }

    #[test]
    fn scalars_decode_like_the_vm() {
        assert_eq!(deserialize::<Int63>(&text(" 42 ")).unwrap(), int(42));
        assert_eq!(deserialize::<Int63>(&text("-0")).unwrap(), Int63::ZERO);
        assert_eq!(
            deserialize::<Int63>(&text("4611686018427387903")).unwrap(),
            Int63::MAX
        );
        assert_eq!(
            deserialize::<Int63>(&text("-4611686018427387904")).unwrap(),
            Int63::MIN
        );
        assert_eq!(deserialize::<f64>(&text("3")).unwrap(), 3.0);
        assert_eq!(deserialize::<f64>(&text("-1.5e3")).unwrap(), -1500.0);
        assert!(!deserialize::<bool>(&text("false")).unwrap());
        deserialize::<()>(&text("null")).unwrap();
        assert_eq!(deserialize::<Str>(&text(r#""☃ x""#)).unwrap(), text("☃ x"));
        assert_eq!(deserialize::<Option<Int63>>(&text("null")).unwrap(), None);
        assert_eq!(
            deserialize::<Option<Int63>>(&text("5")).unwrap(),
            Some(int(5))
        );
        let items: Shared<Vec<Option<Str>>> = deserialize(&text(r#"["a", null]"#)).unwrap();
        assert_eq!(*items.borrow(), vec![Some(text("a")), None]);
    }

    #[test]
    fn parse_errors_carry_serde_json_diagnostics() {
        assert_eq!(
            rendered(deserialize::<Int63>(&text(""))),
            r#"baml.json.ParseError {message: "EOF while parsing a value at line 1 column 0"}"#
        );
        assert_eq!(
            rendered(deserialize::<Int63>(&text("{\"a\": }"))),
            r#"baml.json.ParseError {message: "expected value at line 1 column 7"}"#
        );
        assert_eq!(
            rendered(deserialize::<Int63>(&text("1 2"))),
            r#"baml.json.ParseError {message: "trailing characters at line 1 column 3"}"#
        );
        assert_eq!(
            deserialize::<Int63>(&text("nul")).unwrap_err().exit_code(),
            1
        );
    }

    #[test]
    fn int_and_string_decode_errors_use_the_vm_messages() {
        type Case = (&'static str, fn(&Str) -> Result<(), Thrown>, &'static str);
        let cases: &[Case] = &[
            (
                "\"x\"",
                |t| deserialize::<Int63>(t).map(drop),
                "expected integer",
            ),
            (
                "null",
                |t| deserialize::<Int63>(t).map(drop),
                "expected integer",
            ),
            (
                "true",
                |t| deserialize::<Int63>(t).map(drop),
                "expected integer",
            ),
            (
                "[1]",
                |t| deserialize::<Int63>(t).map(drop),
                "expected integer",
            ),
            (
                "{\"a\": 1}",
                |t| deserialize::<Int63>(t).map(drop),
                "expected integer",
            ),
            (
                "1.0",
                |t| deserialize::<Int63>(t).map(drop),
                EXPECTED_INT_RANGE,
            ),
            (
                "1.5",
                |t| deserialize::<Int63>(t).map(drop),
                EXPECTED_INT_RANGE,
            ),
            (
                "1e3",
                |t| deserialize::<Int63>(t).map(drop),
                EXPECTED_INT_RANGE,
            ),
            (
                "1e400",
                |t| deserialize::<Int63>(t).map(drop),
                EXPECTED_INT_RANGE,
            ),
            (
                "4611686018427387904",
                |t| deserialize::<Int63>(t).map(drop),
                EXPECTED_INT_RANGE,
            ),
            (
                "-4611686018427387905",
                |t| deserialize::<Int63>(t).map(drop),
                EXPECTED_INT_RANGE,
            ),
            (
                "18446744073709551616",
                |t| deserialize::<Int63>(t).map(drop),
                EXPECTED_INT_RANGE,
            ),
            ("1", |t| deserialize::<Str>(t).map(drop), "expected string"),
            (
                "null",
                |t| deserialize::<Str>(t).map(drop),
                "expected string",
            ),
            (
                "[\"a\"]",
                |t| deserialize::<Str>(t).map(drop),
                "expected string",
            ),
            ("{}", |t| deserialize::<Str>(t).map(drop), "expected string"),
            (
                "[1, \"x\"]",
                |t| deserialize::<Shared<Vec<Int63>>>(t).map(drop),
                "expected integer",
            ),
            (
                "\"x\"",
                |t| deserialize::<Option<Int63>>(t).map(drop),
                "expected integer",
            ),
        ];
        for (input, decode, message) in cases {
            assert_eq!(
                rendered(decode(&text(input))),
                format!(r#"baml.json.DecodeError {{message: "{message}", path: ""}}"#),
                "input {input}"
            );
        }
    }

    /// Serde's wording for the other shapes is rewritten into the VM's
    /// (`package_baml/json.rs`: `deserialize_class_instance`,
    /// `ty_serde_to_value`).
    #[test]
    fn other_shape_errors_are_rewritten_into_the_vm_wording() {
        type Case = (&'static str, fn(&Str) -> Result<(), Thrown>, &'static str);
        let cases: &[Case] = &[
            (
                r#"{"count": 1}"#,
                |t| deserialize::<State>(t).map(drop),
                "missing required field `ratio`",
            ),
            (
                "7",
                |t| deserialize::<State>(t).map(drop),
                "expected JSON object for class `State`",
            ),
            (
                "\"s\"",
                |t| deserialize::<Shared<Inner>>(t).map(drop),
                "expected JSON object for class `Inner`",
            ),
            (
                "[]",
                |t| deserialize::<State>(t).map(drop),
                "expected JSON object for class `State`",
            ),
            (
                "null",
                |t| deserialize::<Namespaced>(t).map(drop),
                "expected JSON object for class `ns.Item`",
            ),
            (
                "[]",
                |t| deserialize::<Namespaced>(t).map(drop),
                "expected JSON object for class `ns.Item`",
            ),
            (
                "\"1\"",
                |t| deserialize::<f64>(t).map(drop),
                "expected number",
            ),
            (
                "0",
                |t| deserialize::<bool>(t).map(drop),
                "expected boolean",
            ),
            (
                "{}",
                |t| deserialize::<Vec<Int63>>(t).map(drop),
                "expected array",
            ),
            (
                "\"x\"",
                |t| deserialize::<Shared<Vec<Str>>>(t).map(drop),
                "expected array",
            ),
            ("1", |t| deserialize::<()>(t).map(drop), "expected null"),
            // A leaf error inside a class keeps the VM's leaf message.
            (
                r#"{"count": "1", "ratio": 1, "name": "", "items": []}"#,
                |t| deserialize::<State>(t).map(drop),
                "expected integer",
            ),
            (
                r#"{"count": 1, "ratio": "x", "name": "", "items": []}"#,
                |t| deserialize::<State>(t).map(drop),
                "expected number",
            ),
            (
                r#"{"count": 1, "ratio": 1, "name": "", "items": [], "inner": {"flag": true}}"#,
                |t| deserialize::<State>(t).map(drop),
                "missing required field `tags`",
            ),
        ];
        for (input, decode, message) in cases {
            let result = decode(&text(input));
            assert!(result.is_err(), "input {input} decoded");
            assert_eq!(
                rendered(result),
                format!(r#"baml.json.DecodeError {{message: "{message}", path: ""}}"#),
                "input {input}"
            );
        }
        // Known divergence: a serde derive accepts a complete positional array
        // for a class, which the VM rejects.
        assert!(deserialize::<Inner>(&text("[true, []]")).is_ok());
        assert!(deserialize::<Namespaced>(&text("[1]")).is_ok());
    }

    #[derive(Debug, Deserialize)]
    #[serde(
        crate = "serde",
        expecting = "expected JSON object for class `ns.Item`"
    )]
    struct Namespaced {
        #[allow(dead_code)]
        value: Int63,
    }

    #[test]
    fn vm_wording_table() {
        let table = [
            ("missing field `y`", "missing required field `y`"),
            (
                "invalid type: string \"a, expected b\", expected struct Row",
                "expected JSON object for class `Row`",
            ),
            (
                "invalid length 2, expected struct Row with 3 elements",
                "expected JSON object for class `Row`",
            ),
            (
                "invalid length 0, expected expected JSON object for class `a.B`",
                "expected JSON object for class `a.B`",
            ),
            (
                "invalid length 1, expected struct Row with 2 elements",
                "expected JSON object for class `Row`",
            ),
            ("invalid type: null, expected f64", "expected number"),
            ("invalid type: map, expected a boolean", "expected boolean"),
            (
                "invalid type: integer `1`, expected a sequence",
                "expected array",
            ),
            (
                "invalid type: boolean `true`, expected unit",
                "expected null",
            ),
            (
                "invalid type: number, expected expected JSON object for class `a.B`",
                "expected JSON object for class `a.B`",
            ),
            // Untouched: this crate's own messages and anything unknown.
            ("expected integer", "expected integer"),
            ("expected string", "expected string"),
            (
                "unknown variant `x`, expected one of `A`, `B`",
                "unknown variant `x`, expected one of `A`, `B`",
            ),
            (
                "invalid type: map, expected i64",
                "invalid type: map, expected i64",
            ),
        ];
        for (serde_message, vm_message) in table {
            assert_eq!(vm_wording(serde_message), vm_message, "{serde_message}");
        }
    }

    #[test]
    fn huge_integral_tokens_are_rejected_before_decoding() {
        let digits = "9".repeat(MAX_SURELY_FITTING_DIGITS + 1);
        assert_eq!(
            rendered(deserialize::<f64>(&text(&format!("[{digits}]")))),
            r#"baml.json.DecodeError {message: "integral JSON number exceeds the 268435456-bit bigint limit", path: ""}"#
        );
        // A fraction or exponent is a float token, never a bigint.
        let float_token = format!("{digits}.0");
        assert_eq!(
            rendered(deserialize::<Int63>(&text(&float_token))),
            format!(r#"baml.json.DecodeError {{message: "{EXPECTED_INT_RANGE}", path: ""}}"#)
        );
    }

    /// The digit threshold is derived from `MAX_BIGINT_BITS`; pin both so a
    /// change to the limit cannot silently leave the constant behind.
    #[test]
    fn digit_threshold_matches_the_bigint_bit_limit() {
        assert_eq!(baml_type::MAX_BIGINT_BITS, 1 << 28);
        #[allow(clippy::cast_precision_loss)]
        let bits = baml_type::MAX_BIGINT_BITS as f64;
        // Every number with this many digits is below 10^digits <= 2^bits ...
        #[allow(clippy::cast_precision_loss)]
        let fits = MAX_SURELY_FITTING_DIGITS as f64 * 10f64.log2();
        assert!(fits <= bits, "{fits} > {bits}");
        // ... and one more digit can exceed the limit.
        #[allow(clippy::cast_precision_loss)]
        let overflows = (MAX_SURELY_FITTING_DIGITS + 1) as f64 * 10f64.log2();
        assert!(overflows > bits);
        const { assert!(MAX_SURELY_FITTING_DIGITS < baml_type::MAX_BIGINT_DECIMAL_DIGITS) };
    }

    #[test]
    fn error_objects_render_like_the_engine() {
        let decode_error = DecodeError {
            message: "expected integer".into(),
            path: ".a[0]".into(),
        };
        assert_eq!(
            decode_error.render_readable(),
            r#"baml.json.DecodeError {message: "expected integer", path: ".a[0]"}"#
        );
        assert_eq!(decode_error.to_string(), "expected integer");
        let serialization = SerializationError {
            message: "m".into(),
            path: String::new(),
            reason: "class".into(),
        };
        assert_eq!(
            serialization.render_readable(),
            r#"baml.json.SerializationError {message: "m", path: "", reason: "class"}"#
        );
    }

    #[test]
    fn decode_entry_over_a_parsed_tree() {
        let tree: serde_json::Value = serde_json::from_str("[1, 2]").unwrap();
        let items: Vec<Int63> = decode(&tree).unwrap();
        assert_eq!(items, vec![int(1), int(2)]);
    }
}
