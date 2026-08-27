//! PPIR type representation for compiler2.
//!
//! `PpirTy` carries type-level stream annotations (`PpirTypeAttrs`) on every variant.
//! Constructed from compiler2 AST `TypeExpr` + `RawAttribute` (not from CST).

use baml_base::{Name, attr::TyAttrValue};
use baml_compiler2_ast::{RawAttribute, TypeExpr, TypeExprKind};

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
        /// Associated-type bindings written at the use site
        /// (e.g. `Iterator<Item = int>` → `[("Item", int)]`). Carried through
        /// the round-trip verbatim — dropping them would materialize
        /// synthesized `$stream` fields at a *different* (under-pinned) type
        /// than the source spelled.
        associated_type_bindings: Vec<(Name, PpirTy)>,
        attrs: PpirTypeAttrs,
    },
    Int {
        attrs: PpirTypeAttrs,
    },
    Bigint {
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
            | Self::Bigint { attrs }
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
            | Self::Bigint { attrs }
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
                path,
                generic_args,
                associated_type_bindings,
                ..
            } => Self::Named {
                path: path.clone(),
                generic_args: generic_args.clone(),
                associated_type_bindings: associated_type_bindings.clone(),
                attrs: d,
            },
            Self::Int { .. } => Self::Int { attrs: d },
            Self::Bigint { .. } => Self::Bigint { attrs: d },
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
        match &type_expr.kind {
            TypeExprKind::Int { .. } => PpirTy::Int { attrs },
            TypeExprKind::Bigint { .. } => PpirTy::Bigint { attrs },
            TypeExprKind::Float { .. } => PpirTy::Float { attrs },
            TypeExprKind::String { .. } => PpirTy::String { attrs },
            TypeExprKind::Bool { .. } => PpirTy::Bool { attrs },
            TypeExprKind::Null { .. } => PpirTy::Null { attrs },
            TypeExprKind::Never { .. } => PpirTy::Never { attrs },
            TypeExprKind::Void { .. } => PpirTy::Never { attrs },
            TypeExprKind::Path {
                segments,
                generic_args,
                associated_type_bindings,
                ..
            } => PpirTy::Named {
                path: segments.clone(),
                generic_args: generic_args.iter().map(Self::convert_type_expr).collect(),
                associated_type_bindings: associated_type_bindings
                    .iter()
                    .map(|binding| (binding.name.clone(), Self::convert_type_expr(&binding.ty)))
                    .collect(),
                attrs,
            },
            TypeExprKind::Optional { inner, .. } => PpirTy::Optional {
                inner: Box::new(Self::convert_type_expr(inner)),
                attrs,
            },
            TypeExprKind::List { inner, .. } => PpirTy::List {
                inner: Box::new(Self::convert_type_expr(inner)),
                attrs,
            },
            TypeExprKind::Map { key, value, .. } => PpirTy::Map {
                key: Box::new(Self::convert_type_expr(key)),
                value: Box::new(Self::convert_type_expr(value)),
                attrs,
            },
            TypeExprKind::Union { variants, .. } => PpirTy::Union {
                variants: variants.iter().map(Self::convert_type_expr).collect(),
                attrs,
            },
            TypeExprKind::Literal { value, .. } => match value {
                baml_base::Literal::Float(_) => PpirTy::CannotBeStreamed {
                    origin: CannotBeStreamedOrigin::Unknown,
                    attrs,
                },
                _ => PpirTy::Literal {
                    value: value.clone(),
                    attrs,
                },
            },
            TypeExprKind::Uint8Array { .. } => PpirTy::CannotBeStreamed {
                origin: CannotBeStreamedOrigin::Uint8Array,
                attrs,
            },
            TypeExprKind::Media { kind, .. } => PpirTy::CannotBeStreamed {
                origin: CannotBeStreamedOrigin::Media(*kind),
                attrs,
            },
            TypeExprKind::Rust { .. } => PpirTy::CannotBeStreamed {
                origin: CannotBeStreamedOrigin::RustType,
                attrs,
            },
            TypeExprKind::Error { .. } => PpirTy::CannotBeStreamed {
                origin: CannotBeStreamedOrigin::Error,
                attrs,
            },
            TypeExprKind::Unknown { .. }
            | TypeExprKind::Unreflect { .. }
            | TypeExprKind::AssociatedTypeProjection { .. }
            | TypeExprKind::Type { .. }
            | TypeExprKind::Function { .. }
            // A `_` inference hole is opaque to streaming analysis.
            | TypeExprKind::Infer { .. }
            | TypeExprKind::Missing { .. } => PpirTy::CannotBeStreamed {
                origin: CannotBeStreamedOrigin::Unknown,
                attrs,
            },
        }
    }

    /// Convert a `PpirTy` back to a `TypeExpr` for synthesized AST items.
    /// `PpirTy` carries no source span, so these reconstructed nodes are
    /// synthetic (`TextRange::default()`).
    pub fn to_type_expr(&self) -> TypeExpr {
        let kind = match self {
            PpirTy::Named {
                path,
                generic_args,
                associated_type_bindings,
                ..
            } => TypeExprKind::Path {
                segments: path.clone(),
                generic_args: generic_args.iter().map(PpirTy::to_type_expr).collect(),
                associated_type_bindings: associated_type_bindings
                    .iter()
                    .map(|(name, value)| baml_compiler2_ast::AssociatedTypeBinding {
                        name: name.clone(),
                        ty: Box::new(value.to_type_expr()),
                    })
                    .collect(),
                attrs: vec![],
            },
            PpirTy::Int { .. } => TypeExprKind::Int { attrs: vec![] },
            PpirTy::Bigint { .. } => TypeExprKind::Bigint { attrs: vec![] },
            PpirTy::Float { .. } => TypeExprKind::Float { attrs: vec![] },
            PpirTy::String { .. } => TypeExprKind::String { attrs: vec![] },
            PpirTy::Bool { .. } => TypeExprKind::Bool { attrs: vec![] },
            PpirTy::Null { .. } => TypeExprKind::Null { attrs: vec![] },
            PpirTy::Never { .. } => TypeExprKind::Never { attrs: vec![] },
            PpirTy::Optional { inner, .. } => TypeExprKind::Optional {
                inner: Box::new(inner.to_type_expr()),
                attrs: vec![],
            },
            PpirTy::List { inner, .. } => TypeExprKind::List {
                inner: Box::new(inner.to_type_expr()),
                attrs: vec![],
            },
            PpirTy::Map { key, value, .. } => TypeExprKind::Map {
                key: Box::new(key.to_type_expr()),
                value: Box::new(value.to_type_expr()),
                attrs: vec![],
            },
            PpirTy::Union { variants, .. } => TypeExprKind::Union {
                variants: variants.iter().map(PpirTy::to_type_expr).collect(),
                attrs: vec![],
            },
            PpirTy::Literal { value, .. } => TypeExprKind::Literal {
                value: value.clone(),
                attrs: vec![],
            },
            PpirTy::CannotBeStreamed { origin, .. } => match origin {
                CannotBeStreamedOrigin::Media(kind) => TypeExprKind::Media {
                    kind: *kind,
                    attrs: vec![],
                },
                CannotBeStreamedOrigin::Uint8Array => TypeExprKind::Uint8Array { attrs: vec![] },
                CannotBeStreamedOrigin::RustType => TypeExprKind::Rust { attrs: vec![] },
                CannotBeStreamedOrigin::Error => TypeExprKind::Error { attrs: vec![] },
                // `Unknown` covers function-typed fields as well as genuinely
                // unresolved ones (both map to this origin on the way in). When
                // this materializes a synthesized `$stream` class field it must
                // be a *valid* runtime type — a non-streamable field is just
                // opaque during streaming — so reconstruct it as the `unknown`
                // type rather than the error-recovery `Error` sentinel, which
                // would trip the runtime lowering boundary (`Ty::Error` has no
                // `RuntimeTy`).
                CannotBeStreamedOrigin::Unknown => TypeExprKind::Unknown { attrs: vec![] },
            },
        };
        kind.at(text_size::TextRange::default())
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
        let type_expr = TypeExprKind::Int {
            attrs: vec![make_attr("stream.must_exist")],
        };
        let ppir_ty = PpirTy::from_type_expr(&type_expr.at(TextRange::default()));
        assert_eq!(ppir_ty.attrs().stream_must_exist, TyAttrValue::Set);
        assert_eq!(ppir_ty.attrs().stream_done, TyAttrValue::Unset);
    }

    #[test]
    fn ppir_reads_stream_done_from_type_expr() {
        let type_expr = TypeExprKind::Path {
            segments: vec![Name::new("Fizz")],
            generic_args: vec![],
            associated_type_bindings: vec![],
            attrs: vec![make_attr("stream.done")],
        };
        let ppir_ty = PpirTy::from_type_expr(&type_expr.at(TextRange::default()));
        assert_eq!(ppir_ty.attrs().stream_done, TyAttrValue::Set);
        assert_eq!(ppir_ty.attrs().stream_must_exist, TyAttrValue::Unset);
    }

    #[test]
    fn ppir_reads_stream_with_state_from_type_expr() {
        let type_expr = TypeExprKind::String {
            attrs: vec![make_attr("stream.with_state")],
        };
        let ppir_ty = PpirTy::from_type_expr(&type_expr.at(TextRange::default()));
        assert_eq!(ppir_ty.attrs().stream_with_state, TyAttrValue::Set);
    }

    #[test]
    fn ppir_multiple_attrs_on_type_expr() {
        let type_expr = TypeExprKind::Int {
            attrs: vec![make_attr("stream.must_exist"), make_attr("stream.done")],
        };
        let ppir_ty = PpirTy::from_type_expr(&type_expr.at(TextRange::default()));
        assert_eq!(ppir_ty.attrs().stream_must_exist, TyAttrValue::Set);
        assert_eq!(ppir_ty.attrs().stream_done, TyAttrValue::Set);
    }

    #[test]
    fn ppir_no_attrs_gives_default() {
        let type_expr = TypeExprKind::Int { attrs: vec![] };
        let ppir_ty = PpirTy::from_type_expr(&type_expr.at(TextRange::default()));
        assert_eq!(ppir_ty.attrs().stream_must_exist, TyAttrValue::Unset);
        assert_eq!(ppir_ty.attrs().stream_done, TyAttrValue::Unset);
        assert_eq!(ppir_ty.attrs().stream_with_state, TyAttrValue::Unset);
    }

    #[test]
    fn ppir_nested_type_inner_gets_own_attrs() {
        // Optional(Int @stream.done) — the inner Int has the attr, outer Optional does not
        let inner = TypeExprKind::Int {
            attrs: vec![make_attr("stream.done")],
        };
        let outer = TypeExprKind::Optional {
            inner: Box::new(inner.at(TextRange::default())),
            attrs: vec![make_attr("stream.must_exist")],
        };
        let ppir_ty = PpirTy::from_type_expr(&outer.at(TextRange::default()));

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
        let type_expr = TypeExprKind::Int {
            attrs: vec![make_attr("alias"), make_attr("stream.done")],
        };
        let ppir_ty = PpirTy::from_type_expr(&type_expr.at(TextRange::default()));
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
