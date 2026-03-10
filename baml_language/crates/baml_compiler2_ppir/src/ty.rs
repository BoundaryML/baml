//! PPIR type representation for compiler2.
//!
//! `PpirTy` carries type-level stream annotations (`PpirTypeAttrs`) on every variant.
//! Constructed from compiler2 AST `TypeExpr` + `RawAttribute` (not from CST).

use baml_base::Name;
use baml_compiler2_ast::{RawAttribute, TypeExpr};
use smol_str::SmolStr;

//
// ──────────────────────────────────────────────── TYPE ATTRS ─────
//

/// Type-level attributes captured from AST field/type alias attributes.
/// Carried on every `PpirTy` variant via named `attrs` field.
///
/// Default is "no annotations" — most types have no stream attrs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct PpirTypeAttrs {
    /// `@stream.type(D)` — explicit streaming type override.
    /// When `Some`, `stream_expand` uses D instead of recursing into this type.
    pub stream_type: Option<Box<PpirTy>>,

    /// `@stream.done` — indicates `@sap.in_progress(never)`.
    /// When true and `stream_type` is `None`, desugars to `stream_type = Some(T)`
    /// where T is the type this attr is on.
    pub stream_done: bool,

    /// `@stream.with_state` — wrap final stream type in `StreamState<T>`.
    /// Consumed during TIR stream type generation.
    pub stream_with_state: bool,
}

impl PpirTypeAttrs {
    /// Returns true if no type-level stream annotations are set.
    pub fn is_empty(&self) -> bool {
        self.stream_type.is_none() && !self.stream_done && !self.stream_with_state
    }
}

//
// ──────────────────────────────────────────────── PPIR TY ─────
//

/// PPIR's type reference — carries type-level stream annotations on every variant.
///
/// Structurally parallel to `hir::TypeRef` / `tir::Ty` but defined independently
/// to avoid a circular dependency (PPIR does not depend on HIR).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PpirTy {
    /// Named type reference (user-defined class, enum, type alias, or `stream_*` name).
    /// `path` carries the full qualified path segments (e.g., `["ns", "Foo"]`).
    Named {
        path: Vec<Name>,
        attrs: PpirTypeAttrs,
    },

    /// Primitives.
    Int {
        attrs: PpirTypeAttrs,
    },
    Float {
        attrs: PpirTypeAttrs,
    },
    String {
        attrs: PpirTypeAttrs,
    },
    Bool {
        attrs: PpirTypeAttrs,
    },

    /// Null and never.
    Null {
        attrs: PpirTypeAttrs,
    },
    Never {
        attrs: PpirTypeAttrs,
    },

    /// Type constructors.
    Optional {
        inner: Box<PpirTy>,
        attrs: PpirTypeAttrs,
    },
    List {
        inner: Box<PpirTy>,
        attrs: PpirTypeAttrs,
    },
    Map {
        key: Box<PpirTy>,
        value: Box<PpirTy>,
        attrs: PpirTypeAttrs,
    },
    Union {
        variants: Vec<PpirTy>,
        attrs: PpirTypeAttrs,
    },

    /// Literal types.
    StringLiteral {
        value: std::string::String,
        attrs: PpirTypeAttrs,
    },
    IntLiteral {
        value: i64,
        attrs: PpirTypeAttrs,
    },
    BoolLiteral {
        value: bool,
        attrs: PpirTypeAttrs,
    },

    /// Media types.
    Media {
        kind: baml_base::MediaKind,
        attrs: PpirTypeAttrs,
    },

    /// `$rust_type` — opaque Rust-managed state field type.
    RustType {
        attrs: PpirTypeAttrs,
    },

    /// Error recovery / unknown.
    Unknown {
        attrs: PpirTypeAttrs,
    },
}

impl PpirTy {
    /// Get a reference to this type's stream attributes.
    pub fn attrs(&self) -> &PpirTypeAttrs {
        match self {
            Self::Named { attrs, .. }
            | Self::Int { attrs }
            | Self::Float { attrs }
            | Self::String { attrs }
            | Self::Bool { attrs }
            | Self::Null { attrs }
            | Self::Never { attrs }
            | Self::Optional { attrs, .. }
            | Self::List { attrs, .. }
            | Self::Map { attrs, .. }
            | Self::Union { attrs, .. }
            | Self::StringLiteral { attrs, .. }
            | Self::IntLiteral { attrs, .. }
            | Self::BoolLiteral { attrs, .. }
            | Self::Media { attrs, .. }
            | Self::RustType { attrs }
            | Self::Unknown { attrs } => attrs,
        }
    }

    /// Get a mutable reference to this type's stream attributes.
    pub fn attrs_mut(&mut self) -> &mut PpirTypeAttrs {
        match self {
            Self::Named { attrs, .. }
            | Self::Int { attrs }
            | Self::Float { attrs }
            | Self::String { attrs }
            | Self::Bool { attrs }
            | Self::Null { attrs }
            | Self::Never { attrs }
            | Self::Optional { attrs, .. }
            | Self::List { attrs, .. }
            | Self::Map { attrs, .. }
            | Self::Union { attrs, .. }
            | Self::StringLiteral { attrs, .. }
            | Self::IntLiteral { attrs, .. }
            | Self::BoolLiteral { attrs, .. }
            | Self::Media { attrs, .. }
            | Self::RustType { attrs }
            | Self::Unknown { attrs } => attrs,
        }
    }

    /// Clone this type with default (empty) attrs.
    /// Used by `stream_expand` to strip annotations after consuming them.
    #[must_use]
    pub fn clone_without_attrs(&self) -> Self {
        let d = PpirTypeAttrs::default();
        match self {
            Self::Named { path, .. } => Self::Named {
                path: path.clone(),
                attrs: d,
            },
            Self::Int { .. } => Self::Int { attrs: d },
            Self::Float { .. } => Self::Float { attrs: d },
            Self::String { .. } => Self::String { attrs: d },
            Self::Bool { .. } => Self::Bool { attrs: d },
            Self::Null { .. } => Self::Null { attrs: d },
            Self::Never { .. } => Self::Never { attrs: d },
            Self::Optional { inner, .. } => Self::Optional {
                inner: inner.clone(),
                attrs: d,
            },
            Self::List { inner, .. } => Self::List {
                inner: inner.clone(),
                attrs: d,
            },
            Self::Map { key, value, .. } => Self::Map {
                key: key.clone(),
                value: value.clone(),
                attrs: d,
            },
            Self::Union { variants, .. } => Self::Union {
                variants: variants.clone(),
                attrs: d,
            },
            Self::StringLiteral { value, .. } => Self::StringLiteral {
                value: value.clone(),
                attrs: d,
            },
            Self::IntLiteral { value, .. } => Self::IntLiteral {
                value: *value,
                attrs: d,
            },
            Self::BoolLiteral { value, .. } => Self::BoolLiteral {
                value: *value,
                attrs: d,
            },
            Self::Media { kind, .. } => Self::Media {
                kind: *kind,
                attrs: d,
            },
            Self::RustType { .. } => Self::RustType { attrs: d },
            Self::Unknown { .. } => Self::Unknown { attrs: d },
        }
    }

    //
    // ──────────────────────────── CONSTRUCTORS ─────
    //

    /// Create a simple named type reference (single-segment path).
    pub fn named(name: impl Into<Name>) -> Self {
        PpirTy::Named {
            path: vec![name.into()],
            attrs: PpirTypeAttrs::default(),
        }
    }

    /// Create a list type.
    pub fn list(inner: PpirTy) -> Self {
        PpirTy::List {
            inner: Box::new(inner),
            attrs: PpirTypeAttrs::default(),
        }
    }

    /// Create an optional type.
    pub fn optional(inner: PpirTy) -> Self {
        PpirTy::Optional {
            inner: Box::new(inner),
            attrs: PpirTypeAttrs::default(),
        }
    }

    /// Create a union type.
    pub fn union(types: Vec<PpirTy>) -> Self {
        PpirTy::Union {
            variants: types,
            attrs: PpirTypeAttrs::default(),
        }
    }

    /// Create a `PpirTy` from a type name string (primitive or user-defined).
    pub fn from_type_name(name: &str) -> Self {
        let d = PpirTypeAttrs::default();
        match name {
            "int" => PpirTy::Int { attrs: d },
            "float" => PpirTy::Float { attrs: d },
            "string" => PpirTy::String { attrs: d },
            "bool" => PpirTy::Bool { attrs: d },
            "null" => PpirTy::Null { attrs: d },
            "never" => PpirTy::Never { attrs: d },
            "image" => PpirTy::Media {
                kind: baml_base::MediaKind::Image,
                attrs: d,
            },
            "audio" => PpirTy::Media {
                kind: baml_base::MediaKind::Audio,
                attrs: d,
            },
            "video" => PpirTy::Media {
                kind: baml_base::MediaKind::Video,
                attrs: d,
            },
            "pdf" => PpirTy::Media {
                kind: baml_base::MediaKind::Pdf,
                attrs: d,
            },
            _ => {
                if name.contains('.') {
                    PpirTy::Named {
                        path: name.split('.').map(Name::new).collect(),
                        attrs: d,
                    }
                } else {
                    PpirTy::Named {
                        path: vec![SmolStr::new(name)],
                        attrs: d,
                    }
                }
            }
        }
    }

    //
    // ──────────────────────────── AST CONVERSION ─────
    //

    /// Construct a `PpirTy` from a compiler2 AST `TypeExpr` and field-level attributes.
    ///
    /// Type-level stream attributes (`@stream.type`, `@stream.done`, `@stream.with_state`)
    /// are extracted from the `RawAttribute` list and stored in `PpirTypeAttrs`.
    pub fn from_type_expr(type_expr: &TypeExpr, attrs: &[RawAttribute]) -> PpirTy {
        let ppir_attrs = Self::extract_type_attrs(attrs);
        Self::convert_type_expr(type_expr, ppir_attrs)
    }

    /// Extract `@stream.*` type-level attributes from `RawAttribute` list.
    ///
    /// Note: `@stream.type` is not parsed here — it is an internal implementation
    /// detail set programmatically, not exposed as user-facing syntax.
    fn extract_type_attrs(attrs: &[RawAttribute]) -> PpirTypeAttrs {
        let mut result = PpirTypeAttrs::default();
        for attr in attrs {
            match attr.name.as_str() {
                "stream.done" => result.stream_done = true,
                "stream.with_state" => result.stream_with_state = true,
                _ => {}
            }
        }
        result
    }

    /// Convert a `TypeExpr` to `PpirTy` with the given attributes.
    fn convert_type_expr(type_expr: &TypeExpr, attrs: PpirTypeAttrs) -> PpirTy {
        match type_expr {
            TypeExpr::Int => PpirTy::Int { attrs },
            TypeExpr::Float => PpirTy::Float { attrs },
            TypeExpr::String => PpirTy::String { attrs },
            TypeExpr::Bool => PpirTy::Bool { attrs },
            TypeExpr::Null => PpirTy::Null { attrs },
            TypeExpr::Never => PpirTy::Never { attrs },
            TypeExpr::Path(segments) => PpirTy::Named {
                path: segments.clone(),
                attrs,
            },
            TypeExpr::Optional(inner) => PpirTy::Optional {
                inner: Box::new(Self::convert_type_expr(inner, PpirTypeAttrs::default())),
                attrs,
            },
            TypeExpr::List(inner) => PpirTy::List {
                inner: Box::new(Self::convert_type_expr(inner, PpirTypeAttrs::default())),
                attrs,
            },
            TypeExpr::Map { key, value } => PpirTy::Map {
                key: Box::new(Self::convert_type_expr(key, PpirTypeAttrs::default())),
                value: Box::new(Self::convert_type_expr(value, PpirTypeAttrs::default())),
                attrs,
            },
            TypeExpr::Union(variants) => PpirTy::Union {
                variants: variants
                    .iter()
                    .map(|v| Self::convert_type_expr(v, PpirTypeAttrs::default()))
                    .collect(),
                attrs,
            },
            TypeExpr::Literal(lit) => match lit {
                baml_base::Literal::String(s) => PpirTy::StringLiteral {
                    value: s.clone(),
                    attrs,
                },
                baml_base::Literal::Int(i) => PpirTy::IntLiteral { value: *i, attrs },
                baml_base::Literal::Bool(b) => PpirTy::BoolLiteral { value: *b, attrs },
                baml_base::Literal::Float(_) => {
                    // Float literals don't have a dedicated PpirTy variant.
                    // Treat as Unknown for now (rare in practice).
                    PpirTy::Unknown { attrs }
                }
            },
            TypeExpr::Media(kind) => PpirTy::Media { kind: *kind, attrs },
            TypeExpr::BuiltinUnknown
            | TypeExpr::Type
            | TypeExpr::Function { .. }
            | TypeExpr::Error
            | TypeExpr::Unknown => PpirTy::Unknown { attrs },
            TypeExpr::Rust => PpirTy::RustType { attrs },
        }
    }

    //
    // ──────────────────────────── BACK-CONVERSION ─────
    //

    /// Convert a `PpirTy` back to a `TypeExpr` for synthesized AST items.
    ///
    /// Drops `PpirTypeAttrs` — those are emitted as separate `RawAttribute`s
    /// on the synthesized field/class.
    pub fn to_type_expr(&self) -> TypeExpr {
        match self {
            PpirTy::Named { path, .. } => TypeExpr::Path(path.clone()),
            PpirTy::Int { .. } => TypeExpr::Int,
            PpirTy::Float { .. } => TypeExpr::Float,
            PpirTy::String { .. } => TypeExpr::String,
            PpirTy::Bool { .. } => TypeExpr::Bool,
            PpirTy::Null { .. } => TypeExpr::Null,
            PpirTy::Never { .. } => TypeExpr::Never,
            PpirTy::Optional { inner, .. } => TypeExpr::Optional(Box::new(inner.to_type_expr())),
            PpirTy::List { inner, .. } => TypeExpr::List(Box::new(inner.to_type_expr())),
            PpirTy::Map { key, value, .. } => TypeExpr::Map {
                key: Box::new(key.to_type_expr()),
                value: Box::new(value.to_type_expr()),
            },
            PpirTy::Union { variants, .. } => {
                TypeExpr::Union(variants.iter().map(PpirTy::to_type_expr).collect())
            }
            PpirTy::StringLiteral { value, .. } => {
                TypeExpr::Literal(baml_base::Literal::String(value.clone()))
            }
            PpirTy::IntLiteral { value, .. } => TypeExpr::Literal(baml_base::Literal::Int(*value)),
            PpirTy::BoolLiteral { value, .. } => {
                TypeExpr::Literal(baml_base::Literal::Bool(*value))
            }
            PpirTy::Media { kind, .. } => TypeExpr::Media(*kind),
            PpirTy::RustType { .. } => TypeExpr::Rust,
            PpirTy::Unknown { .. } => TypeExpr::Unknown,
        }
    }
}

//
// ──────────────────────────────────────────────────────── FIELD ─────
//

/// A PPIR field with parsed type and field-level stream annotations.
///
/// Intermediate representation — the expansion step processes these
/// into `PpirDesugaredField`s.
#[derive(Debug, Clone)]
pub struct PpirRawField {
    pub name: Name,
    /// The parsed type (carries type-level attrs extracted from field attributes).
    pub ty: PpirTy,
    /// `@stream.not_null` — field is absent until streaming completes.
    pub not_null: bool,
}
