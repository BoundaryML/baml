//! Stream expansion logic and output types for compiler2.
//!
//! PPIR expansion computes per-field expansion data (`stream_type`, `sap_starts_as`,
//! `sap_in_progress_never`) and per-alias expanded bodies. The synthesize module
//! consumes these to build synthetic AST items.

use baml_base::Name;
use baml_compiler2_hir::{contributions::Definition, package::PackageItems};
use rustc_hash::FxHashMap;
use smol_str::SmolStr;

use crate::ty::{PpirRawField, PpirTy, PpirTypeAttrs};

/// Symbol classification result for stream expansion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Class,
    Enum,
    TypeAlias,
}

/// Classify a type path using HIR's package-level name resolution.
pub fn classify_type(package_items: &PackageItems<'_>, path: &[Name]) -> Option<SymbolKind> {
    package_items.lookup_type(path).and_then(|def| match def {
        Definition::Class(_) => Some(SymbolKind::Class),
        Definition::Enum(_) => Some(SymbolKind::Enum),
        Definition::TypeAlias(_) => Some(SymbolKind::TypeAlias),
        _ => None,
    })
}

//
// ──────────────────────────────────────────────── OUTPUT TYPES ─────
//

/// SAP starts-as value, synthesized from `@stream.not_null` / defaults.
/// This becomes the `@sap.class_completed_field_missing` and `@sap.class_in_progress_field_missing`
/// attribute values, computed as part of `@stream.*` desugaring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PpirStreamStartsAs {
    /// Field is absent until it begins streaming.
    /// From `@stream.not_null`, `@@stream.done`, or default for literal/never `stream_types`.
    Never,
    /// Default value computed during PPIR expansion from `stream_type`'s syntactic category.
    /// null for scalars, `[]` for lists, `{}` for maps, never for literals.
    DefaultFor(PpirTy),
}

impl PpirStreamStartsAs {
    /// Extract the type representation for union computation.
    /// Used as one side of `sap_starts_as_type | stream_type`.
    pub fn as_ty(&self) -> Option<PpirTy> {
        match self {
            PpirStreamStartsAs::Never => Some(PpirTy::Never {
                attrs: PpirTypeAttrs::default(),
            }),
            PpirStreamStartsAs::DefaultFor(ty) => Some(ty.clone()),
        }
    }
}

/// Per-class desugared results. Carries the original class name (NOT `stream_*`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpirDesugaredClass {
    pub name: Name,
    pub fields: Vec<PpirDesugaredField>,
}

/// Per-field desugared results with synthesized `@sap.*` attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpirDesugaredField {
    pub name: Name,
    /// The during-streaming type — result of `stream_expand` on the field's type.
    pub stream_type: PpirTy,
    /// `@sap.in_progress(never)` — synthesized from `@stream.done`.
    pub sap_in_progress_never: bool,
    /// Synthesized from `@stream.starts_as` / `@stream.not_null` / defaults.
    /// Becomes `@sap.class_completed_field_missing` and `@sap.class_in_progress_field_missing`.
    pub sap_starts_as: PpirStreamStartsAs,
}

/// Per-alias desugared results. Carries the original alias name (NOT `stream_*`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpirDesugaredTypeAlias {
    pub name: Name,
    /// The result of `stream_expand` on the alias body.
    pub expanded_body: PpirTy,
}

//
// ──────────────────────────────────────────── STREAM EXPAND ─────
//

/// Compute the stream-expanded type from a `PpirTy`.
///
/// Checks `PpirTypeAttrs` before recursing:
/// - `@stream.type(D)`: use D, don't recurse
/// - `@stream.done` (without `stream_type`): use T as-is (atomic)
/// - Otherwise: normal recursive expansion using HIR name classification
pub fn stream_expand(ty: &PpirTy, package_items: &PackageItems<'_>) -> PpirTy {
    let attrs = ty.attrs();

    // Explicit @stream.type(D) — use D directly
    if let Some(d) = &attrs.stream_type {
        return (**d).clone();
    }

    // @stream.done without explicit type — type is atomic, keep as-is
    if attrs.stream_done {
        return ty.clone_without_attrs();
    }

    // Normal recursive expansion using HIR classification
    match ty {
        PpirTy::Int { .. } | PpirTy::Float { .. } | PpirTy::String { .. } | PpirTy::Bool { .. } => {
            ty.clone_without_attrs()
        }

        PpirTy::Null { .. } => PpirTy::Null {
            attrs: PpirTypeAttrs::default(),
        },
        PpirTy::Never { .. } => PpirTy::Never {
            attrs: PpirTypeAttrs::default(),
        },

        PpirTy::StringLiteral { .. } | PpirTy::IntLiteral { .. } | PpirTy::BoolLiteral { .. } => {
            ty.clone_without_attrs()
        }

        // HIR-based classification: class/type_alias → stream_*, enum → unchanged
        PpirTy::Named { path, .. } => {
            match classify_type(package_items, path) {
                Some(SymbolKind::Class | SymbolKind::TypeAlias) => {
                    let (bare_name, prefix) = path.split_last().expect("non-empty path");
                    PpirTy::Named {
                        path: prefix
                            .iter()
                            .cloned()
                            .chain(std::iter::once(SmolStr::new(format!("stream_{bare_name}"))))
                            .collect(),
                        attrs: PpirTypeAttrs::default(),
                    }
                }
                _ => ty.clone_without_attrs(), // Enum, unknown, builtin — unchanged
            }
        }

        PpirTy::List { inner, .. } => PpirTy::List {
            inner: Box::new(stream_expand(inner, package_items)),
            attrs: PpirTypeAttrs::default(),
        },

        PpirTy::Map { key, value, .. } => PpirTy::Map {
            key: key.clone(),
            value: Box::new(stream_expand(value, package_items)),
            attrs: PpirTypeAttrs::default(),
        },

        PpirTy::Union { variants, .. } => PpirTy::Union {
            variants: variants
                .iter()
                .map(|v| stream_expand(v, package_items))
                .collect(),
            attrs: PpirTypeAttrs::default(),
        },

        PpirTy::Optional { inner, .. } => PpirTy::Union {
            variants: vec![
                stream_expand(inner, package_items),
                PpirTy::Null {
                    attrs: PpirTypeAttrs::default(),
                },
            ],
            attrs: PpirTypeAttrs::default(),
        },

        _ => ty.clone_without_attrs(),
    }
}

//
// ──────────────────────────────────────── DEFAULT SAP STARTS-AS ─────
//

/// Compute the default starts-as value from a field's `stream_type`.
///
/// Per the stream-types spec:
/// - Literal types → never (absent until complete)
/// - Never → never
/// - List → empty list (`list<never>`)
/// - Map → empty map (`map<key, never>`)
/// - Everything else → null
pub fn default_sap_starts_as(stream_type: &PpirTy) -> PpirStreamStartsAs {
    match stream_type {
        PpirTy::StringLiteral { .. }
        | PpirTy::IntLiteral { .. }
        | PpirTy::BoolLiteral { .. }
        | PpirTy::Never { .. }
        | PpirTy::RustType { .. } => PpirStreamStartsAs::Never,

        PpirTy::List { .. } => PpirStreamStartsAs::DefaultFor(PpirTy::List {
            inner: Box::new(PpirTy::Never {
                attrs: PpirTypeAttrs::default(),
            }),
            attrs: PpirTypeAttrs::default(),
        }),

        PpirTy::Map { key, .. } => PpirStreamStartsAs::DefaultFor(PpirTy::Map {
            key: key.clone(),
            value: Box::new(PpirTy::Never {
                attrs: PpirTypeAttrs::default(),
            }),
            attrs: PpirTypeAttrs::default(),
        }),

        _ => PpirStreamStartsAs::DefaultFor(PpirTy::Null {
            attrs: PpirTypeAttrs::default(),
        }),
    }
}

//
// ──────────────────────────────────────── BUILDING PPIR FIELDS ─────
//

/// Build a `PpirRawField` from a compiler2 AST `FieldDef`.
///
/// Type-level annotations (`@stream.done`, `@stream.with_state`)
/// are captured by `PpirTy::from_type_expr()` on the field's type.
/// Field-level annotation `@stream.not_null` is read from the field's `RawAttribute` list.
pub fn build_ppir_field(field: &baml_compiler2_ast::FieldDef) -> PpirRawField {
    let type_expr = field
        .type_expr
        .as_ref()
        .map(|s| &s.expr)
        .unwrap_or(&baml_compiler2_ast::TypeExpr::Unknown);

    let ty = PpirTy::from_type_expr(type_expr, &field.attributes);

    let not_null = field
        .attributes
        .iter()
        .any(|a| a.name.as_str() == "stream.not_null");

    PpirRawField {
        name: field.name.clone(),
        ty,
        not_null,
    }
}

//
// ──────────────────────────────────────── FIELD DESUGARING ─────
//

/// Desugar a single field's stream annotations into `PpirDesugaredField`.
///
/// Computes `stream_type` via `stream_expand`, synthesizes @sap.* attributes.
pub fn desugar_field(
    pf: &PpirRawField,
    package_items: &PackageItems<'_>,
    block_attrs: &FxHashMap<Vec<Name>, Vec<Name>>,
) -> PpirDesugaredField {
    // 1. Compute stream_type via stream_expand (respects type-level attrs)
    let stream_type = stream_expand(&pf.ty, package_items);

    // 2. Synthesize @sap.in_progress from @stream.done
    let sap_in_progress_never = pf.ty.attrs().stream_done;

    // 3. Synthesize sap_starts_as from @stream.not_null / defaults
    //    Priority order:
    //    1. Field-level @stream.not_null → Never
    //    2. Type-level @@stream.not_null on referenced type → Never
    //    3. Default from stream_type
    let sap_starts_as =
        if pf.not_null || type_has_block_attr(&pf.ty, "stream.not_null", block_attrs) {
            PpirStreamStartsAs::Never
        } else {
            default_sap_starts_as(&stream_type)
        };

    PpirDesugaredField {
        name: pf.name.clone(),
        stream_type,
        sap_in_progress_never,
        sap_starts_as,
    }
}

/// Check if the field's top-level type references a class/enum that has
/// a specific @@stream.* block attribute.
///
/// Only matches bare named types (e.g., `Foo`). Does NOT match `Foo[]`, `Foo?`,
/// `Foo | Bar`, etc. — those use their own default `starts_as` behavior.
fn type_has_block_attr(
    ty: &PpirTy,
    attr: &str,
    block_attrs: &FxHashMap<Vec<Name>, Vec<Name>>,
) -> bool {
    let PpirTy::Named { path, .. } = ty else {
        return false;
    };
    block_attrs
        .get(path)
        .is_some_and(|attrs| attrs.iter().any(|a| a == attr))
}
