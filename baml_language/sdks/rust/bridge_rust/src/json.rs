//! [`Json`]: BAML's `baml.json.json` as `serde_json::Value`.

use num_bigint::BigInt;

use crate::{
    DecodeError,
    baml_value::internal::__BamlValuePrivate,
    serde_json::{Number, Value},
    wire::{
        self, baml_outbound_value::Value as Out, inbound_map_entry::Key, inbound_value::Value as In,
    },
};

/// BAML's `baml.json.json`, the recursive stdlib alias
/// `null | bool | int | bigint | float | string | json[] | map<string, json>`.
///
/// - Integers: BAML `int` is 63-bit (the VM keeps a tag bit), so an integer
///   outside `[-2^62, 2^62)` crosses as a BAML `bigint`, as
///   `baml.json.parse` reads such a token, and a BAML `bigint` decodes when
///   it fits `i64` or `u64`. Every `i64` and `u64` round-trips.
/// - Floats: finite `f64`. An integral float stays a float.
/// - Objects keep the key order `serde_json::Map` gives them: sorted,
///   unless the build enables `serde_json`'s `preserve_order` feature.
///
/// Decoding fails with [`DecodeError::NotJson`] on a `bigint` outside
/// `i64`/`u64`, a non-finite float, or a value that is not JSON at all (a
/// class, enum, media, bytes or handle).
///
/// Only when another crate in the build enables `serde_json`'s
/// `arbitrary_precision` feature can a number fall outside `i64`/`u64`/`f64`:
/// a wider integer is sent exactly as a `bigint`, and a float beyond `f64`'s
/// range (`1e400`) is sent as infinity, as `str::parse::<f64>` reads it.
pub type Json = Value;

/// Smallest BAML `int`.
const BAML_INT_MIN: i64 = -(1 << 62);
/// Largest BAML `int`.
const BAML_INT_MAX: i64 = (1 << 62) - 1;

/// Containers carry no type annotation: the declared `json` slot types
/// them, exactly like a BAML-born `baml.json.parse` value.
fn encode(value: &Json) -> wire::InboundValue {
    let value = match value {
        // An absent oneof is the wire's null.
        Value::Null => None,
        Value::Bool(value) => Some(In::BoolValue(*value)),
        Value::Number(number) => Some(encode_number(number)),
        Value::String(value) => Some(In::StringValue(value.clone())),
        Value::Array(items) => Some(In::ListValue(wire::InboundListValue {
            values: items.iter().map(encode).collect(),
        })),
        Value::Object(entries) => Some(In::MapValue(wire::InboundMapValue {
            entries: entries
                .iter()
                .map(|(key, item)| wire::InboundMapEntry {
                    key: Some(Key::StringKey(key.clone())),
                    value: Some(encode(item)),
                })
                .collect(),
        })),
    };
    wire::InboundValue {
        value_type: None,
        value,
    }
}

fn encode_number(number: &Number) -> In {
    if let Some(int) = number
        .as_i64()
        .filter(|int| (BAML_INT_MIN..=BAML_INT_MAX).contains(int))
    {
        return In::IntValue(int);
    }
    // `as_f64` also rounds integers, so ask what the number *is*.
    if let Some(float) = number.as_f64().filter(|_| number.is_f64()) {
        return In::FloatValue(float);
    }
    let text = number.to_string();
    match text.parse::<BigInt>() {
        Ok(int) => In::BigintValue(format!("{int:x}")),
        // Only an `arbitrary_precision` number beyond `f64`'s range.
        Err(_) if text.starts_with('-') => In::FloatValue(f64::NEG_INFINITY),
        Err(_) => In::FloatValue(f64::INFINITY),
    }
}

fn decode(value: wire::BamlOutboundValue) -> Result<Json, DecodeError> {
    let value = crate::decode::unwrap(value);
    let not_json = |got| Err(DecodeError::NotJson { got });
    match value.value {
        None | Some(Out::NullValue(_)) => Ok(Value::Null),
        Some(Out::BoolValue(value)) => Ok(Value::Bool(value)),
        Some(Out::IntValue(value)) => Ok(Value::Number(value.into())),
        Some(Out::BigintValue(hex)) => {
            let int = BigInt::parse_bytes(hex.as_bytes(), 16)
                .ok_or(DecodeError::InvalidBigint { len: hex.len() })?;
            match (i64::try_from(&int), u64::try_from(&int)) {
                (Ok(int), _) => Ok(Value::Number(int.into())),
                (_, Ok(int)) => Ok(Value::Number(int.into())),
                _ => not_json("an integer outside the i64/u64 range"),
            }
        }
        Some(Out::FloatValue(value)) => match Number::from_f64(value) {
            Some(number) => Ok(Value::Number(number)),
            None => not_json("a non-finite float"),
        },
        Some(Out::StringValue(value)) => Ok(Value::String(value)),
        Some(Out::ListValue(list)) => list
            .items
            .into_iter()
            .map(decode)
            .collect::<Result<_, _>>()
            .map(Value::Array),
        Some(Out::MapValue(map)) => map
            .entries
            .into_iter()
            .map(|entry| Ok((entry.key, decode(entry.value.unwrap_or_default())?)))
            .collect::<Result<_, _>>()
            .map(Value::Object),
        Some(_) => not_json(crate::baml_value::wire_variant_kind(&value)),
    }
}

impl __BamlValuePrivate for Json {
    fn to_baml(&self) -> wire::InboundValue {
        encode(self)
    }

    fn from_baml(value: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        decode(value)
    }

    fn baml_ty() -> wire::BamlTy {
        wire::BamlTy {
            ty: Some(wire::baml_ty::Ty::TypeAlias(wire::BamlTyTypeAlias {
                name: "baml.json.json".to_string(),
                type_args: Vec::new(),
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        serde_json::{self, json},
        wire::baml_literal_value::Literal,
    };

    /// What the engine sends back for an inbound value it echoes: the same
    /// tree on the outbound side, every node in the `union_variant`
    /// envelope a value read through the json alias carries.
    fn echoed(value: wire::InboundValue) -> wire::BamlOutboundValue {
        let bare = value.value.map(|value| match value {
            In::BoolValue(value) => Out::BoolValue(value),
            In::IntValue(value) => Out::IntValue(value),
            In::BigintValue(value) => Out::BigintValue(value),
            In::FloatValue(value) => Out::FloatValue(value),
            In::StringValue(value) => Out::StringValue(value),
            In::ListValue(list) => Out::ListValue(wire::BamlValueList {
                item_type: None,
                items: list.values.into_iter().map(echoed).collect(),
            }),
            In::MapValue(map) => Out::MapValue(wire::BamlValueMap {
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
            }),
            other => panic!("json never encodes {other:?}"),
        });
        outbound(Out::UnionVariantValue(Box::new(
            wire::BamlValueUnionVariant {
                value: Some(Box::new(wire::BamlOutboundValue { value: bare })),
                ..Default::default()
            },
        )))
    }

    fn outbound(value: Out) -> wire::BamlOutboundValue {
        wire::BamlOutboundValue { value: Some(value) }
    }

    fn round_trip(value: &Json) -> Json {
        Json::from_baml(echoed(value.to_baml())).expect("decodes")
    }

    fn not_json(got: &'static str) -> Result<Json, DecodeError> {
        Err(DecodeError::NotJson { got })
    }

    #[test]
    fn nested_values_round_trip() {
        let value = json!({
            "null": null,
            "bool": true,
            "int": -3,
            "float": 2.25,
            "integral_float": 2.0,
            "string": "text",
            "list": [null, false, 1, 0.5, "s", [], {}],
            "object": { "child": [{ "deep": ["found"] }] },
        });
        let decoded = round_trip(&value);
        assert_eq!(decoded, value);
        assert!(decoded["integral_float"].is_f64());
        assert!(decoded["int"].is_i64());
    }

    #[test]
    fn integers_outside_baml_int_cross_as_bigint_and_round_trip() {
        for int in [0, BAML_INT_MIN, BAML_INT_MAX] {
            assert_eq!(json!(int).to_baml().value, Some(In::IntValue(int)));
        }
        for (value, hex) in [
            (json!(BAML_INT_MAX + 1), "4000000000000000"),
            (json!(BAML_INT_MIN - 1), "-4000000000000001"),
            (json!(i64::MAX), "7fffffffffffffff"),
            (json!(i64::MIN), "-8000000000000000"),
            (json!(u64::MAX), "ffffffffffffffff"),
        ] {
            assert_eq!(
                value.to_baml().value,
                Some(In::BigintValue(hex.to_string())),
                "{value}"
            );
            let decoded = round_trip(&value);
            assert_eq!(decoded, value);
            assert!(!decoded.is_f64(), "{value} must stay an integer");
        }
    }

    #[test]
    fn numbers_only_arbitrary_precision_admits() {
        // Without `arbitrary_precision` serde_json reads the integer as a
        // float and rejects `1e400`; with it (another crate may enable it),
        // both stay exact text.
        let text = "123456789012345678901234567890";
        let wide: Json = serde_json::from_str(text).unwrap();
        if !wide.is_f64() {
            let hex = format!("{:x}", text.parse::<BigInt>().unwrap());
            assert_eq!(wide.to_baml().value, Some(In::BigintValue(hex)));
            assert_eq!(
                Json::from_baml(echoed(wide.to_baml())),
                not_json("an integer outside the i64/u64 range")
            );
        }
        if let Ok(huge) = serde_json::from_str::<Json>("[1e400, -1e400]") {
            let In::ListValue(list) = huge.to_baml().value.unwrap() else {
                panic!("arrays encode as lists");
            };
            let floats: Vec<_> = list.values.into_iter().map(|value| value.value).collect();
            assert_eq!(
                floats,
                [
                    Some(In::FloatValue(f64::INFINITY)),
                    Some(In::FloatValue(f64::NEG_INFINITY))
                ]
            );
        }
    }

    #[test]
    fn values_json_cannot_hold_fail_to_decode() {
        let bigint = |hex: &str| outbound(Out::BigintValue(hex.to_string()));
        // 2^64 and -(2^63) - 1.
        for hex in ["10000000000000000", "-8000000000000001"] {
            assert_eq!(
                Json::from_baml(bigint(hex)),
                not_json("an integer outside the i64/u64 range")
            );
        }
        assert_eq!(
            Json::from_baml(bigint("xyz")),
            Err(DecodeError::InvalidBigint { len: 3 })
        );
        for float in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                Json::from_baml(outbound(Out::FloatValue(float))),
                not_json("a non-finite float")
            );
        }
        let class = outbound(Out::ClassValue(wire::BamlValueClass {
            name: "user.Point".to_string(),
            ..Default::default()
        }));
        let media = outbound(Out::MediaValue(wire::BamlValueMedia::default()));
        let bytes = outbound(Out::Uint8arrayValue(vec![1]));
        for (value, got) in [(class, "class"), (media, "media"), (bytes, "uint8array")] {
            assert_eq!(Json::from_baml(value), not_json(got));
        }
        // Nested inside an otherwise valid value, the whole decode fails.
        let nested = outbound(Out::ListValue(wire::BamlValueList {
            item_type: None,
            items: vec![outbound(Out::IntValue(1)), bigint("10000000000000000")],
        }));
        assert_eq!(
            Json::from_baml(nested),
            not_json("an integer outside the i64/u64 range")
        );
    }

    #[test]
    fn literal_envelopes_decode_to_plain_values() {
        let literal = |literal| {
            Json::from_baml(outbound(Out::LiteralValue(wire::BamlLiteralValue {
                literal: Some(literal),
            })))
        };
        assert_eq!(
            literal(Literal::StringValue("ok".to_string())),
            Ok(json!("ok"))
        );
        assert_eq!(literal(Literal::IntValue(7)), Ok(json!(7)));
        assert_eq!(literal(Literal::BoolValue(true)), Ok(json!(true)));
        assert_eq!(
            literal(Literal::FloatValue("1.5".to_string())),
            Ok(json!(1.5))
        );
        assert_eq!(
            literal(Literal::BigintValue("ffffffffffffffff".to_string())),
            Ok(json!(u64::MAX))
        );
    }

    #[test]
    fn optional_json_decodes_null_as_none() {
        let null = wire::BamlOutboundValue { value: None };
        assert_eq!(Option::<Json>::from_baml(null), Ok(None));
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

    /// `CallOptions::argument` encodes eagerly, outside any call. A json
    /// number BAML `int` cannot hold must still encode (as a bigint), not
    /// panic.
    #[test]
    fn call_options_accept_any_json_number() {
        let mut kwargs = Vec::new();
        crate::CallOptions::new()
            .argument("extra", json!({ "id": u64::MAX }))
            .append_to(&mut kwargs);
        let Some(In::MapValue(map)) = kwargs[0].value.clone().unwrap().value else {
            panic!("objects encode as maps");
        };
        assert_eq!(
            map.entries[0].value.clone().unwrap().value,
            Some(In::BigintValue("ffffffffffffffff".to_string()))
        );
    }
}
