//! Canonical value semantics for the public SQL surface
//! (TASK/baml-query-scope.md §5.5; cct-1 semantics ported onto the
//! neutral value model).
//!
//! - Path navigation over an AVAILABLE value: an absent path or an
//!   incompatible step yields the SQL-NULL-like [`Nav::Missing`] — a
//!   non-match that keeps the result complete. A captured BAML null is
//!   ordinary null-like data. Elided/omitted subtrees are unavailability,
//!   not data.
//! - Equality is canonical semantic equality: map/class entries compare
//!   order-insensitively, every NaN equals the canonical NaN, and
//!   +0.0/-0.0 stay DISTINCT (bit comparison). Across numeric kinds
//!   (int/float/bigint) equality is numeric, agreeing with the ordering
//!   rule below; `Int(0)` therefore equals `Float(-0.0)` even though
//!   `Float(0.0)` does not.
//! - Ordering comparisons apply to comparable scalar kinds (int/float/
//!   bigint cross-compare numerically, strings bytewise); a cross-kind
//!   ordering comparison is a NULL-like non-match.

use serde::{Deserialize, Serialize};

use crate::value::model::{MediaContent, Presence, Value};

/// One frozen path step: `['key']` selects an object/class/map field,
/// `[N]` selects a zero-based list element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PathSeg {
    Key(String),
    Index(i64),
}

/// Navigation result over an available value.
#[derive(Debug, PartialEq)]
pub enum Nav<'a> {
    Found(&'a Value),
    /// Captured BAML null at this position (ordinary null-like data).
    Null,
    /// Absent path or incompatible step: SQL-NULL-like non-match; the
    /// result stays complete.
    Missing,
    /// The step landed on an omitted/elided subtree: the evidence for
    /// this path is not decodable — unavailability, not data.
    Elided,
}

/// Navigate `path` from `value`.
#[must_use]
pub fn navigate<'a>(value: &'a Value, path: &[PathSeg]) -> Nav<'a> {
    let mut current = value;
    for seg in path {
        current = match (current, seg) {
            (Value::Omitted { .. }, _) => return Nav::Elided,
            (Value::Null, _) => return Nav::Missing,
            (Value::Map(entries), PathSeg::Key(key)) => {
                match entries.iter().find(|(k, _)| k == key) {
                    Some((_, v)) => v,
                    None => return Nav::Missing,
                }
            }
            (Value::Class { fields, .. }, PathSeg::Key(key)) => {
                match fields.iter().find(|(name, ..)| name == key) {
                    Some((_, Presence::Null, _)) => return Nav::Null,
                    Some((_, Presence::Absent, _)) => return Nav::Missing,
                    Some((_, _, Some(v))) => v,
                    Some((_, _, None)) => return Nav::Missing,
                    None => return Nav::Missing,
                }
            }
            (Value::List(items), PathSeg::Index(idx)) => {
                let idx = usize::try_from(*idx).ok();
                match idx.and_then(|i| items.get(i)) {
                    Some(v) => v,
                    None => return Nav::Missing,
                }
            }
            // Every other combination is an incompatible step.
            _ => return Nav::Missing,
        };
    }
    match current {
        Value::Null => Nav::Null,
        Value::Omitted { .. } => Nav::Elided,
        v => Nav::Found(v),
    }
}

/// The comparison operators the lowering plants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

impl CmpOp {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            CmpOp::Eq => "=",
            CmpOp::NotEq => "!=",
            CmpOp::Lt => "<",
            CmpOp::LtEq => "<=",
            CmpOp::Gt => ">",
            CmpOp::GtEq => ">=",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<CmpOp> {
        Some(match s {
            "=" => CmpOp::Eq,
            "!=" => CmpOp::NotEq,
            "<" => CmpOp::Lt,
            "<=" => CmpOp::LtEq,
            ">" => CmpOp::Gt,
            ">=" => CmpOp::GtEq,
            _ => return None,
        })
    }
}

/// Canonical semantic equality. Never serialized-byte or handle equality:
/// structural, order-insensitive for maps/classes, canonical NaN folded,
/// ±0.0 distinct.
#[must_use]
pub fn semantic_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => float_eq(*x, *y),
        (Value::BigInt(x), Value::BigInt(y)) => x == y,
        (Value::Int(x), Value::BigInt(y)) | (Value::BigInt(y), Value::Int(x)) => {
            // Canonical bigints are minimal decimal, so a small bigint and
            // an int with the same value render identically.
            y == &x.to_string()
        }
        // Float against an integral kind: defer to `scalar_ord` rather than
        // enumerate the pairs, so equality and ordering cannot drift apart.
        // NaN falls out for free -- `scalar_ord` yields `None` for it.
        (Value::Int(_) | Value::BigInt(_), Value::Float(_))
        | (Value::Float(_), Value::Int(_) | Value::BigInt(_)) => {
            scalar_ord(a, b) == Some(std::cmp::Ordering::Equal)
        }
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Bytes(x), Value::Bytes(y)) => x == y,
        (Value::List(x), Value::List(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(l, r)| semantic_eq(l, r))
        }
        (Value::Map(x), Value::Map(y)) => {
            // Order-insensitive: compare through sorted views.
            let mut xs: Vec<&(String, Value)> = x.iter().collect();
            let mut ys: Vec<&(String, Value)> = y.iter().collect();
            xs.sort_by(|l, r| l.0.as_bytes().cmp(r.0.as_bytes()));
            ys.sort_by(|l, r| l.0.as_bytes().cmp(r.0.as_bytes()));
            xs.len() == ys.len()
                && xs
                    .iter()
                    .zip(ys)
                    .all(|(l, r)| l.0 == r.0 && semantic_eq(&l.1, &r.1))
        }
        (
            Value::Class {
                name: na,
                fields: fa,
            },
            Value::Class {
                name: nb,
                fields: fb,
            },
        ) => {
            if na != nb || fa.len() != fb.len() {
                return false;
            }
            let mut xs: Vec<_> = fa.iter().collect();
            let mut ys: Vec<_> = fb.iter().collect();
            xs.sort_by(|l, r| l.0.as_bytes().cmp(r.0.as_bytes()));
            ys.sort_by(|l, r| l.0.as_bytes().cmp(r.0.as_bytes()));
            xs.iter().zip(ys).all(|(l, r)| {
                l.0 == r.0
                    && l.1 == r.1
                    && match (&l.2, &r.2) {
                        (Some(x), Some(y)) => semantic_eq(x, y),
                        (None, None) => true,
                        _ => false,
                    }
            })
        }
        (
            Value::Enum {
                name: na,
                variant: va,
            },
            Value::Enum {
                name: nb,
                variant: vb,
            },
        ) => na == nb && va == vb,
        (
            Value::Media {
                kind: ka,
                mime: ma,
                content: ca,
            },
            Value::Media {
                kind: kb,
                mime: mb,
                content: cb,
            },
        ) => ka == kb && ma == mb && ca == cb,
        // Omitted subtrees carry no comparable data; the caller treats
        // them as unavailability before reaching equality.
        _ => false,
    }
}

/// Canonical float equality: every NaN equals the canonical NaN; ±0.0
/// stay distinct (bitwise), matching the codec.
fn float_eq(x: f64, y: f64) -> bool {
    if x.is_nan() && y.is_nan() {
        return true;
    }
    x.to_bits() == y.to_bits()
}

/// Scalar ordering for comparable kinds. `None` = incomparable (NULL-like
/// non-match). NaN orders against nothing (equality handled separately).
fn scalar_ord(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    let num = |v: &Value| -> Option<f64> {
        match v {
            Value::Int(i) =>
            {
                #[expect(clippy::cast_precision_loss, reason = "numeric cross-compare")]
                Some(*i as f64)
            }
            Value::Float(f) => Some(*f),
            Value::BigInt(s) => s.parse::<f64>().ok(),
            _ => None,
        }
    };
    match (a, b) {
        // Exact integer fast path avoids f64 rounding at the extremes.
        (Value::Int(x), Value::Int(y)) => Some(x.cmp(y)),
        (Value::String(x), Value::String(y)) => Some(x.as_bytes().cmp(y.as_bytes())),
        (Value::Bool(x), Value::Bool(y)) => Some(x.cmp(y)),
        _ => {
            let (x, y) = (num(a)?, num(b)?);
            if x.is_nan() || y.is_nan() {
                return None;
            }
            x.partial_cmp(&y)
        }
    }
}

/// Evaluate one comparison with the frozen semantics. `None` = NULL-like
/// non-match (incompatible kinds or non-ordered operands).
#[must_use]
pub fn compare(op: CmpOp, a: &Value, b: &Value) -> Option<bool> {
    match op {
        CmpOp::Eq => Some(semantic_eq(a, b)),
        CmpOp::NotEq => Some(!semantic_eq(a, b)),
        CmpOp::Lt | CmpOp::LtEq | CmpOp::Gt | CmpOp::GtEq => {
            let ord = scalar_ord(a, b)?;
            Some(match op {
                CmpOp::Lt => ord.is_lt(),
                CmpOp::LtEq => ord.is_le(),
                CmpOp::Gt => ord.is_gt(),
                CmpOp::GtEq => ord.is_ge(),
                CmpOp::Eq | CmpOp::NotEq => unreachable!(),
            })
        }
    }
}

/// The canonical JSON projection of a value (the render for structured
/// results and the reference shape for `baml_value_json`).
#[must_use]
pub fn to_json(value: &Value) -> serde_json::Value {
    use serde_json::Value as Json;
    match value {
        Value::Null => Json::Null,
        Value::Bool(b) => Json::Bool(*b),
        Value::Int(i) => Json::Number((*i).into()),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map_or_else(|| Json::String(f.to_string()), Json::Number),
        Value::BigInt(s) => s
            .parse::<i64>()
            .map(|i| Json::Number(i.into()))
            .unwrap_or_else(|_| Json::String(s.clone())),
        Value::String(s) => Json::String(s.clone()),
        Value::Bytes(b) => serde_json::json!({ "$bytes": hex(b) }),
        Value::List(items) => Json::Array(items.iter().map(to_json).collect()),
        Value::Map(entries) => Json::Object(
            entries
                .iter()
                .map(|(k, v)| (k.clone(), to_json(v)))
                .collect(),
        ),
        Value::Class { fields, .. } => Json::Object(
            fields
                .iter()
                .filter_map(|(name, presence, value)| match presence {
                    Presence::Absent => None,
                    Presence::Null => Some((name.clone(), Json::Null)),
                    Presence::Present => {
                        Some((name.clone(), value.as_ref().map_or(Json::Null, to_json)))
                    }
                })
                .collect(),
        ),
        Value::Enum { variant, .. } => Json::String(variant.clone()),
        Value::Media {
            kind,
            mime,
            content,
        } => match content {
            MediaContent::Url(url) => {
                serde_json::json!({ "$media": kind, "mime": mime, "url": url })
            }
            MediaContent::Bytes(bytes) => {
                serde_json::json!({ "$media": kind, "mime": mime, "bytes_len": bytes.len() })
            }
        },
        Value::Omitted { reason } => serde_json::json!({ "$omitted": reason }),
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Render a value for the SQL surface: scalar leaves as bare text (so
/// `CAST`/string functions behave naturally), structured values as
/// canonical JSON.
#[must_use]
pub fn render(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::BigInt(s) => s.clone(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Enum { variant, .. } => variant.clone(),
        other => to_json(other).to_string(),
    }
}

/// Interpret a JSON parameter (`baml_value_json`) as a value: object →
/// map, array → list, string/number/bool/null direct. Classes/enums/media
/// cannot be expressed this way — whole-value equality against
/// class-typed captures uses CID references.
#[must_use]
pub fn json_to_value(json: &serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::BigInt(n.to_string())
            }
        }
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Array(items) => Value::List(items.iter().map(json_to_value).collect()),
        serde_json::Value::Object(entries) => Value::Map(
            entries
                .iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(entries: &[(&str, Value)]) -> Value {
        Value::Map(
            entries
                .iter()
                .map(|(k, v)| ((*k).to_string(), v.clone()))
                .collect(),
        )
    }

    #[test]
    fn navigation_distinguishes_found_null_missing_elided() {
        let value = map(&[
            ("age", Value::Int(36)),
            ("nick", Value::Null),
            (
                "big",
                Value::Omitted {
                    reason: "elided".into(),
                },
            ),
            ("tags", Value::List(vec![Value::String("a".into())])),
        ]);
        let key = |k: &str| vec![PathSeg::Key(k.to_string())];
        assert_eq!(navigate(&value, &key("age")), Nav::Found(&Value::Int(36)));
        assert_eq!(navigate(&value, &key("nick")), Nav::Null);
        assert_eq!(navigate(&value, &key("ghost")), Nav::Missing);
        assert_eq!(navigate(&value, &key("big")), Nav::Elided);
        // Zero-based list index; out of bounds and negative are Missing.
        let tags0 = vec![PathSeg::Key("tags".into()), PathSeg::Index(0)];
        assert!(matches!(navigate(&value, &tags0), Nav::Found(_)));
        let tags9 = vec![PathSeg::Key("tags".into()), PathSeg::Index(9)];
        assert_eq!(navigate(&value, &tags9), Nav::Missing);
        let neg = vec![PathSeg::Key("tags".into()), PathSeg::Index(-1)];
        assert_eq!(navigate(&value, &neg), Nav::Missing);
        // Incompatible step: keying into a scalar.
        let bad = vec![PathSeg::Key("age".into()), PathSeg::Key("x".into())];
        assert_eq!(navigate(&value, &bad), Nav::Missing);
    }

    #[test]
    fn class_fields_distinguish_presence() {
        let value = Value::Class {
            name: "Customer".into(),
            fields: vec![
                ("age".into(), Presence::Present, Some(Value::Int(36))),
                ("nick".into(), Presence::Null, None),
                ("ghost".into(), Presence::Absent, None),
            ],
        };
        let key = |k: &str| vec![PathSeg::Key(k.to_string())];
        assert_eq!(navigate(&value, &key("age")), Nav::Found(&Value::Int(36)));
        assert_eq!(navigate(&value, &key("nick")), Nav::Null);
        assert_eq!(navigate(&value, &key("ghost")), Nav::Missing);
        assert_eq!(navigate(&value, &key("unknown")), Nav::Missing);
    }

    #[test]
    fn semantic_equality_is_order_insensitive_and_canonical() {
        let a = map(&[("x", Value::Int(1)), ("y", Value::Int(2))]);
        let b = map(&[("y", Value::Int(2)), ("x", Value::Int(1))]);
        assert!(semantic_eq(&a, &b), "map order never affects equality");

        // Every NaN equals the canonical NaN.
        assert!(semantic_eq(
            &Value::Float(f64::NAN),
            &Value::Float(f64::from_bits(0x7ff8_dead_beef_0000)),
        ));
        // +0.0 and -0.0 stay distinct, exactly like the codec.
        assert!(!semantic_eq(&Value::Float(0.0), &Value::Float(-0.0)));
        // Never byte/handle equality: equal values, differently built.
        assert!(semantic_eq(&Value::BigInt("7".into()), &Value::Int(7)));
    }

    #[test]
    fn comparisons_cross_numeric_and_reject_cross_kind_order() {
        let i = Value::Int(30);
        let f = Value::Float(29.5);
        assert_eq!(compare(CmpOp::Gt, &i, &f), Some(true));
        assert_eq!(compare(CmpOp::Lt, &f, &i), Some(true));
        // Strings order bytewise.
        assert_eq!(
            compare(
                CmpOp::Lt,
                &Value::String("a".into()),
                &Value::String("b".into()),
            ),
            Some(true)
        );
        // Cross-kind ordering: NULL-like non-match, not an error.
        assert_eq!(compare(CmpOp::Gt, &Value::String("30".into()), &i), None);
        // NaN never orders…
        assert_eq!(compare(CmpOp::Gt, &Value::Float(f64::NAN), &f), None);
        // …but equals the canonical NaN.
        assert_eq!(
            compare(CmpOp::Eq, &Value::Float(f64::NAN), &Value::Float(f64::NAN)),
            Some(true)
        );
    }

    #[test]
    fn equality_and_ordering_agree_across_numeric_kinds() {
        // Every mixed numeric pair that orders equal must also compare equal:
        // a SQL integer literal lowers to `Int`, a captured float decodes to
        // `Float`, so `= 30` and `>= 30 AND <= 30` have to select the same rows.
        for (a, b) in [
            (Value::Int(30), Value::Float(30.0)),
            (Value::BigInt("30".into()), Value::Float(30.0)),
        ] {
            assert_eq!(compare(CmpOp::Eq, &a, &b), Some(true), "{a:?} == {b:?}");
            assert_eq!(compare(CmpOp::Eq, &b, &a), Some(true), "{b:?} == {a:?}");
            assert_eq!(compare(CmpOp::NotEq, &a, &b), Some(false));
            assert_eq!(compare(CmpOp::GtEq, &a, &b), Some(true));
            assert_eq!(compare(CmpOp::LtEq, &a, &b), Some(true));
        }
        // A fractional float still differs from the integer.
        assert_eq!(
            compare(CmpOp::Eq, &Value::Int(30), &Value::Float(30.5)),
            Some(false)
        );
        // NaN equals no number, in either direction.
        assert_eq!(
            compare(CmpOp::Eq, &Value::Int(30), &Value::Float(f64::NAN)),
            Some(false)
        );
        // Cross-kind non-numerics stay unequal rather than becoming numeric.
        assert_eq!(
            compare(CmpOp::Eq, &Value::Int(30), &Value::String("30".into())),
            Some(false)
        );
    }

    #[test]
    fn rendering_is_bare_for_scalars_and_json_for_structures() {
        assert_eq!(render(&Value::String("ada".into())), "ada");
        assert_eq!(render(&Value::Int(42)), "42");
        assert_eq!(render(&Value::Bool(true)), "true");
        let structured = map(&[("x", Value::Int(1))]);
        assert_eq!(render(&structured), r#"{"x":1}"#);
        // Classes render their present fields; enums their variant.
        let class = Value::Class {
            name: "C".into(),
            fields: vec![
                ("a".into(), Presence::Present, Some(Value::Int(1))),
                ("gone".into(), Presence::Absent, None),
            ],
        };
        assert_eq!(render(&class), r#"{"a":1}"#);
        assert_eq!(
            render(&Value::Enum {
                name: "Color".into(),
                variant: "Red".into()
            }),
            "Red"
        );
    }
}
