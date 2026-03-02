//! Stream annotation normalization.
//!
//! Computes per-field D (during-streaming type), S (starts-as value),
//! and typeof(S) from raw stream annotations and type classification.
//! This is the PPIR-level implementation of Phase 1 normalization.

use baml_base::Name;

use crate::expand::{PpirExpandedField, PpirSapMissing, extract_starts_as_text};
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

//
// ──────────────────────────────────────────── NORMALIZED TYPES ─────
//

/// Fully normalized streaming annotations for a single field.
///
/// Every field gets one after normalization. Phase 3+ reads these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedStreamField {
    pub name: Name,
    /// The during-streaming type D (always explicit after normalization).
    pub stream_type: PpirTy,
    /// Whether D carries `@sap.in_progress(never)` (from `@stream.done`).
    pub in_progress_never: bool,
    /// The before-streaming value S (always explicit after normalization).
    pub starts_as: StartsAs,
    /// The inferred type of S. None for EmptyList/EmptyMap (deferred to
    /// Phase 3 which knows the container element type from D).
    pub typeof_s: Option<PpirTy>,
}

/// Normalized streaming annotations for a class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedStreamClass {
    /// Original class name (e.g., "Foo", not "stream_Foo").
    pub name: Name,
    pub fields: Vec<NormalizedStreamField>,
}

//
// ──────────────────────────────────────── NORMALIZATION LOGIC ─────
//

/// Normalize expanded fields into NormalizedStreamFields.
///
/// Takes PpirExpandedFields (output of Phase 1 expansion) and produces
/// NormalizedStreamFields with semantic StartsAs values.
pub fn normalize_expanded_fields(expanded_fields: &[PpirExpandedField]) -> Vec<NormalizedStreamField> {
    expanded_fields.iter().map(normalize_expanded_field).collect()
}

/// Normalize a single expanded field.
fn normalize_expanded_field(ef: &PpirExpandedField) -> NormalizedStreamField {
    // Compute StartsAs from PpirSapMissing
    let starts_as = match &ef.sap_missing {
        PpirSapMissing::Never => StartsAs::Never,
        PpirSapMissing::Default(ty) => default_starts_as_semantic(ty),
        PpirSapMissing::Explicit(node) => {
            // Extract text from the SyntaxNode and parse it
            let text = extract_starts_as_text(node);
            parse_starts_as_value(&text)
        }
    };

    let typeof_s = infer_typeof_s(&starts_as);

    NormalizedStreamField {
        name: ef.name.clone(),
        stream_type: ef.stream_type.clone(),
        in_progress_never: ef.sap_in_progress_never,
        starts_as,
        typeof_s,
    }
}

/// Normalize all fields of a class (legacy, operates on ClassifiedField).
pub fn normalize_class_fields(ppir_fields: &[PpirExpandedField]) -> Vec<NormalizedStreamField> {
    normalize_expanded_fields(ppir_fields)
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
        PpirTy::StringLiteral { .. }
        | PpirTy::IntLiteral { .. }
        | PpirTy::BoolLiteral { .. } => StartsAs::Never,

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
            // Try float
            if s.contains('.') {
                if s.parse::<f64>().is_ok() {
                    return StartsAs::Literal(StartsAsLiteral::Float(s.to_string()));
                }
            }
            // Treat as string literal (the CST string_arg() already strips quotes)
            StartsAs::Literal(StartsAsLiteral::String(s.to_string()))
        }
    }
}

/// Infer the type of a starts-as value.
///
/// Returns None for EmptyList/EmptyMap — these need D's container element
/// type, which Phase 3 resolves.
pub fn infer_typeof_s(starts_as: &StartsAs) -> Option<PpirTy> {
    let d = PpirTypeAttrs::default();
    match starts_as {
        StartsAs::Never => Some(PpirTy::Never { attrs: d }),
        StartsAs::Null => Some(PpirTy::Null { attrs: d }),
        StartsAs::Literal(lit) => Some(match lit {
            StartsAsLiteral::String(s) => PpirTy::StringLiteral { value: s.clone(), attrs: d },
            StartsAsLiteral::Int(i) => PpirTy::IntLiteral { value: *i, attrs: d },
            StartsAsLiteral::Float(_) => PpirTy::Float { attrs: d },
            StartsAsLiteral::Bool(b) => PpirTy::BoolLiteral { value: *b, attrs: d },
        }),
        StartsAs::EmptyList => None,
        StartsAs::EmptyMap => None,
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
