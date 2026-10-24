//! `baml.json.to_string` / `baml.json.deserialize<T>` for native values.
//!
//! Serialization: the VM renders a value to a `serde_json::Value` (class →
//! object in declared field order, `float` through `Number::from_f64`, so NaN
//! and the infinities become `null`) and prints it with
//! `serde_json::to_string`. This module does the same with the same crate and
//! the workspace's `preserve_order` + `arbitrary_precision` features, so the
//! bytes agree; values implement serde's `Serialize` and a generated class
//! derives it with `#[serde(crate = "bex_aot::serde")]`.
//!
//! Decoding: the VM parses with `serde_json::from_str`, rejects an integral
//! number past the bigint limit, then decodes the parsed value against the
//! target type (`from_json<T>` in `package_baml/json.rs`): classes, arrays
//! and maps through its dispatcher, each nested value from a fresh root, and
//! everything else through `ty_serde_to_value`. [`FromJson`] is that decode
//! for native types: every rule and message here is the VM's, and the
//! `path` of a `DecodeError` is `""` as the dispatcher reports it, so
//! `baml.json.DecodeError { message, path }` agrees field for field. A
//! generated class, enum and union implements it (field by declared name,
//! an absent nullable field as `null`, unknown keys ignored, a variant by
//! name, a union's first member that fits, in the spelled order). A class
//! with its own `baml.FromJson` is rejected at compile time until the
//! `json` value exists natively.
//!
//! # Errors
//!
//! Invalid JSON throws `baml.json.ParseError { message }` with `serde_json`'s
//! diagnostic, or the VM's bigint-limit message. A parsed value that does not
//! fit the target throws `baml.json.DecodeError { message, path }`.

use std::fmt;

use serde::Serialize;

use crate::{
    ErrorObject, Int63, Str, Thrown,
    handle::{Kind, Shared},
};

/// A parsed JSON document, as the VM holds it while decoding.
pub type Json = serde_json::Value;

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

    fn trace(&self, _visitor: &mut dyn crate::mem::Visitor) {}
}

/// `baml.json.DecodeError { message, path }`: valid JSON of the wrong shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError {
    /// What the decoder expected, e.g. `"missing field `count`"`.
    pub message: String,
    /// Location in jq-ish notation; `""` from `deserialize<T>`, whose
    /// dispatcher decodes every nested value from a fresh root (the VM's
    /// `from_json<T>` does the same).
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

    fn trace(&self, _visitor: &mut dyn crate::mem::Visitor) {}
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

    fn trace(&self, _visitor: &mut dyn crate::mem::Visitor) {}
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

fn decode_error(message: impl Into<String>) -> DecodeError {
    DecodeError {
        message: message.into(),
        path: String::new(),
    }
}

/// One member's decoder in a generated union's [`FromJson`] impl.
pub type MemberDecoder<T> = fn(&Json) -> Result<T, DecodeError>;

/// The typed decode of a parsed JSON value, as the VM's
/// `baml.json.deserialize<T>` performs it: `from_json<T>` dispatches a class
/// (field by field), an array (element by element) and a map (value by
/// value) with a fresh root each time, and decodes anything else
/// structurally, so the `path` a `DecodeError` carries is always `""`, and
/// an array or map that is not one reports `expected JSON array` /
/// `expected JSON object`, the dispatcher's wording. A nullable value is
/// `null` or its type, reporting that type's error.
pub trait FromJson: Sized {
    fn from_json(json: &Json) -> Result<Self, DecodeError>;
}

impl FromJson for () {
    fn from_json(json: &Json) -> Result<Self, DecodeError> {
        match json {
            Json::Null => Ok(()),
            _ => Err(decode_error("expected null")),
        }
    }
}

impl FromJson for bool {
    fn from_json(json: &Json) -> Result<Self, DecodeError> {
        match json {
            Json::Bool(value) => Ok(*value),
            _ => Err(decode_error("expected boolean")),
        }
    }
}

impl FromJson for Int63 {
    fn from_json(json: &Json) -> Result<Self, DecodeError> {
        match json {
            Json::Number(number) => number.as_i64().and_then(Int63::new).ok_or_else(|| {
                decode_error("expected integer in the BAML int range [-2^62, 2^62 - 1]")
            }),
            _ => Err(decode_error("expected integer")),
        }
    }
}

impl FromJson for f64 {
    fn from_json(json: &Json) -> Result<Self, DecodeError> {
        match json {
            Json::Number(number) => number
                .as_f64()
                .ok_or_else(|| decode_error("expected number")),
            _ => Err(decode_error("expected number")),
        }
    }
}

impl FromJson for Str {
    fn from_json(json: &Json) -> Result<Self, DecodeError> {
        match json {
            Json::String(text) => Ok(Str::from(text.clone())),
            _ => Err(decode_error("expected string")),
        }
    }
}

/// `T | null` at a dispatch point: `null`, or `T` with `T`'s own error.
impl<T: FromJson> FromJson for Option<T> {
    fn from_json(json: &Json) -> Result<Self, DecodeError> {
        match json {
            Json::Null => Ok(None),
            _ => T::from_json(json).map(Some),
        }
    }
}

impl<T: FromJson> FromJson for Vec<T> {
    fn from_json(json: &Json) -> Result<Self, DecodeError> {
        match json {
            Json::Array(items) => items.iter().map(T::from_json).collect(),
            _ => Err(decode_error("expected JSON array")),
        }
    }
}

impl<T: FromJson + crate::mem::Trace, H: Kind> FromJson for Shared<T, H> {
    fn from_json(json: &Json) -> Result<Self, DecodeError> {
        T::from_json(json).map(Shared::new)
    }
}

/// The object a generated class decodes from, or the VM's complaint naming
/// the class as its JSON decode errors do.
pub fn object<'a>(
    json: &'a Json,
    class_name: &str,
) -> Result<&'a serde_json::Map<String, Json>, DecodeError> {
    match json {
        Json::Object(map) => Ok(map),
        _ => Err(decode_error(format!(
            "expected JSON object for class `{class_name}`"
        ))),
    }
}

/// One field of a generated class: absent and nullable decodes as `null`,
/// absent otherwise is the VM's missing-field error; unknown keys in the
/// object are ignored, as the VM ignores them.
pub fn field<T: FromJson>(
    object: &serde_json::Map<String, Json>,
    name: &str,
    nullable: bool,
) -> Result<T, DecodeError> {
    match object.get(name) {
        Some(json) => T::from_json(json),
        None if nullable => T::from_json(&Json::Null),
        None => Err(decode_error(format!("missing required field `{name}`"))),
    }
}

/// A generated enum: the variant named by a JSON string, by position in
/// `names` (declaration order).
pub fn enum_variant(json: &Json, enum_name: &str, names: &[&str]) -> Result<usize, DecodeError> {
    match json {
        Json::String(name) => names.iter().position(|known| known == name).ok_or_else(|| {
            decode_error(format!("unknown variant `{name}` for enum `{enum_name}`"))
        }),
        _ => Err(decode_error("expected enum variant string")),
    }
}

/// A generated union: the first member, in the union's spelled order, that
/// the value decodes as.
pub fn union<T>(json: &Json, members: &[MemberDecoder<T>]) -> Result<T, DecodeError> {
    members
        .iter()
        .find_map(|member| member(json).ok())
        .ok_or_else(|| decode_error("no union member matched"))
}

/// `baml.json.parse(text)`: the document, or the VM's `ParseError` (the
/// parser's diagnostic, or an integral number past the bigint limit, which
/// the VM rejects at parse time).
pub fn parse(text: &Str) -> Result<Json, Thrown> {
    let json: Json = serde_json::from_str(text.as_str()).map_err(|error| {
        Thrown::error(ParseError {
            message: error.to_string(),
        })
    })?;
    validate_bigint_bounds(&json).map_err(|message| Thrown::error(ParseError { message }))?;
    Ok(json)
}

/// The VM's `validate_json_bigint_bounds`: an integral number token (no
/// fraction, no exponent) that no bigint can hold.
fn validate_bigint_bounds(json: &Json) -> Result<(), String> {
    match json {
        Json::Number(number) => {
            let text = number.to_string();
            let integral = !text.bytes().any(|byte| matches!(byte, b'.' | b'e' | b'E'));
            if integral && crate::bigint::parse_decimal_text(&text).is_none() {
                return Err(format!(
                    "integral JSON number exceeds the {}-bit bigint limit",
                    baml_type::MAX_BIGINT_BITS
                ));
            }
            Ok(())
        }
        Json::Array(items) => items.iter().try_for_each(validate_bigint_bounds),
        Json::Object(entries) => entries.values().try_for_each(validate_bigint_bounds),
        Json::Null | Json::Bool(_) | Json::String(_) => Ok(()),
    }
}

/// `baml.json.deserialize<T>(text)`: parse, then decode against `T`. Throws
/// [`ParseError`] for invalid JSON and [`DecodeError`] for a shape mismatch.
pub fn deserialize<T: FromJson>(text: &Str) -> Result<T, Thrown> {
    let json = parse(text)?;
    T::from_json(&json).map_err(Thrown::error)
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::{array, string::from_literal};

    fn int(value: i64) -> Int63 {
        Int63::new(value).unwrap()
    }

    fn text(s: &str) -> Str {
        Str::from(s)
    }

    /// A class decoded by hand the way the generated code does it.
    #[derive(Debug, PartialEq)]
    struct Point {
        x: Int63,
        y: Option<Int63>,
    }
    crate::mem::trace_fields!(Point, x, y);

    impl FromJson for Point {
        fn from_json(json: &Json) -> Result<Self, DecodeError> {
            let object = object(json, "Point")?;
            Ok(Self {
                x: field(object, "x", false)?,
                y: field(object, "y", true)?,
            })
        }
    }

    /// `(message, path)` of the `DecodeError` thrown for `input`.
    fn decode_error_of<T: FromJson + fmt::Debug>(input: &str) -> (String, String) {
        match deserialize::<T>(&text(input)) {
            Err(Thrown::Error(error)) => {
                assert_eq!(error.class_fqn(), "baml.json.DecodeError", "input {input}");
                let fields = error.fields();
                let get = |name: &str| -> String {
                    let (_, value) = fields.iter().find(|(n, _)| *n == name).unwrap();
                    serde_json::from_str(value).expect("a rendered string")
                };
                (get("message"), get("path"))
            }
            other => panic!("expected a DecodeError for {input}, got {other:?}"),
        }
    }

    fn message_of<T: FromJson + fmt::Debug>(input: &str) -> String {
        decode_error_of::<T>(input).0
    }

    #[test]
    fn scalars_decode_with_the_vms_messages() {
        assert_eq!(deserialize::<Int63>(&text("7")).unwrap(), int(7));
        assert_eq!(deserialize::<f64>(&text("1.5")).unwrap(), 1.5);
        assert!(deserialize::<bool>(&text("true")).unwrap());
        assert_eq!(deserialize::<Str>(&text("\"s\"")).unwrap().as_str(), "s");
        deserialize::<()>(&text("null")).unwrap();
        let range = "expected integer in the BAML int range [-2^62, 2^62 - 1]";
        assert_eq!(
            decode_error_of::<Int63>("1.5"),
            (range.into(), String::new())
        );
        assert_eq!(message_of::<Int63>("4611686018427387904"), range);
        assert_eq!(message_of::<Int63>("\"7\""), "expected integer");
        assert_eq!(message_of::<f64>("\"x\""), "expected number");
        assert_eq!(message_of::<bool>("1"), "expected boolean");
        assert_eq!(message_of::<Str>("1"), "expected string");
        assert_eq!(message_of::<()>("1"), "expected null");
        // A nullable reports its type's error, not the union's.
        assert_eq!(message_of::<Option<Int63>>("\"x\""), "expected integer");
        assert_eq!(deserialize::<Option<Int63>>(&text("null")).unwrap(), None);
    }

    #[test]
    fn containers_use_the_dispatchers_wording_and_no_path() {
        let xs = deserialize::<Shared<Vec<Int63>>>(&text("[1, 2]")).unwrap();
        assert_eq!(*xs.borrow(), vec![int(1), int(2)]);
        assert_eq!(
            decode_error_of::<Shared<Vec<Int63>>>("[1, \"2\"]"),
            ("expected integer".into(), String::new())
        );
        assert_eq!(
            message_of::<Shared<Vec<Int63>>>("{}"),
            "expected JSON array"
        );
        let p = deserialize::<Shared<Point>>(&text("{\"x\": 1, \"extra\": true}")).unwrap();
        assert_eq!(*p.borrow(), Point { x: int(1), y: None });
        assert_eq!(
            message_of::<Shared<Point>>("{\"y\": 1}"),
            "missing required field `x`"
        );
        assert_eq!(
            decode_error_of::<Shared<Point>>("{\"x\": 1, \"y\": \"no\"}"),
            ("expected integer".into(), String::new())
        );
        assert_eq!(
            message_of::<Shared<Point>>("[1, 2]"),
            "expected JSON object for class `Point`"
        );
        assert_eq!(
            decode_error_of::<Shared<Vec<Shared<Point>>>>("[{\"x\": 1}, {\"x\": \"a\"}]"),
            ("expected integer".into(), String::new())
        );
        assert_eq!(array::len(&array::new(Vec::<Int63>::new())), int(0));
    }

    #[test]
    fn parse_errors_are_the_parsers_diagnostic() {
        match deserialize::<Int63>(&text("not json")) {
            Err(Thrown::Error(error)) => {
                assert_eq!(error.class_fqn(), "baml.json.ParseError");
                assert!(error.fields()[0].1.contains("expected"));
            }
            other => panic!("expected a ParseError, got {other:?}"),
        }
        // An integral number past the bigint limit (2^28 bits) is a parse
        // error on the VM; the check is mirrored, its trigger is not
        // practical to exercise here.
        assert!(validate_bigint_bounds(&Json::from(1)).is_ok());
    }

    #[test]
    fn enums_and_unions() {
        let names = ["Red", "Blue"];
        assert_eq!(
            enum_variant(&Json::String("Blue".into()), "Color", &names).unwrap(),
            1
        );
        let err = enum_variant(&Json::String("Green".into()), "Color", &names).unwrap_err();
        assert_eq!(err.message, "unknown variant `Green` for enum `Color`");
        let err = enum_variant(&Json::Null, "Color", &names).unwrap_err();
        assert_eq!(err.message, "expected enum variant string");
        let members: [MemberDecoder<Str>; 2] = [
            |json| {
                Int63::from_json(json).map(|n| from_literal(if n.get() == 1 { "one" } else { "n" }))
            },
            Str::from_json,
        ];
        assert_eq!(union(&Json::from(1), &members).unwrap().as_str(), "one");
        assert_eq!(union(&Json::from("s"), &members).unwrap().as_str(), "s");
        let err = union(&Json::Null, &members).unwrap_err();
        assert_eq!(
            (err.message.as_str(), err.path.as_str()),
            ("no union member matched", "")
        );
    }

    #[test]
    fn serialization_is_compact_and_non_finite_floats_are_null() {
        assert_eq!(to_string(&f64::NAN).unwrap().as_str(), "null");
        assert_eq!(
            to_string(&array::new(vec![int(1), int(2)]))
                .unwrap()
                .as_str(),
            "[1,2]"
        );
    }
}
