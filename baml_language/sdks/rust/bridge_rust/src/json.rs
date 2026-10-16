//! BAML's `baml.json.json` as [`serde_json::Value`].
//!
//! `baml.json.json` is the recursive stdlib alias
//! `null | bool | int | bigint | float | string | json[] | map<string, json>`.
//! A recursive alias cannot be a plain Rust `type`, so generated SDKs map
//! it onto [`Json`] instead, covering every arm except `bigint`:
//!
//! | BAML                | [`Json`]                                     |
//! |---------------------|----------------------------------------------|
//! | `null`              | `Value::Null`                                |
//! | `bool`              | `Value::Bool`                                |
//! | `int`               | `Value::Number` holding an integer           |
//! | `float`             | `Value::Number` holding a finite float       |
//! | `string`            | `Value::String`                              |
//! | `json[]`            | `Value::Array`                               |
//! | `map<string, json>` | `Value::Object`                              |
//!
//! Object entries keep their order because this crate enables `serde_json`'s
//! `preserve_order` feature; integer-ness is checked explicitly, so the
//! mapping holds with or without `arbitrary_precision`.
//!
//! Nothing outside that table crosses the boundary silently:
//!
//! - Decoding fails with [`DecodeError::NotJson`] on a BAML `bigint`, on a
//!   non-finite float, and on any value that is not JSON at all (a class,
//!   enum, media, bytes, or handle that drifted into a json slot).
//! - Encoding fails on a number that has no exact BAML `int` or `float`
//!   form: an integer outside BAML's 63-bit `int` range (`u64::MAX`, or the
//!   arbitrarily wide integers `serde_json`'s `arbitrary_precision` feature
//!   admits) and a number whose float value is not finite. Such an integer is
//!   never rounded into a float.
//!
//! `to_baml` is infallible by trait design, so encode failures travel
//! through an *encode scope*: generated function calls encode their
//! arguments inside one, and the first failure aborts the call with
//! [`Error::Sdk`](crate::Error::Sdk) before anything reaches the engine.
//! Outside a scope (a value returned from a host callback, or arguments a
//! caller encodes by hand) the failure panics with the same message; the
//! bridge already reports a panicking host callback to BAML as a panic.

use std::{
    cell::RefCell,
    fmt::{self, Write as _},
};

use serde_json::{Number, Value};

use crate::{
    DecodeError,
    baml_value::internal::__BamlValuePrivate,
    wire::{
        self, baml_literal_value::Literal, baml_outbound_value::Value as Out,
        inbound_map_entry::Key, inbound_value::Value as In,
    },
};

/// BAML's `baml.json.json`. See the [module docs](self) for the value
/// mapping and what fails to convert.
pub type Json = Value;

/// The canonical FQN of the stdlib alias [`Json`] stands for.
const JSON_FQN: &str = "baml.json.json";

/// Smallest BAML `int`. The language's `int` is a 63-bit two's-complement
/// integer (the VM keeps a tag bit), not a full `i64`.
const BAML_INT_MIN: i64 = -(1 << 62);
/// Largest BAML `int`.
const BAML_INT_MAX: i64 = (1 << 62) - 1;

/// A [`Json`] number with no exact BAML json form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JsonEncodeError {
    /// Location of the number from the root, e.g. `$["items"][2]`.
    path: String,
    /// The number as `serde_json` holds it.
    number: String,
    reason: Unrepresentable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unrepresentable {
    /// An integer outside BAML's `int` range. Carrying it would take a BAML
    /// `bigint`, which the Rust SDK does not map inside json.
    IntegerOutOfRange,
    /// A number whose float value is infinite (or that is not a number).
    NotFinite,
}

impl fmt::Display for JsonEncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            path,
            number,
            reason,
        } = self;
        match reason {
            Unrepresentable::IntegerOutOfRange => write!(
                f,
                "cannot encode {JSON_FQN} at {path}: integer {number} is outside the BAML int \
                 range [{BAML_INT_MIN}, {BAML_INT_MAX}] (bigint is not supported inside json)"
            ),
            Unrepresentable::NotFinite => write!(
                f,
                "cannot encode {JSON_FQN} at {path}: number {number} is not a finite float"
            ),
        }
    }
}

impl std::error::Error for JsonEncodeError {}

/// One step from the root to the value being encoded. Rendered only when
/// encoding fails, so the happy path never formats a path.
enum Segment<'a> {
    Index(usize),
    Key(&'a str),
}

fn render_path(path: &[Segment<'_>]) -> String {
    let mut rendered = String::from("$");
    for segment in path {
        // Writing into a `String` cannot fail.
        let _ = match segment {
            Segment::Index(index) => write!(rendered, "[{index}]"),
            Segment::Key(key) => write!(rendered, "[{key:?}]"),
        };
    }
    rendered
}

/// Encode a [`Json`] value as an inbound wire value.
///
/// Containers carry no type annotation: the declared `json` slot is the
/// context the engine types them by, exactly like a BAML-born
/// `baml.json.parse` value.
pub(crate) fn encode(value: &Json) -> Result<wire::InboundValue, JsonEncodeError> {
    encode_at(value, &mut Vec::new())
}

fn encode_at<'a>(
    value: &'a Json,
    path: &mut Vec<Segment<'a>>,
) -> Result<wire::InboundValue, JsonEncodeError> {
    let encoded = match value {
        // An absent oneof is the wire's null.
        Value::Null => None,
        Value::Bool(value) => Some(In::BoolValue(*value)),
        Value::Number(number) => Some(encode_number(number).map_err(|reason| JsonEncodeError {
            path: render_path(path),
            number: number.to_string(),
            reason,
        })?),
        Value::String(value) => Some(In::StringValue(value.clone())),
        Value::Array(items) => {
            let mut values = Vec::with_capacity(items.len());
            for (index, item) in items.iter().enumerate() {
                path.push(Segment::Index(index));
                values.push(encode_at(item, path)?);
                path.pop();
            }
            Some(In::ListValue(wire::InboundListValue { values }))
        }
        Value::Object(entries) => {
            let mut encoded = Vec::with_capacity(entries.len());
            for (key, item) in entries {
                path.push(Segment::Key(key));
                encoded.push(wire::InboundMapEntry {
                    key: Some(Key::StringKey(key.clone())),
                    value: Some(encode_at(item, path)?),
                });
                path.pop();
            }
            Some(In::MapValue(wire::InboundMapValue { entries: encoded }))
        }
    };
    Ok(wire::InboundValue {
        value_type: None,
        value: encoded,
    })
}

/// Map a JSON number onto BAML `int` or `float` by what it *is*, never by
/// what it rounds to. Integer-ness is checked explicitly: with
/// `arbitrary_precision` (enabled by this crate), `as_f64` happily rounds an
/// integer too wide for `i64` into a float.
fn encode_number(number: &Number) -> Result<In, Unrepresentable> {
    if let Some(integer) = number.as_i64() {
        return if (BAML_INT_MIN..=BAML_INT_MAX).contains(&integer) {
            Ok(In::IntValue(integer))
        } else {
            Err(Unrepresentable::IntegerOutOfRange)
        };
    }
    // `is_f64` holds for a number with a fraction or exponent whose float
    // value is finite (and, without `arbitrary_precision`, for every stored
    // float, which `serde_json` guarantees finite). An integer too wide for
    // `i64` is never `is_f64`.
    if number.is_f64()
        && let Some(float) = number.as_f64().filter(|float| float.is_finite())
    {
        return Ok(In::FloatValue(float));
    }
    if is_integer_literal(&number.to_string()) {
        Err(Unrepresentable::IntegerOutOfRange)
    } else {
        Err(Unrepresentable::NotFinite)
    }
}

/// Whether `text` is a JSON integer literal: an optional sign, then digits.
fn is_integer_literal(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
}

/// Decode an outbound wire value as [`Json`], rejecting everything outside
/// the JSON value algebra.
pub(crate) fn decode(value: wire::BamlOutboundValue) -> Result<Json, DecodeError> {
    let value = peel_union_variants(value);
    match value.value {
        None | Some(Out::NullValue(_)) => Ok(Value::Null),
        Some(Out::BoolValue(value)) => Ok(Value::Bool(value)),
        Some(Out::IntValue(value)) => Ok(Value::Number(value.into())),
        Some(Out::FloatValue(value)) => decode_float(value),
        Some(Out::StringValue(value)) => Ok(Value::String(value)),
        Some(Out::LiteralValue(literal)) => match literal.literal {
            Some(Literal::StringValue(value)) => Ok(Value::String(value)),
            Some(Literal::IntValue(value)) => Ok(Value::Number(value.into())),
            Some(Literal::BoolValue(value)) => Ok(Value::Bool(value)),
            // Float literals ride as their decimal source text.
            Some(Literal::FloatValue(text)) => match text.parse::<f64>() {
                Ok(value) => decode_float(value),
                Err(_) => Err(not_json("malformed float literal")),
            },
            Some(Literal::BigintValue(_)) => Err(not_json("bigint")),
            None => Err(not_json("empty literal")),
        },
        Some(Out::ListValue(list)) => list
            .items
            .into_iter()
            .map(decode)
            .collect::<Result<_, _>>()
            .map(Value::Array),
        Some(Out::MapValue(map)) => {
            let mut object = serde_json::Map::with_capacity(map.entries.len());
            for entry in map.entries {
                let item = decode(entry.value.unwrap_or_default())?;
                if object.insert(entry.key, item).is_some() {
                    return Err(not_json("map with a duplicate key"));
                }
            }
            Ok(Value::Object(object))
        }
        Some(_) => Err(not_json(crate::baml_value::wire_variant_kind(&value))),
    }
}

/// Strip the `union_variant` envelopes the engine wraps json values in (the
/// alias is itself a union); the payload alone decides the JSON shape.
fn peel_union_variants(mut value: wire::BamlOutboundValue) -> wire::BamlOutboundValue {
    while let Some(Out::UnionVariantValue(variant)) = value.value {
        value = variant.value.map(|inner| *inner).unwrap_or_default();
    }
    value
}

fn decode_float(value: f64) -> Result<Json, DecodeError> {
    Number::from_f64(value)
        .map(Value::Number)
        .ok_or_else(|| not_json("non-finite float"))
}

fn not_json(got: &'static str) -> DecodeError {
    DecodeError::NotJson { got }
}

/// An open encode scope: the first failure reported inside it.
#[derive(Default)]
struct OpenScope {
    failure: Option<JsonEncodeError>,
}

thread_local! {
    /// The innermost encode scope open on this thread, if any.
    static ENCODE_SCOPE: RefCell<Option<OpenScope>> = const { RefCell::new(None) };
}

/// Run `encode` with [`Json`] encode failures collected instead of
/// panicking. Returns the first failure, if any, in place of the value.
///
/// Scopes nest: an inner scope reports only its own failures, and the outer
/// scope's state is restored on exit (also when `encode` unwinds).
pub(crate) fn encode_scope<T>(encode: impl FnOnce() -> T) -> Result<T, JsonEncodeError> {
    struct Restore(Option<OpenScope>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let outer = self.0.take();
            ENCODE_SCOPE.with(|scope| *scope.borrow_mut() = outer);
        }
    }
    let _restore = Restore(ENCODE_SCOPE.with(|scope| scope.replace(Some(OpenScope::default()))));
    let value = encode();
    let failure = ENCODE_SCOPE.with(|scope| {
        scope
            .borrow_mut()
            .as_mut()
            .and_then(|open| open.failure.take())
    });
    match failure {
        Some(error) => Err(error),
        None => Ok(value),
    }
}

/// Hand an encode failure to the open scope, or panic when there is none.
fn report(error: JsonEncodeError) {
    let unscoped = ENCODE_SCOPE.with(|scope| match scope.borrow_mut().as_mut() {
        Some(open) => {
            open.failure.get_or_insert(error);
            None
        }
        None => Some(error),
    });
    if let Some(error) = unscoped {
        panic!("{error}");
    }
}

impl __BamlValuePrivate for Json {
    fn to_baml(&self) -> wire::InboundValue {
        encode(self).unwrap_or_else(|error| {
            report(error);
            // Only reached inside an encode scope, which discards the whole
            // encoding once it has seen a failure.
            wire::InboundValue {
                value_type: None,
                value: None,
            }
        })
    }

    fn from_baml(value: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        decode(value)
    }

    fn baml_ty() -> wire::BamlTy {
        wire::BamlTy {
            ty: Some(wire::baml_ty::Ty::TypeAlias(wire::BamlTyTypeAlias {
                name: JSON_FQN.to_string(),
                type_args: Vec::new(),
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// What the engine sends back for an inbound value it echoes: the same
    /// tree on the outbound side, with every node wrapped in a
    /// `union_variant` envelope the way a value read through the json alias
    /// is.
    fn echoed(value: wire::InboundValue) -> wire::BamlOutboundValue {
        let bare = match value.value {
            None => None,
            Some(In::BoolValue(value)) => Some(Out::BoolValue(value)),
            Some(In::IntValue(value)) => Some(Out::IntValue(value)),
            Some(In::FloatValue(value)) => Some(Out::FloatValue(value)),
            Some(In::StringValue(value)) => Some(Out::StringValue(value)),
            Some(In::ListValue(list)) => Some(Out::ListValue(wire::BamlValueList {
                item_type: None,
                items: list.values.into_iter().map(echoed).collect(),
            })),
            Some(In::MapValue(map)) => Some(Out::MapValue(wire::BamlValueMap {
                key_type: None,
                value_type: None,
                entries: map
                    .entries
                    .into_iter()
                    .map(|entry| wire::BamlOutboundMapEntry {
                        key: match entry.key {
                            Some(Key::StringKey(key)) => key,
                            other => panic!("json encodes string keys only, got {other:?}"),
                        },
                        value: entry.value.map(echoed),
                    })
                    .collect(),
            })),
            Some(other) => panic!("json never encodes {other:?}"),
        };
        wire::BamlOutboundValue {
            value: Some(Out::UnionVariantValue(Box::new(
                wire::BamlValueUnionVariant {
                    value: Some(Box::new(wire::BamlOutboundValue { value: bare })),
                    ..Default::default()
                },
            ))),
        }
    }

    fn round_trip(value: &Json) -> Json {
        decode(echoed(encode(value).expect("encodes"))).expect("decodes")
    }

    fn outbound(value: Out) -> wire::BamlOutboundValue {
        wire::BamlOutboundValue { value: Some(value) }
    }

    fn encode_error(value: &Json) -> String {
        encode(value)
            .expect_err("value must not encode")
            .to_string()
    }

    #[test]
    fn nested_objects_and_arrays_round_trip() {
        let value = json!({
            "null": null,
            "bool": true,
            "int": -3,
            "float": 2.25,
            "string": "text",
            "list": [null, false, 1, 0.5, "s", [], {}],
            "object": { "child": [{ "deep": ["found"] }] },
        });
        assert_eq!(round_trip(&value), value);
    }

    #[test]
    fn object_key_order_is_preserved_both_ways() {
        let value = json!({ "zeta": 1, "alpha": 2, "mid": { "b": 1, "a": 2 } });
        let keys =
            |value: &Json| -> Vec<String> { value.as_object().unwrap().keys().cloned().collect() };
        let decoded = round_trip(&value);
        assert_eq!(keys(&decoded), ["zeta", "alpha", "mid"]);
        assert_eq!(keys(&decoded["mid"]), ["b", "a"]);

        let In::MapValue(map) = encode(&value).unwrap().value.unwrap() else {
            panic!("objects encode as maps");
        };
        let wire_keys: Vec<_> = map
            .entries
            .iter()
            .map(|entry| match &entry.key {
                Some(Key::StringKey(key)) => key.as_str(),
                other => panic!("unexpected key {other:?}"),
            })
            .collect();
        assert_eq!(wire_keys, ["zeta", "alpha", "mid"]);
    }

    #[test]
    fn integers_round_trip_as_baml_int_up_to_its_bounds() {
        for integer in [0, -1, BAML_INT_MIN, BAML_INT_MAX] {
            let value = json!(integer);
            assert_eq!(
                encode(&value).unwrap().value,
                Some(In::IntValue(integer)),
                "{integer}"
            );
            let decoded = round_trip(&value);
            assert_eq!(decoded.as_i64(), Some(integer));
            assert!(!decoded.is_f64(), "{integer} must stay an integer");
        }
    }

    #[test]
    fn floats_round_trip_as_baml_float_even_when_integral() {
        for float in [0.5, -1.25, 2.0, 1e300, f64::MIN_POSITIVE] {
            let value = Value::Number(Number::from_f64(float).unwrap());
            assert_eq!(
                encode(&value).unwrap().value,
                Some(In::FloatValue(float)),
                "{float}"
            );
            let decoded = round_trip(&value);
            assert_eq!(decoded.as_f64(), Some(float));
            assert!(decoded.is_f64(), "{float} must stay a float");
        }
        // Exponent notation is a float in JSON, however it reads.
        let parsed: Json = serde_json::from_str("1e2").unwrap();
        assert_eq!(encode(&parsed).unwrap().value, Some(In::FloatValue(100.0)));
    }

    #[test]
    fn integers_outside_the_baml_int_range_fail_to_encode() {
        let message = encode_error(&json!({ "ids": [1, u64::MAX] }));
        assert_eq!(
            message,
            "cannot encode baml.json.json at $[\"ids\"][1]: integer 18446744073709551615 is \
             outside the BAML int range [-4611686018427387904, 4611686018427387903] (bigint \
             is not supported inside json)"
        );
        // Full-width i64 values fit Rust but not BAML's 63-bit int.
        assert!(encode_error(&json!(i64::MAX)).contains("integer 9223372036854775807"));
        assert!(encode_error(&json!(i64::MIN)).contains("integer -9223372036854775808"));
        // `arbitrary_precision` keeps wider integers exact; they must not
        // round into a float.
        let wide: Json = serde_json::from_str("[123456789012345678901234567890]").unwrap();
        assert!(
            encode_error(&wide).contains("at $[0]: integer 123456789012345678901234567890"),
            "{}",
            encode_error(&wide)
        );
    }

    #[test]
    fn non_finite_numbers_fail_to_encode() {
        // A float literal too large for f64 parses (and stays exact) under
        // `arbitrary_precision`; it has no finite float value.
        // (`serde_json` normalizes the exponent's text to `1e+400`.)
        let huge: Json = serde_json::from_str("{\"x\": 1e400}").unwrap();
        assert_eq!(
            encode_error(&huge),
            "cannot encode baml.json.json at $[\"x\"]: number 1e+400 is not a finite float"
        );
    }

    #[test]
    fn bigint_values_fail_to_decode() {
        assert_eq!(
            decode(outbound(Out::BigintValue("ff".to_string()))),
            Err(DecodeError::NotJson { got: "bigint" })
        );
        let literal = outbound(Out::LiteralValue(wire::BamlLiteralValue {
            literal: Some(Literal::BigintValue("ff".to_string())),
        }));
        assert_eq!(decode(literal), Err(DecodeError::NotJson { got: "bigint" }));
        // Nested inside an otherwise valid value.
        let nested = outbound(Out::ListValue(wire::BamlValueList {
            item_type: None,
            items: vec![
                outbound(Out::IntValue(1)),
                outbound(Out::BigintValue("-1".to_string())),
            ],
        }));
        assert_eq!(decode(nested), Err(DecodeError::NotJson { got: "bigint" }));
    }

    #[test]
    fn non_finite_floats_fail_to_decode() {
        for float in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                decode(outbound(Out::FloatValue(float))),
                Err(DecodeError::NotJson {
                    got: "non-finite float"
                }),
                "{float}"
            );
        }
    }

    #[test]
    fn non_json_values_fail_to_decode() {
        let class = outbound(Out::ClassValue(wire::BamlValueClass {
            name: "user.Point".to_string(),
            fields: Vec::new(),
            type_args: Vec::new(),
        }));
        let enum_value = outbound(Out::EnumValue(wire::BamlValueEnum {
            name: "user.Color".to_string(),
            value: "Red".to_string(),
            ..Default::default()
        }));
        let media = outbound(Out::MediaValue(wire::BamlValueMedia::default()));
        let bytes = outbound(Out::Uint8arrayValue(vec![1, 2]));
        for (value, got) in [
            (class, "class"),
            (enum_value, "enum"),
            (media, "media"),
            (bytes, "uint8array"),
        ] {
            assert_eq!(decode(value), Err(DecodeError::NotJson { got }));
        }
        // A non-JSON value nested in a map fails the whole decode.
        let nested = outbound(Out::MapValue(wire::BamlValueMap {
            key_type: None,
            value_type: None,
            entries: vec![wire::BamlOutboundMapEntry {
                key: "image".to_string(),
                value: Some(outbound(Out::MediaValue(wire::BamlValueMedia::default()))),
            }],
        }));
        assert_eq!(decode(nested), Err(DecodeError::NotJson { got: "media" }));
    }

    #[test]
    fn duplicate_map_keys_fail_to_decode() {
        let entry = |value| wire::BamlOutboundMapEntry {
            key: "k".to_string(),
            value: Some(outbound(Out::IntValue(value))),
        };
        let value = outbound(Out::MapValue(wire::BamlValueMap {
            key_type: None,
            value_type: None,
            entries: vec![entry(1), entry(2)],
        }));
        assert_eq!(
            decode(value),
            Err(DecodeError::NotJson {
                got: "map with a duplicate key"
            })
        );
    }

    #[test]
    fn literal_envelopes_decode_to_plain_values() {
        let literal = |literal| {
            outbound(Out::LiteralValue(wire::BamlLiteralValue {
                literal: Some(literal),
            }))
        };
        assert_eq!(
            decode(literal(Literal::StringValue("ok".to_string()))),
            Ok(json!("ok"))
        );
        assert_eq!(decode(literal(Literal::IntValue(7))), Ok(json!(7)));
        assert_eq!(decode(literal(Literal::BoolValue(true))), Ok(json!(true)));
        assert_eq!(
            decode(literal(Literal::FloatValue("1.5".to_string()))),
            Ok(json!(1.5))
        );
        assert_eq!(
            decode(literal(Literal::FloatValue("not a float".to_string()))),
            Err(DecodeError::NotJson {
                got: "malformed float literal"
            })
        );
    }

    #[test]
    fn optional_json_decodes_null_as_none() {
        assert_eq!(
            Option::<Json>::from_baml(wire::BamlOutboundValue { value: None }),
            Ok(None)
        );
        assert_eq!(
            Option::<Json>::from_baml(outbound(Out::NullValue(wire::BamlValueNull {}))),
            Ok(None)
        );
        assert_eq!(
            Option::<Json>::from_baml(outbound(Out::StringValue("x".to_string()))),
            Ok(Some(json!("x")))
        );
        // `Some(Null)` is the same BAML value as `None`.
        assert_eq!(Some(Value::Null).to_baml().value, None);
    }

    #[test]
    fn baml_ty_names_the_canonical_alias() {
        assert_eq!(
            Json::baml_ty().ty,
            Some(wire::baml_ty::Ty::TypeAlias(wire::BamlTyTypeAlias {
                name: "baml.json.json".to_string(),
                type_args: Vec::new(),
            }))
        );
    }

    #[test]
    fn encode_scope_collects_the_first_failure() {
        let values = [json!(1), json!(u64::MAX), json!([u64::MAX])];
        let result = encode_scope(|| values.iter().map(Json::to_baml).collect::<Vec<_>>());
        let error = result.expect_err("the scope must report the failure");
        assert!(
            error
                .to_string()
                .contains("at $: integer 18446744073709551615")
        );

        // A clean scope returns the encoding, and no state leaks into it from
        // the failed one.
        let encoded = encode_scope(|| json!([1, "a"]).to_baml()).expect("encodes");
        assert!(matches!(encoded.value, Some(In::ListValue(_))));
    }

    #[test]
    fn encode_scopes_nest() {
        let outer = encode_scope(|| {
            let inner = encode_scope(|| json!(u64::MAX).to_baml());
            assert!(inner.is_err(), "the inner scope owns its failure");
            json!(1).to_baml()
        });
        assert!(outer.is_ok(), "the inner failure must not leak outward");
    }

    #[test]
    #[should_panic(expected = "integer 18446744073709551615 is outside the BAML int range")]
    fn encode_failure_outside_a_scope_panics() {
        let _ = json!(u64::MAX).to_baml();
    }
}
