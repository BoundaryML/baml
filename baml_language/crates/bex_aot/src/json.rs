//! `baml.json.to_string` / `baml.json.deserialize<T>` for native values.
//!
//! The VM renders a value to a `serde_json::Value` (class → object in declared
//! field order, `float` through `Number::from_f64`, so NaN and the infinities
//! become `null`) and prints it with `serde_json::to_string`; it decodes by
//! parsing with `serde_json::from_str` and walking the result against the
//! target type. This module does the same with the same crate and the
//! workspace's `preserve_order` + `arbitrary_precision` features, so the bytes
//! agree.
//!
//! Values use serde's own traits: [`Int63`](crate::Int63) (feature `serde` of
//! `baml_type`), [`Str`], [`Shared<T>`](crate::Shared), `bool`, `f64`,
//! `Option<T>` and `Vec<T>` all implement `Serialize` / `Deserialize`. A
//! generated class derives both with `#[serde(crate = "bex_aot::serde")]`
//! (this crate re-exports `serde` and `serde_json`), in declared field order.
//! Nothing else is needed: `serde_json` writes a non-finite `f64` as `null`,
//! serde defaults an absent `Option<T>` field to `None`, as the VM decodes an
//! absent nullable field to `null`, and ignores unknown keys, as the VM does.
//!
//! # Errors
//!
//! Invalid JSON throws `baml.json.ParseError { message }` with `serde_json`'s
//! diagnostic (identical to the VM's, same parser). A parsed value that does
//! not fit the target throws `baml.json.DecodeError { message, path }`. The
//! classes and their non-message fields match the VM; the message text is
//! serde's, where the VM words its own (the VM's `deserialize<T>` reports
//! `path` as `""`, and so does this module).

use std::fmt;

use serde::{Serialize, de::DeserializeOwned};

use crate::{ErrorObject, Str, Thrown};

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

    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("message", format!("{:?}", self.message))]
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn clone_box(&self) -> Box<dyn ErrorObject> {
        Box::new(self.clone())
    }
}

/// `baml.json.DecodeError { message, path }`: valid JSON of the wrong shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError {
    /// What the decoder expected, e.g. `"missing field `count`"`.
    pub message: String,
    /// Location of the offending node in jq-ish notation; `""` from
    /// [`deserialize`], see the module docs.
    pub path: String,
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

    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("message", format!("{:?}", self.message)),
            ("path", format!("{:?}", self.path)),
        ]
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn clone_box(&self) -> Box<dyn ErrorObject> {
        Box::new(self.clone())
    }
}

/// `baml.json.SerializationError { message, path, reason }`: a value with no
/// JSON form, which among the types this crate serializes is a map keyed by
/// `int` or `bool` ([`crate::map::STRING_MAP_REQUIRED`]).
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

    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("message", format!("{:?}", self.message)),
            ("path", format!("{:?}", self.path)),
            ("reason", format!("{:?}", self.reason)),
        ]
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn clone_box(&self) -> Box<dyn ErrorObject> {
        Box::new(self.clone())
    }
}

/// `baml.json.to_string(value)`: compact JSON, as `serde_json::to_string`.
/// A value with no JSON form (a map keyed by `int` or `bool`) throws
/// [`SerializationError`] with the reason `"unserializable"`, as the VM's
/// untyped rendering does.
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
    T::deserialize(json).map_err(|error| {
        Thrown::error(DecodeError {
            message: error.to_string(),
            path: String::new(),
        })
    })
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use serde::Deserialize;

    use super::*;
    use crate::{Int63, Shared, array, shared, string::from_literal};

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

    /// The `message` of the `DecodeError` thrown for `input`. Fields render
    /// in `{:?}` form, which for these messages is JSON string syntax.
    fn decode_message<T: DeserializeOwned + fmt::Debug>(input: &str) -> String {
        match deserialize::<T>(&text(input)) {
            Err(Thrown::Error(error)) => {
                assert_eq!(error.class_fqn(), "baml.json.DecodeError", "input {input}");
                let fields = error.fields();
                let (_, message) = fields
                    .iter()
                    .find(|(name, _)| *name == "message")
                    .expect("DecodeError has a message");
                serde_json::from_str(message).expect("a rendered string")
            }
            other => panic!("expected a DecodeError for {input}, got {other:?}"),
        }
    }

    fn json<T: Serialize + ?Sized>(value: &T) -> String {
        to_string(value).unwrap().to_string()
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(crate = "serde")]
    struct State {
        count: Int63,
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

    impl PartialEq for Shared<Vec<Int63>> {
        fn eq(&self, other: &Self) -> bool {
            *self.borrow() == *other.borrow()
        }
    }

    impl PartialEq for Shared<Vec<Str>> {
        fn eq(&self, other: &Self) -> bool {
            *self.borrow() == *other.borrow()
        }
    }

    impl PartialEq for Shared<Inner> {
        fn eq(&self, other: &Self) -> bool {
            *self.borrow() == *other.borrow()
        }
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
        }
        // `Number::from_f64` is `None` for a non-finite float and the VM
        // writes `null`; `serde_json` does the same for a bare `f64`.
        assert_eq!(json(&f64::NAN), "null");
        assert_eq!(json(&f64::INFINITY), "null");
        assert_eq!(json(&f64::NEG_INFINITY), "null");
        assert_eq!(json(&text("tab\t\"q\" ☃")), r#""tab\t\"q\" ☃""#);
        assert_eq!(json(&None::<Int63>), "null");
        assert_eq!(json(&Some(int(1))), "1");
        assert_eq!(json(&array::new(vec![f64::NAN, 2.0])), "[null,2.0]");
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
        // A NaN field prints as null.
        let mut nan = sample();
        nan.ratio = f64::NAN;
        assert!(json(&nan).contains(r#""ratio":null"#));
    }

    #[test]
    fn scalars_decode_like_the_vm() {
        assert_eq!(deserialize::<Int63>(&text(" 42 ")).unwrap(), int(42));
        assert_eq!(deserialize::<Int63>(&text("-0")).unwrap(), crate::int::ZERO);
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

    /// Every shape mismatch is a `DecodeError` with `path` `""`, as on the
    /// VM. The messages are serde's and are pinned here so a serde upgrade
    /// that rewords them shows up as a test change, not a silent one.
    #[test]
    fn shape_mismatches_are_decode_errors() {
        const OUT_OF_RANGE: &str = "is outside the int range [-2^62, 2^62 - 1]";
        assert_eq!(
            rendered(deserialize::<Int63>(&text("\"x\""))),
            r#"baml.json.DecodeError {message: "invalid type: string \"x\", expected an int in [-2^62, 2^62 - 1]", path: ""}"#
        );
        assert_eq!(
            decode_message::<Int63>("1.5"),
            "invalid type: floating point `1.5`, expected an int in [-2^62, 2^62 - 1]"
        );
        for input in [
            "4611686018427387904",
            "-4611686018427387905",
            "18446744073709551616",
        ] {
            assert_eq!(
                decode_message::<Int63>(input),
                format!("{input} {OUT_OF_RANGE}")
            );
        }
        assert_eq!(
            decode_message::<Str>("1"),
            "invalid type: number, expected a string"
        );
        assert_eq!(
            decode_message::<State>(r#"{"count": 1}"#),
            "missing field `ratio`"
        );
        assert_eq!(
            decode_message::<State>("7"),
            "invalid type: number, expected struct State"
        );
        assert_eq!(
            decode_message::<Shared<Vec<Str>>>("\"x\""),
            "invalid type: string \"x\", expected a sequence"
        );
        assert_eq!(
            decode_message::<bool>("0"),
            "invalid type: number, expected a boolean"
        );
        assert_eq!(
            decode_message::<()>("1"),
            "invalid type: number, expected unit"
        );
        // A leaf error inside a class reports the leaf.
        assert_eq!(
            decode_message::<State>(r#"{"count": "1", "ratio": 1, "name": "", "items": []}"#),
            "invalid type: string \"1\", expected an int in [-2^62, 2^62 - 1]"
        );
        assert_eq!(
            decode_message::<State>(
                r#"{"count": 1, "ratio": 1, "name": "", "items": [], "inner": {"flag": true}}"#
            ),
            "missing field `tags`"
        );
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
}
