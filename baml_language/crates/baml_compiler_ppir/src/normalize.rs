//! Stream annotation normalization.
//!
//! Computes per-field D (during-streaming type), S (starts-as value),
//! and typeof(S) from raw stream annotations and type classification.
//! This is the PPIR-level implementation of Phase 1 normalization.

use baml_base::Name;
use rustc_hash::FxHashMap;
use smol_str::SmolStr;

use crate::ty::{PpirTy, PpirTypeAttrs};

//
// ──────────────────────────────────────────────── STARTS-AS TYPES ─────
//

/// The before-streaming value S, represented as a semantic value.
///
/// This is richer than `PpirTy` — it captures the user's intent
/// (e.g., "empty list" vs. a list type, "null" vs. null type).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartsAs {
    /// Field absent until streaming begins (S = never).
    Never,
    /// Field starts as null.
    Null,
    /// Field starts as a literal value.
    Literal(StartsAsLiteral),
    /// Field starts as an empty list [].
    EmptyList,
    /// Field starts as an empty map {}.
    EmptyMap,
    /// An enum value reference, e.g. `Status.Loading`.
    /// PPIR stores the text; HIR resolves and validates.
    EnumValue {
        enum_name: String,
        variant_name: String,
    },
    /// Unrecognized value — PPIR cannot infer its type.
    /// `typeof_starts_as` defaults to `never`; HIR must validate.
    Unknown(String),
}

/// A literal value for `@stream.starts_as(...)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartsAsLiteral {
    String(String),
    Int(i64),
    /// Float stored as string to avoid f64 Eq/Hash issues.
    Float(String),
    Bool(bool),
}

/// Compute the default starts-as value from D's type category.
///
/// Per the stream-types spec:
/// - Literal types → never (absent until complete)
/// - Never → never
/// - List → empty list []
/// - Map → empty map {}
/// - Everything else → null
pub fn default_starts_as_semantic(d: &PpirTy) -> StartsAs {
    match d {
        // Literals: absent until complete
        PpirTy::StringLiteral { .. } | PpirTy::IntLiteral { .. } | PpirTy::BoolLiteral { .. } => {
            StartsAs::Never
        }

        // Never: absent
        PpirTy::Never { .. } => StartsAs::Never,

        // Containers: start empty
        PpirTy::List { .. } => StartsAs::EmptyList,
        PpirTy::Map { .. } => StartsAs::EmptyMap,

        // Everything else (primitives, classes, enums, type aliases, unions, optionals): null
        _ => StartsAs::Null,
    }
}

/// Parse a `@stream.starts_as(...)` value expression string into a `StartsAs`.
///
/// Handles: "never", "null", "true", "false", numeric literals,
/// "[]", "{}", and quoted strings.
pub fn parse_starts_as_value(s: &str) -> StartsAs {
    match s {
        "never" => StartsAs::Never,
        "null" => StartsAs::Null,
        "true" => StartsAs::Literal(StartsAsLiteral::Bool(true)),
        "false" => StartsAs::Literal(StartsAsLiteral::Bool(false)),
        "[]" => StartsAs::EmptyList,
        "{}" => StartsAs::EmptyMap,
        _ => {
            // Try integer
            if let Ok(i) = s.parse::<i64>() {
                return StartsAs::Literal(StartsAsLiteral::Int(i));
            }
            // Try float (exclude strings with alphabetic chars to avoid Foo.Bar confusion)
            if s.contains('.') && !s.contains(|c: char| c.is_alphabetic()) {
                if s.parse::<f64>().is_ok() {
                    return StartsAs::Literal(StartsAsLiteral::Float(s.to_string()));
                }
            }
            // Try enum value: Foo.Bar pattern (exactly one dot, both parts are identifiers)
            if let Some((left, right)) = s.split_once('.') {
                if is_identifier(left) && is_identifier(right) {
                    return StartsAs::EnumValue {
                        enum_name: left.to_string(),
                        variant_name: right.to_string(),
                    };
                }
            }
            // If it looks like an expression (has parens), it's unrecognized.
            // Everything else is a string literal.
            if s.contains('(') || s.contains(')') {
                return StartsAs::Unknown(s.to_string());
            }
            StartsAs::Literal(StartsAsLiteral::String(s.to_string()))
        }
    }
}

fn is_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// Infer the type of a starts-as value.
///
/// Returns None for EmptyList/EmptyMap — these need D's container element
/// type, which HIR lowering resolves.
/// Also returns None for Unknown — caller falls back to Never.
pub fn infer_typeof_s(
    starts_as: &StartsAs,
    enum_names: &FxHashMap<Name, Vec<Name>>,
) -> Option<PpirTy> {
    let d = PpirTypeAttrs::default();
    match starts_as {
        StartsAs::Never => Some(PpirTy::Never { attrs: d }),
        StartsAs::Null => Some(PpirTy::Null { attrs: d }),
        StartsAs::Literal(lit) => Some(match lit {
            StartsAsLiteral::String(s) => PpirTy::StringLiteral {
                value: s.clone(),
                attrs: d,
            },
            StartsAsLiteral::Int(i) => PpirTy::IntLiteral {
                value: *i,
                attrs: d,
            },
            StartsAsLiteral::Float(_) => PpirTy::Float { attrs: d },
            StartsAsLiteral::Bool(b) => PpirTy::BoolLiteral {
                value: *b,
                attrs: d,
            },
        }),
        StartsAs::EmptyList => None,
        StartsAs::EmptyMap => None,
        StartsAs::EnumValue { enum_name, .. } => {
            if enum_names.contains_key(enum_name.as_str()) {
                Some(PpirTy::Named {
                    name: SmolStr::new(enum_name),
                    attrs: d,
                })
            } else {
                None // Unknown type name → caller falls back to Never
            }
        }
        StartsAs::Unknown(_) => None, // Cannot infer → caller falls back to Never
    }
}

//
// ──────────────────────────────────────── DISPLAY ─────
//

impl std::fmt::Display for StartsAs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StartsAs::Never => write!(f, "never"),
            StartsAs::Null => write!(f, "null"),
            StartsAs::Literal(lit) => write!(f, "{lit}"),
            StartsAs::EmptyList => write!(f, "[]"),
            StartsAs::EmptyMap => write!(f, "{{}}"),
            StartsAs::EnumValue {
                enum_name,
                variant_name,
            } => write!(f, "{enum_name}.{variant_name}"),
            StartsAs::Unknown(s) => write!(f, "{s}"),
        }
    }
}

impl std::fmt::Display for StartsAsLiteral {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StartsAsLiteral::String(s) => write!(f, "\"{s}\""),
            StartsAsLiteral::Int(i) => write!(f, "{i}"),
            StartsAsLiteral::Float(v) => write!(f, "{v}"),
            StartsAsLiteral::Bool(b) => write!(f, "{b}"),
        }
    }
}
