//! CAS codec-1 (`BamlOutboundValue`) → neutral [`Value`] decoder (B4,
//! TASK/baml-query-scope.md §5.5).
//!
//! The prost mirror below covers exactly the tags the engine's trace
//! encoder emits (`trace_value_encode.rs`: 2, 3, 4, 5, 6, 7, 8, 11, 12,
//! 17, 19, 20); unknown fields (e.g. class `type_args`) are skipped by
//! prost's proto3 semantics, never an error.

use baml_query::{
    outcome::UnavailableReason,
    value::model::{MediaContent, Presence, Value},
};
use prost::Message;

#[derive(Clone, PartialEq, Message)]
struct PbValue {
    #[prost(oneof = "PbVariant", tags = "2, 3, 4, 5, 6, 7, 8, 11, 12, 17, 19, 20")]
    value: Option<PbVariant>,
}

#[derive(Clone, PartialEq, ::prost::Oneof)]
enum PbVariant {
    #[prost(message, tag = "2")]
    Null(PbNull),
    #[prost(string, tag = "3")]
    String(String),
    #[prost(int64, tag = "4")]
    Int(i64),
    #[prost(double, tag = "5")]
    Float(f64),
    #[prost(bool, tag = "6")]
    Bool(bool),
    #[prost(message, tag = "7")]
    Class(PbClass),
    #[prost(message, tag = "8")]
    Enum(PbEnum),
    #[prost(message, tag = "11")]
    List(PbList),
    #[prost(message, tag = "12")]
    Map(PbMap),
    #[prost(message, tag = "17")]
    Media(PbMedia),
    #[prost(bytes, tag = "19")]
    Bytes(Vec<u8>),
    #[prost(string, tag = "20")]
    Bigint(String),
}

#[derive(Clone, PartialEq, Message)]
#[expect(
    clippy::empty_structs_with_brackets,
    reason = "prost empty message structs use braced form"
)]
struct PbNull {}

#[derive(Clone, PartialEq, Message)]
struct PbList {
    #[prost(message, repeated, tag = "2")]
    items: Vec<PbValue>,
}

#[derive(Clone, PartialEq, Message)]
struct PbMapEntry {
    #[prost(string, tag = "1")]
    key: String,
    #[prost(message, optional, tag = "2")]
    value: Option<PbValue>,
}

#[derive(Clone, PartialEq, Message)]
struct PbMap {
    #[prost(message, repeated, tag = "3")]
    entries: Vec<PbMapEntry>,
}

#[derive(Clone, PartialEq, Message)]
struct PbClass {
    #[prost(string, tag = "1")]
    name: String,
    #[prost(message, repeated, tag = "2")]
    fields: Vec<PbMapEntry>,
}

#[derive(Clone, PartialEq, Message)]
struct PbEnum {
    #[prost(string, tag = "1")]
    name: String,
    #[prost(string, tag = "2")]
    value: String,
}

#[derive(Clone, PartialEq, Message)]
struct PbMedia {
    #[prost(enumeration = "PbMediaKind", tag = "1")]
    media: i32,
    #[prost(string, optional, tag = "2")]
    mime_type: Option<String>,
    #[prost(oneof = "PbMediaValue", tags = "3, 4, 5")]
    value: Option<PbMediaValue>,
}

#[derive(Clone, PartialEq, ::prost::Oneof)]
enum PbMediaValue {
    #[prost(string, tag = "3")]
    Url(String),
    #[prost(string, tag = "4")]
    Base64(String),
    #[prost(string, tag = "5")]
    File(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ::prost::Enumeration)]
#[repr(i32)]
enum PbMediaKind {
    Unspecified = 0,
    Image = 1,
    Audio = 2,
    Pdf = 3,
    Video = 4,
    Other = 5,
}

/// The class name the engine uses for elided subtrees
/// (`trace_value_encode::omission_to_class`).
const OMITTED_CLASS: &str = "baml.trace.OmittedValue";

/// Decode one codec-1 body. `Err` carries a typed unavailability
/// (corrupt bytes, depth cap).
pub(crate) fn decode_codec1(body: &[u8], max_depth: u32) -> Result<Value, UnavailableReason> {
    let decoded = PbValue::decode(body).map_err(|_| UnavailableReason::Corrupt)?;
    convert(&decoded, max_depth)
}

fn convert(value: &PbValue, depth_left: u32) -> Result<Value, UnavailableReason> {
    if depth_left == 0 {
        return Err(UnavailableReason::Truncated);
    }
    let next = depth_left - 1;
    Ok(match &value.value {
        // The engine's encoder always sets a variant; a decoded `None`
        // means the oneof carried a tag this mirror does not know (a
        // NEWER engine added one). Surface that as typed unavailability
        // of the subtree rather than a silent SQL NULL, so codec drift
        // degrades loudly.
        None => Value::Omitted {
            reason: "unknown codec-1 value variant (newer producer?)".to_string(),
        },
        Some(PbVariant::Null(_)) => Value::Null,
        Some(PbVariant::String(s)) => Value::String(s.clone()),
        Some(PbVariant::Int(i)) => Value::Int(*i),
        Some(PbVariant::Float(f)) => Value::Float(*f),
        Some(PbVariant::Bool(b)) => Value::Bool(*b),
        Some(PbVariant::Bytes(b)) => Value::Bytes(b.clone()),
        // The bridge wire carries bigints in base-16; the neutral model
        // is minimal decimal.
        Some(PbVariant::Bigint(hex)) => Value::BigInt(bigint_hex_to_decimal(hex)),
        Some(PbVariant::List(list)) => Value::List(
            list.items
                .iter()
                .map(|item| convert(item, next))
                .collect::<Result<_, _>>()?,
        ),
        Some(PbVariant::Map(map)) => Value::Map(
            map.entries
                .iter()
                .map(|entry| {
                    Ok((
                        entry.key.clone(),
                        convert_entry(entry.value.as_ref(), next)?,
                    ))
                })
                .collect::<Result<_, UnavailableReason>>()?,
        ),
        Some(PbVariant::Class(class)) if class.name == OMITTED_CLASS => Value::Omitted {
            reason: omission_reason(class),
        },
        Some(PbVariant::Class(class)) => Value::Class {
            name: class.name.clone(),
            fields: class
                .fields
                .iter()
                .map(|entry| {
                    let value = convert_entry(entry.value.as_ref(), next)?;
                    Ok(match value {
                        Value::Null => (entry.key.clone(), Presence::Null, None),
                        value => (entry.key.clone(), Presence::Present, Some(value)),
                    })
                })
                .collect::<Result<_, UnavailableReason>>()?,
        },
        Some(PbVariant::Enum(e)) => Value::Enum {
            name: e.name.clone(),
            variant: e.value.clone(),
        },
        Some(PbVariant::Media(media)) => Value::Media {
            kind: match PbMediaKind::try_from(media.media) {
                Ok(PbMediaKind::Image) => "image",
                Ok(PbMediaKind::Audio) => "audio",
                Ok(PbMediaKind::Pdf) => "pdf",
                Ok(PbMediaKind::Video) => "video",
                _ => "media",
            }
            .to_string(),
            mime: media.mime_type.clone().unwrap_or_default(),
            content: match &media.value {
                Some(PbMediaValue::Url(url)) => MediaContent::Url(url.clone()),
                Some(PbMediaValue::File(file)) => MediaContent::Url(file.clone()),
                Some(PbMediaValue::Base64(b64)) => {
                    use base64::Engine as _;
                    match base64::engine::general_purpose::STANDARD.decode(b64) {
                        Ok(bytes) => MediaContent::Bytes(bytes),
                        Err(_) => MediaContent::Bytes(b64.clone().into_bytes()),
                    }
                }
                None => MediaContent::Bytes(Vec::new()),
            },
        },
    })
}

fn convert_entry(value: Option<&PbValue>, depth_left: u32) -> Result<Value, UnavailableReason> {
    value.map_or(Ok(Value::Null), |value| convert(value, depth_left))
}

fn omission_reason(class: &PbClass) -> String {
    let field = |key: &str| {
        class.fields.iter().find(|f| f.key == key).and_then(|f| {
            match f.value.as_ref().and_then(|v| v.value.as_ref()) {
                Some(PbVariant::String(s)) => Some(s.clone()),
                _ => None,
            }
        })
    };
    match (field("reason"), field("message")) {
        (Some(reason), Some(message)) if !message.is_empty() => format!("{reason}: {message}"),
        (Some(reason), _) => reason,
        (None, Some(message)) => message,
        (None, None) => "omitted".to_string(),
    }
}

fn bigint_hex_to_decimal(hex: &str) -> String {
    let (negative, digits) = hex
        .strip_prefix('-')
        .map_or((false, hex), |rest| (true, rest));
    num_bigint::BigUint::parse_bytes(digits.as_bytes(), 16).map_or_else(
        || hex.to_string(),
        |value| {
            if negative {
                format!("-{value}")
            } else {
                value.to_string()
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pb(variant: PbVariant) -> PbValue {
        PbValue {
            value: Some(variant),
        }
    }

    #[test]
    fn scalars_lists_maps_and_classes_decode() {
        let body = pb(PbVariant::Map(PbMap {
            entries: vec![
                PbMapEntry {
                    key: "n".into(),
                    value: Some(pb(PbVariant::Int(7))),
                },
                PbMapEntry {
                    key: "who".into(),
                    value: Some(pb(PbVariant::Class(PbClass {
                        name: "user.Person".into(),
                        fields: vec![
                            PbMapEntry {
                                key: "name".into(),
                                value: Some(pb(PbVariant::String("ada".into()))),
                            },
                            PbMapEntry {
                                key: "nick".into(),
                                value: Some(pb(PbVariant::Null(PbNull {}))),
                            },
                        ],
                    }))),
                },
            ],
        }))
        .encode_to_vec();
        let value = decode_codec1(&body, 16).unwrap();
        let Value::Map(entries) = &value else {
            panic!("map root");
        };
        assert_eq!(entries[0], ("n".into(), Value::Int(7)));
        let Value::Class { name, fields } = &entries[1].1 else {
            panic!("class value");
        };
        assert_eq!(name, "user.Person");
        assert_eq!(
            fields[0],
            (
                "name".into(),
                Presence::Present,
                Some(Value::String("ada".into()))
            )
        );
        assert_eq!(fields[1], ("nick".into(), Presence::Null, None));
    }

    #[test]
    fn bigints_convert_from_wire_hex_to_minimal_decimal() {
        let body = pb(PbVariant::Bigint("ff".into())).encode_to_vec();
        assert_eq!(
            decode_codec1(&body, 4).unwrap(),
            Value::BigInt("255".into())
        );
        let body = pb(PbVariant::Bigint("-a".into())).encode_to_vec();
        assert_eq!(
            decode_codec1(&body, 4).unwrap(),
            Value::BigInt("-10".into())
        );
    }

    #[test]
    fn omission_classes_become_omitted_values() {
        let body = pb(PbVariant::Class(PbClass {
            name: OMITTED_CLASS.into(),
            fields: vec![
                PbMapEntry {
                    key: "reason".into(),
                    value: Some(pb(PbVariant::String("hostOwnedValue".into()))),
                },
                PbMapEntry {
                    key: "message".into(),
                    value: Some(pb(PbVariant::String("host-owned callable".into()))),
                },
            ],
        }))
        .encode_to_vec();
        assert_eq!(
            decode_codec1(&body, 4).unwrap(),
            Value::Omitted {
                reason: "hostOwnedValue: host-owned callable".into()
            }
        );
    }

    #[test]
    fn depth_cap_is_typed_truncation_and_garbage_is_corrupt() {
        let mut deep = pb(PbVariant::Int(1));
        for _ in 0..8 {
            deep = pb(PbVariant::List(PbList { items: vec![deep] }));
        }
        let body = deep.encode_to_vec();
        assert!(decode_codec1(&body, 32).is_ok());
        assert_eq!(decode_codec1(&body, 4), Err(UnavailableReason::Truncated));
        assert_eq!(
            decode_codec1(&[0xFF, 0xFF, 0xFF], 4),
            Err(UnavailableReason::Corrupt)
        );
    }
}
