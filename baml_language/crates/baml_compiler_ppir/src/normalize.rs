//! Stream annotation normalization.
//!
//! Computes per-field D (during-streaming type), S (starts-as value),
//! and typeof(S) from raw stream annotations and type classification.
//! This is the PPIR-level implementation of Phase 1 normalization.

use baml_base::Name;

use crate::ty::{ClassifiedField, PpirTypeRef};

//
// ──────────────────────────────────────────────── STARTS-AS TYPES ─────
//

/// The before-streaming value S, represented as a semantic value.
///
/// This is richer than `PpirTypeRef` — it captures the user's intent
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
    pub stream_type: PpirTypeRef,
    /// Whether D carries `@sap.in_progress(never)` (from `@stream.done`).
    pub in_progress_never: bool,
    /// The before-streaming value S (always explicit after normalization).
    pub starts_as: StartsAs,
    /// The inferred type of S. None for EmptyList/EmptyMap (deferred to
    /// Phase 3 which knows the container element type from D).
    pub typeof_s: Option<PpirTypeRef>,
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

/// Normalize a single field's streaming annotations.
///
/// Implements the normalization algorithm:
/// 1. Desugar legacy attributes (@stream.done, @stream.not_null)
/// 2. Compute D (stream_type): explicit or via stream_expand
/// 3. Compute S (starts_as): explicit or default from D's category
/// 4. Infer typeof(S)
pub fn normalize_field(pf: &ClassifiedField) -> NormalizedStreamField {
    // 1. Desugar legacy: @stream.done → effective D = field's original type
    let (effective_stream_type, _has_completed) = crate::expand::desugar_stream_attrs(pf);
    let in_progress_never = pf.stream_done;

    // 2. Compute D
    let d = effective_stream_type.unwrap_or_else(|| pf.ty.stream_expand());

    // 3. Compute S
    let starts_as = if pf.stream_not_null {
        StartsAs::Never
    } else if let Some(ref sa_str) = pf.stream_starts_as {
        parse_starts_as_value(sa_str)
    } else {
        default_starts_as_semantic(&d)
    };

    // 4. Infer typeof(S)
    let typeof_s = infer_typeof_s(&starts_as);

    NormalizedStreamField {
        name: pf.name.clone(),
        stream_type: d,
        in_progress_never,
        starts_as,
        typeof_s,
    }
}

/// Normalize all fields of a class.
pub fn normalize_class_fields(ppir_fields: &[ClassifiedField]) -> Vec<NormalizedStreamField> {
    ppir_fields.iter().map(normalize_field).collect()
}

/// Compute the default starts-as value from D's type category.
///
/// Per the stream-types spec:
/// - Literal types → never (absent until complete)
/// - Never → never
/// - List → empty list []
/// - Map → empty map {}
/// - Everything else → null
pub fn default_starts_as_semantic(d: &PpirTypeRef) -> StartsAs {
    match d {
        // Literals: absent until complete
        PpirTypeRef::StringLiteral(_)
        | PpirTypeRef::IntLiteral(_)
        | PpirTypeRef::BoolLiteral(_) => StartsAs::Never,

        // Never: absent
        PpirTypeRef::Never => StartsAs::Never,

        // Containers: start empty
        PpirTypeRef::List(_) => StartsAs::EmptyList,
        PpirTypeRef::Map { .. } => StartsAs::EmptyMap,

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
pub fn infer_typeof_s(starts_as: &StartsAs) -> Option<PpirTypeRef> {
    match starts_as {
        StartsAs::Never => Some(PpirTypeRef::Never),
        StartsAs::Null => Some(PpirTypeRef::Null),
        StartsAs::Literal(lit) => Some(match lit {
            StartsAsLiteral::String(s) => PpirTypeRef::StringLiteral(s.clone()),
            StartsAsLiteral::Int(i) => PpirTypeRef::IntLiteral(*i),
            StartsAsLiteral::Float(_) => PpirTypeRef::Float,
            StartsAsLiteral::Bool(b) => PpirTypeRef::BoolLiteral(*b),
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
