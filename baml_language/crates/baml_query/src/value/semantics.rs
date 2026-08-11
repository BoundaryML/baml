//! Canonical value semantics for the public SQL surface (IN-Q1-2/3/6).
//!
//! - Path navigation over an AVAILABLE value: an absent path or an
//!   incompatible step yields the SQL-NULL-like [`Nav::Missing`] — a
//!   non-match that keeps the result complete. A captured BAML null is
//!   ordinary null-like data. Elided/omitted subtrees are unavailability,
//!   not data.
//! - Equality is canonical semantic equality: map/class entries compare
//!   order-insensitively (the codec sorts them), every NaN equals the
//!   canonical NaN, and +0.0/-0.0 stay DISTINCT (bit comparison), exactly
//!   as the canonical codec keeps them.
//! - Ordering comparisons apply to comparable scalar kinds (int/float/
//!   bigint cross-compare numerically, strings bytewise); a cross-kind
//!   ordering comparison is a NULL-like non-match.

use bex_events::store::canon::{CanonValue, Presence};
use serde::{Deserialize, Serialize};

/// One frozen path step: `['key']` selects an object/class/map field,
/// `[N]` selects a zero-based list element (IN-Q1-2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PathSeg {
    Key(String),
    Index(i64),
}

/// Navigation result over an available value.
#[derive(Debug, PartialEq)]
pub enum Nav<'a> {
    Found(&'a CanonValue),
    /// Captured BAML null at this position (ordinary null-like data).
    Null,
    /// Absent path or incompatible step (IN-Q1-3): SQL-NULL-like
    /// non-match; the result stays complete.
    Missing,
    /// The step landed on an omitted/elided subtree: the evidence for
    /// this path is not decodable — unavailability, not data.
    Elided,
}

/// Navigate `path` from `value`.
#[must_use]
pub fn navigate<'a>(value: &'a CanonValue, path: &[PathSeg]) -> Nav<'a> {
    let mut current = value;
    for seg in path {
        current = match (current, seg) {
            (CanonValue::Omitted { .. }, _) => return Nav::Elided,
            (CanonValue::Null, _) => return Nav::Missing,
            (CanonValue::Map(entries), PathSeg::Key(key)) => {
                // The codec sorts by key bytes with last-dup-wins; a
                // decoded map is already deduplicated.
                match entries.iter().find(|(k, _)| k == key) {
                    Some((_, v)) => v,
                    None => return Nav::Missing,
                }
            }
            (CanonValue::Class { fields, .. }, PathSeg::Key(key)) => {
                match fields.iter().find(|(name, ..)| name == key) {
                    Some((_, Presence::Null, _)) => return Nav::Null,
                    Some((_, Presence::Absent, _)) => return Nav::Missing,
                    Some((_, _, Some(v))) => v,
                    Some((_, _, None)) => return Nav::Missing,
                    None => return Nav::Missing,
                }
            }
            (CanonValue::List(items), PathSeg::Index(idx)) => {
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
        CanonValue::Null => Nav::Null,
        CanonValue::Omitted { .. } => Nav::Elided,
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

/// Canonical semantic equality (D7). Never serialized-byte or handle
/// equality: structural, order-insensitive for maps/classes, canonical
/// NaN folded, ±0.0 distinct.
#[must_use]
pub fn semantic_eq(a: &CanonValue, b: &CanonValue) -> bool {
    match (a, b) {
        (CanonValue::Null, CanonValue::Null) => true,
        (CanonValue::Bool(x), CanonValue::Bool(y)) => x == y,
        (CanonValue::Int(x), CanonValue::Int(y)) => x == y,
        (CanonValue::Float(x), CanonValue::Float(y)) => float_eq(*x, *y),
        (CanonValue::Bigint(x), CanonValue::Bigint(y)) => x == y,
        (CanonValue::Int(x), CanonValue::Bigint(y))
        | (CanonValue::Bigint(y), CanonValue::Int(x)) => {
            // Canonical bigints are minimal decimal, so a small bigint and
            // an int with the same value render identically.
            y == &x.to_string()
        }
        (CanonValue::String(x), CanonValue::String(y)) => x == y,
        (CanonValue::Bytes(x), CanonValue::Bytes(y)) => x == y,
        (CanonValue::List(x), CanonValue::List(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(l, r)| semantic_eq(l, r))
        }
        (CanonValue::Map(x), CanonValue::Map(y)) => {
            // Order-insensitive: compare through sorted views (decoded
            // canonical maps are already sorted; parameter-built ones may
            // not be).
            let mut xs: Vec<&(String, CanonValue)> = x.iter().collect();
            let mut ys: Vec<&(String, CanonValue)> = y.iter().collect();
            xs.sort_by(|l, r| l.0.as_bytes().cmp(r.0.as_bytes()));
            ys.sort_by(|l, r| l.0.as_bytes().cmp(r.0.as_bytes()));
            xs.len() == ys.len()
                && xs
                    .iter()
                    .zip(ys)
                    .all(|(l, r)| l.0 == r.0 && semantic_eq(&l.1, &r.1))
        }
        (
            CanonValue::Class {
                definition_key: ka,
                fields: fa,
            },
            CanonValue::Class {
                definition_key: kb,
                fields: fb,
            },
        ) => {
            if ka != kb || fa.len() != fb.len() {
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
            CanonValue::Enum {
                definition_key: ka,
                variant: va,
            },
            CanonValue::Enum {
                definition_key: kb,
                variant: vb,
            },
        ) => ka == kb && va == vb,
        (
            CanonValue::Media {
                kind: ka,
                mime: ma,
                content_kind: cka,
                content: ca,
            },
            CanonValue::Media {
                kind: kb,
                mime: mb,
                content_kind: ckb,
                content: cb,
            },
        ) => ka == kb && ma == mb && cka == ckb && ca == cb,
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
fn scalar_ord(a: &CanonValue, b: &CanonValue) -> Option<std::cmp::Ordering> {
    use std::cmp::Ordering;
    let num = |v: &CanonValue| -> Option<f64> {
        match v {
            CanonValue::Int(i) => Some(*i as f64),
            CanonValue::Float(f) => Some(*f),
            CanonValue::Bigint(s) => s.parse::<f64>().ok(),
            _ => None,
        }
    };
    match (a, b) {
        // Exact integer fast path avoids f64 rounding at the extremes.
        (CanonValue::Int(x), CanonValue::Int(y)) => Some(x.cmp(y)),
        (CanonValue::String(x), CanonValue::String(y)) => Some(x.as_bytes().cmp(y.as_bytes())),
        (CanonValue::Bool(x), CanonValue::Bool(y)) => Some(x.cmp(y)),
        _ => {
            let (x, y) = (num(a)?, num(b)?);
            if x.is_nan() || y.is_nan() {
                return None;
            }
            x.partial_cmp(&y)
                .or(if x == y { Some(Ordering::Equal) } else { None })
        }
    }
}

/// Evaluate one comparison with the frozen semantics. `None` = NULL-like
/// non-match (incompatible kinds or non-ordered operands).
#[must_use]
pub fn compare(op: CmpOp, a: &CanonValue, b: &CanonValue) -> Option<bool> {
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

/// Render a value for the SQL surface (IN-Q1 lowering): scalar leaves as
/// bare text (so `CAST`/string functions behave naturally), structured
/// values as canonical JSON.
#[must_use]
pub fn render(value: &CanonValue) -> String {
    match value {
        CanonValue::String(s) => s.clone(),
        CanonValue::Int(i) => i.to_string(),
        CanonValue::Bigint(s) => s.clone(),
        CanonValue::Float(f) => {
            if f.is_finite() {
                f.to_string()
            } else {
                format!("{f}")
            }
        }
        CanonValue::Bool(b) => b.to_string(),
        CanonValue::Enum { variant, .. } => variant.clone(),
        other => bex_events::store::canon::to_json(other).to_string(),
    }
}

/// Interpret a JSON parameter (`@json:` binding / `baml_value_json`) as a
/// canonical value: object → map, array → list, string/number/bool/null
/// direct. Classes/enums/media cannot be expressed this way — whole-value
/// equality against class-typed captures uses `@cid:` references.
#[must_use]
pub fn json_to_canon(json: &serde_json::Value) -> CanonValue {
    match json {
        serde_json::Value::Null => CanonValue::Null,
        serde_json::Value::Bool(b) => CanonValue::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                CanonValue::Int(i)
            } else if let Some(f) = n.as_f64() {
                CanonValue::Float(f)
            } else {
                CanonValue::Bigint(n.to_string())
            }
        }
        serde_json::Value::String(s) => CanonValue::String(s.clone()),
        serde_json::Value::Array(items) => {
            CanonValue::List(items.iter().map(json_to_canon).collect())
        }
        serde_json::Value::Object(entries) => CanonValue::Map(
            entries
                .iter()
                .map(|(k, v)| (k.clone(), json_to_canon(v)))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(entries: &[(&str, CanonValue)]) -> CanonValue {
        CanonValue::Map(
            entries
                .iter()
                .map(|(k, v)| ((*k).to_string(), v.clone()))
                .collect(),
        )
    }

    #[test]
    fn navigation_distinguishes_found_null_missing_elided() {
        let value = map(&[
            ("age", CanonValue::Int(36)),
            ("nick", CanonValue::Null),
            (
                "big",
                CanonValue::Omitted {
                    reason: 1,
                    message: "elided".into(),
                },
            ),
            (
                "tags",
                CanonValue::List(vec![CanonValue::String("a".into())]),
            ),
        ]);
        let key = |k: &str| vec![PathSeg::Key(k.to_string())];
        assert_eq!(
            navigate(&value, &key("age")),
            Nav::Found(&CanonValue::Int(36))
        );
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
    fn semantic_equality_is_order_insensitive_and_canonical() {
        let a = map(&[("x", CanonValue::Int(1)), ("y", CanonValue::Int(2))]);
        let b = map(&[("y", CanonValue::Int(2)), ("x", CanonValue::Int(1))]);
        assert!(semantic_eq(&a, &b), "map order never affects equality");

        // Every NaN equals the canonical NaN.
        assert!(semantic_eq(
            &CanonValue::Float(f64::NAN),
            &CanonValue::Float(f64::from_bits(0x7ff8_dead_beef_0000)),
        ));
        // +0.0 and -0.0 stay distinct, exactly like the codec.
        assert!(!semantic_eq(
            &CanonValue::Float(0.0),
            &CanonValue::Float(-0.0),
        ));
        // Never byte/handle equality: equal values, differently built.
        assert!(semantic_eq(
            &CanonValue::Bigint("7".into()),
            &CanonValue::Int(7),
        ));
    }

    #[test]
    fn comparisons_cross_numeric_and_reject_cross_kind_order() {
        let i = CanonValue::Int(30);
        let f = CanonValue::Float(29.5);
        assert_eq!(compare(CmpOp::Gt, &i, &f), Some(true));
        assert_eq!(compare(CmpOp::Lt, &f, &i), Some(true));
        // Strings order bytewise.
        assert_eq!(
            compare(
                CmpOp::Lt,
                &CanonValue::String("a".into()),
                &CanonValue::String("b".into()),
            ),
            Some(true)
        );
        // Cross-kind ordering: NULL-like non-match, not an error.
        assert_eq!(
            compare(CmpOp::Gt, &CanonValue::String("30".into()), &i),
            None
        );
        // NaN never orders…
        assert_eq!(compare(CmpOp::Gt, &CanonValue::Float(f64::NAN), &f), None);
        // …but equals the canonical NaN.
        assert_eq!(
            compare(
                CmpOp::Eq,
                &CanonValue::Float(f64::NAN),
                &CanonValue::Float(f64::NAN)
            ),
            Some(true)
        );
    }

    #[test]
    fn rendering_is_bare_for_scalars_and_json_for_structures() {
        assert_eq!(render(&CanonValue::String("ada".into())), "ada");
        assert_eq!(render(&CanonValue::Int(42)), "42");
        assert_eq!(render(&CanonValue::Bool(true)), "true");
        let structured = map(&[("x", CanonValue::Int(1))]);
        assert_eq!(render(&structured), r#"{"x":1}"#);
    }
}
