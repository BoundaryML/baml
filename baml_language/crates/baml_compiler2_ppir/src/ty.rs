//! PPIR type representation for compiler2.
//!
//! `PpirTy` carries type-level stream annotations (`PpirTypeAttrs`) on every variant.
//! Constructed from compiler2 AST `TypeExpr` + `RawAttribute` (not from CST).

use baml_base::{Name, attr::TyAttrValue};
use baml_compiler2_ast::{RawAttribute, TypeExpr};

// ── NO-STREAMING ORIGIN ─────────────────────────────────────────────────────

/// Tracks which AST type a `CannotBeStreamed` variant originated from,
/// so we can round-trip back to the correct `TypeExpr`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CannotBeStreamedOrigin {
    Media(baml_base::MediaKind),
    Uint8Array,
    RustType,
    Error,
    Unknown,
}

// ── TYPE ATTRS ───────────────────────────────────────────────────────────────

/// Type-level attributes captured from AST field/type alias attributes.
/// Carried on every `PpirTy` variant via named `attrs` field.
///
/// Default is "no annotations" — most types have no stream attrs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct PpirTypeAttrs {
    /// `@stream.must_exist` — field is absent until streaming completes.
    pub stream_must_exist: TyAttrValue,

    /// `@stream.done` — indicates `@sap.in_progress_never`.
    pub stream_done: TyAttrValue,

    /// `@stream.with_state` — wrap final stream type in `StreamState<T>`.
    /// Captured but not expanded (deferred to a future phase).
    pub stream_with_state: TyAttrValue,
}

// ── PPIR TY ──────────────────────────────────────────────────────────────────

/// PPIR's type reference — carries type-level stream annotations on every variant.
///
/// Structurally parallel to AST `TypeExpr` but defined independently.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PpirTy {
    Named {
        path: Vec<Name>,
        /// Generic args passed at the use site (e.g. `Box<int>` → `[int]`).
        /// Empty for non-generic types and references that omit args.
        generic_args: Vec<PpirTy>,
        attrs: PpirTypeAttrs,
    },
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
    Null {
        attrs: PpirTypeAttrs,
    },
    Never {
        attrs: PpirTypeAttrs,
    },
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
    Literal {
        value: baml_base::Literal,
        attrs: PpirTypeAttrs,
    },
    /// Types with no streaming behavior: media, $`rust_type`, and error-recovery sentinels.
    /// Always expanded as `(T, InherentlyNever, NotAllowed)` — no pending default,
    /// no in-progress state.
    CannotBeStreamed {
        origin: CannotBeStreamedOrigin,
        attrs: PpirTypeAttrs,
    },
}

impl PpirTy {
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
            | Self::Literal { attrs, .. }
            | Self::CannotBeStreamed { attrs, .. } => attrs,
        }
    }

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
            | Self::Literal { attrs, .. }
            | Self::CannotBeStreamed { attrs, .. } => attrs,
        }
    }

    /// Clone this type with default (empty) attrs.
    #[must_use]
    pub fn clone_without_attrs(&self) -> Self {
        let d = PpirTypeAttrs::default();
        match self {
            Self::Named {
                path, generic_args, ..
            } => Self::Named {
                path: path.clone(),
                generic_args: generic_args.clone(),
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
            Self::Literal { value, .. } => Self::Literal {
                value: value.clone(),
                attrs: d,
            },
            Self::CannotBeStreamed { origin, .. } => Self::CannotBeStreamed {
                origin: *origin,
                attrs: d,
            },
        }
    }

    // ── Constructors ─────────────────────────────────────────────────────────

    pub fn named(name: impl Into<Name>) -> Self {
        PpirTy::Named {
            path: vec![name.into()],
            generic_args: vec![],
            attrs: PpirTypeAttrs::default(),
        }
    }

    pub fn list(inner: PpirTy) -> Self {
        PpirTy::List {
            inner: Box::new(inner),
            attrs: PpirTypeAttrs::default(),
        }
    }

    pub fn optional(inner: PpirTy) -> Self {
        PpirTy::Optional {
            inner: Box::new(inner),
            attrs: PpirTypeAttrs::default(),
        }
    }

    pub fn union(types: Vec<PpirTy>) -> Self {
        PpirTy::Union {
            variants: types,
            attrs: PpirTypeAttrs::default(),
        }
    }

    // ── AST Conversion ───────────────────────────────────────────────────────

    /// Construct a `PpirTy` from a compiler2 AST `TypeExpr`.
    ///
    /// Type attributes are read directly from each `TypeExpr` variant's `attrs` field,
    /// rather than from a flat field-level attribute list.
    pub fn from_type_expr(type_expr: &TypeExpr) -> PpirTy {
        Self::convert_type_expr(type_expr)
    }

    fn extract_type_attrs(attrs: &[RawAttribute]) -> PpirTypeAttrs {
        let mut result = PpirTypeAttrs::default();
        for attr in attrs {
            match attr.name.as_str() {
                "stream.must_exist" => result.stream_must_exist = TyAttrValue::Set,
                "stream.done" => result.stream_done = TyAttrValue::Set,
                "stream.with_state" => result.stream_with_state = TyAttrValue::Set,
                _ => {}
            }
        }
        result
    }

    fn convert_type_expr(type_expr: &TypeExpr) -> PpirTy {
        let attrs = Self::extract_type_attrs(type_expr.attrs());
        match type_expr {
            TypeExpr::Int { .. } => PpirTy::Int { attrs },
            TypeExpr::Float { .. } => PpirTy::Float { attrs },
            TypeExpr::String { .. } => PpirTy::String { attrs },
            TypeExpr::Bool { .. } => PpirTy::Bool { attrs },
            TypeExpr::Null { .. } => PpirTy::Null { attrs },
            TypeExpr::Never { .. } => PpirTy::Never { attrs },
            TypeExpr::Void { .. } => PpirTy::Never { attrs },
            TypeExpr::Path {
                segments,
                generic_args,
                ..
            } => PpirTy::Named {
                path: segments.clone(),
                generic_args: generic_args.iter().map(Self::convert_type_expr).collect(),
                attrs,
            },
            TypeExpr::Optional { inner, .. } => PpirTy::Optional {
                inner: Box::new(Self::convert_type_expr(inner)),
                attrs,
            },
            TypeExpr::List { inner, .. } => PpirTy::List {
                inner: Box::new(Self::convert_type_expr(inner)),
                attrs,
            },
            TypeExpr::Map { key, value, .. } => PpirTy::Map {
                key: Box::new(Self::convert_type_expr(key)),
                value: Box::new(Self::convert_type_expr(value)),
                attrs,
            },
            TypeExpr::Union { variants, .. } => PpirTy::Union {
                variants: variants.iter().map(Self::convert_type_expr).collect(),
                attrs,
            },
            TypeExpr::Literal { value, .. } => match value {
                baml_base::Literal::Float(_) => PpirTy::CannotBeStreamed {
                    origin: CannotBeStreamedOrigin::Unknown,
                    attrs,
                },
                _ => PpirTy::Literal {
                    value: value.clone(),
                    attrs,
                },
            },
            TypeExpr::Uint8Array { .. } => PpirTy::CannotBeStreamed {
                origin: CannotBeStreamedOrigin::Uint8Array,
                attrs,
            },
            TypeExpr::Media { kind, .. } => PpirTy::CannotBeStreamed {
                origin: CannotBeStreamedOrigin::Media(*kind),
                attrs,
            },
            TypeExpr::Rust { .. } => PpirTy::CannotBeStreamed {
                origin: CannotBeStreamedOrigin::RustType,
                attrs,
            },
            TypeExpr::Error { .. } => PpirTy::CannotBeStreamed {
                origin: CannotBeStreamedOrigin::Error,
                attrs,
            },
            TypeExpr::BuiltinUnknown { .. }
            | TypeExpr::Type { .. }
            | TypeExpr::Function { .. }
            | TypeExpr::Unknown { .. } => PpirTy::CannotBeStreamed {
                origin: CannotBeStreamedOrigin::Unknown,
                attrs,
            },
        }
    }

    /// Convert a `PpirTy` back to a `TypeExpr` for synthesized AST items.
    pub fn to_type_expr(&self) -> TypeExpr {
        match self {
            PpirTy::Named {
                path, generic_args, ..
            } => TypeExpr::Path {
                segments: path.clone(),
                generic_args: generic_args.iter().map(PpirTy::to_type_expr).collect(),
                attrs: vec![],
            },
            PpirTy::Int { .. } => TypeExpr::Int { attrs: vec![] },
            PpirTy::Float { .. } => TypeExpr::Float { attrs: vec![] },
            PpirTy::String { .. } => TypeExpr::String { attrs: vec![] },
            PpirTy::Bool { .. } => TypeExpr::Bool { attrs: vec![] },
            PpirTy::Null { .. } => TypeExpr::Null { attrs: vec![] },
            PpirTy::Never { .. } => TypeExpr::Never { attrs: vec![] },
            PpirTy::Optional { inner, .. } => TypeExpr::Optional {
                inner: Box::new(inner.to_type_expr()),
                attrs: vec![],
            },
            PpirTy::List { inner, .. } => TypeExpr::List {
                inner: Box::new(inner.to_type_expr()),
                attrs: vec![],
            },
            PpirTy::Map { key, value, .. } => TypeExpr::Map {
                key: Box::new(key.to_type_expr()),
                value: Box::new(value.to_type_expr()),
                attrs: vec![],
            },
            PpirTy::Union { variants, .. } => TypeExpr::Union {
                variants: variants.iter().map(PpirTy::to_type_expr).collect(),
                attrs: vec![],
            },
            PpirTy::Literal { value, .. } => TypeExpr::Literal {
                value: value.clone(),
                attrs: vec![],
            },
            PpirTy::CannotBeStreamed { origin, .. } => match origin {
                CannotBeStreamedOrigin::Media(kind) => TypeExpr::Media {
                    kind: *kind,
                    attrs: vec![],
                },
                CannotBeStreamedOrigin::Uint8Array => TypeExpr::Uint8Array { attrs: vec![] },
                CannotBeStreamedOrigin::RustType => TypeExpr::Rust { attrs: vec![] },
                CannotBeStreamedOrigin::Error => TypeExpr::Error { attrs: vec![] },
                CannotBeStreamedOrigin::Unknown => TypeExpr::Unknown { attrs: vec![] },
            },
        }
    }

    /// Check if this type contains null (for union pending default logic).
    pub fn contains_null(&self) -> bool {
        match self {
            PpirTy::Null { .. } => true,
            PpirTy::Optional { .. } => true,
            PpirTy::Union { variants, .. } => variants.iter().any(PpirTy::contains_null),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use baml_base::attr::TyAttrValue;
    use text_size::TextRange;

    use super::*;

    fn make_attr(name: &str) -> RawAttribute {
        RawAttribute {
            name: Name::new(name),
            args: vec![],
            span: TextRange::default(),
        }
    }

    #[test]
    fn ppir_reads_stream_must_exist_from_type_expr() {
        let type_expr = TypeExpr::Int {
            attrs: vec![make_attr("stream.must_exist")],
        };
        let ppir_ty = PpirTy::from_type_expr(&type_expr);
        assert_eq!(ppir_ty.attrs().stream_must_exist, TyAttrValue::Set);
        assert_eq!(ppir_ty.attrs().stream_done, TyAttrValue::Unset);
    }

    #[test]
    fn ppir_reads_stream_done_from_type_expr() {
        let type_expr = TypeExpr::Path {
            segments: vec![Name::new("Fizz")],
            generic_args: vec![],
            attrs: vec![make_attr("stream.done")],
        };
        let ppir_ty = PpirTy::from_type_expr(&type_expr);
        assert_eq!(ppir_ty.attrs().stream_done, TyAttrValue::Set);
        assert_eq!(ppir_ty.attrs().stream_must_exist, TyAttrValue::Unset);
    }

    #[test]
    fn ppir_reads_stream_with_state_from_type_expr() {
        let type_expr = TypeExpr::String {
            attrs: vec![make_attr("stream.with_state")],
        };
        let ppir_ty = PpirTy::from_type_expr(&type_expr);
        assert_eq!(ppir_ty.attrs().stream_with_state, TyAttrValue::Set);
    }

    #[test]
    fn ppir_multiple_attrs_on_type_expr() {
        let type_expr = TypeExpr::Int {
            attrs: vec![make_attr("stream.must_exist"), make_attr("stream.done")],
        };
        let ppir_ty = PpirTy::from_type_expr(&type_expr);
        assert_eq!(ppir_ty.attrs().stream_must_exist, TyAttrValue::Set);
        assert_eq!(ppir_ty.attrs().stream_done, TyAttrValue::Set);
    }

    #[test]
    fn ppir_no_attrs_gives_default() {
        let type_expr = TypeExpr::Int { attrs: vec![] };
        let ppir_ty = PpirTy::from_type_expr(&type_expr);
        assert_eq!(ppir_ty.attrs().stream_must_exist, TyAttrValue::Unset);
        assert_eq!(ppir_ty.attrs().stream_done, TyAttrValue::Unset);
        assert_eq!(ppir_ty.attrs().stream_with_state, TyAttrValue::Unset);
    }

    #[test]
    fn ppir_nested_type_inner_gets_own_attrs() {
        // Optional(Int @stream.done) — the inner Int has the attr, outer Optional does not
        let inner = TypeExpr::Int {
            attrs: vec![make_attr("stream.done")],
        };
        let outer = TypeExpr::Optional {
            inner: Box::new(inner),
            attrs: vec![make_attr("stream.must_exist")],
        };
        let ppir_ty = PpirTy::from_type_expr(&outer);

        // Outer: stream.must_exist
        assert_eq!(ppir_ty.attrs().stream_must_exist, TyAttrValue::Set);
        assert_eq!(ppir_ty.attrs().stream_done, TyAttrValue::Unset);

        // Inner: stream.done
        if let PpirTy::Optional { inner, .. } = &ppir_ty {
            assert_eq!(inner.attrs().stream_done, TyAttrValue::Set);
            assert_eq!(inner.attrs().stream_must_exist, TyAttrValue::Unset);
        } else {
            panic!("expected PpirTy::Optional");
        }
    }

    #[test]
    fn ppir_unknown_attrs_are_ignored() {
        let type_expr = TypeExpr::Int {
            attrs: vec![make_attr("alias"), make_attr("stream.done")],
        };
        let ppir_ty = PpirTy::from_type_expr(&type_expr);
        // @alias is not a type attr — it's ignored by extract_type_attrs
        assert_eq!(ppir_ty.attrs().stream_done, TyAttrValue::Set);
        assert_eq!(ppir_ty.attrs().stream_must_exist, TyAttrValue::Unset);
    }
}

// ── PPIR RAW FIELD ───────────────────────────────────────────────────────────

/// A PPIR field with parsed type and type-level stream annotations.
#[derive(Debug, Clone)]
pub struct PpirRawField {
    pub name: Name,
    pub ty: PpirTy,
}
