//! Stream expansion algorithm (BEP-006 v12).
//!
//! Implements `stream_expand` and `expand_partial` from the spec.

use baml_base::{Name, attr::TyAttrValue};
use baml_compiler2_hir::{contributions::Definition, package::PackageItems};
use rustc_hash::FxHashMap;
use smol_str::SmolStr;

use crate::ty::{PpirTy, PpirTypeAttrs};

// ── Symbol Classification ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Class,
    Enum,
    TypeAlias,
}

pub fn classify_type(package_items: &PackageItems<'_>, path: &[Name]) -> Option<SymbolKind> {
    package_items.lookup_type(path).and_then(|def| match def {
        Definition::Class(_) => Some(SymbolKind::Class),
        Definition::Enum(_) => Some(SymbolKind::Enum),
        Definition::TypeAlias(_) => Some(SymbolKind::TypeAlias),
        _ => None,
    })
}

/// Classify a type, falling back to cross-package lookup if the current package
/// doesn't contain the path. For example, `["baml", "http", "Request"]` is
/// looked up first in the current package, and if not found, the first segment
/// is tried as a package name with the remainder as the intra-package path.
fn classify_type_cross_pkg(path: &[Name], ctx: &ExpandCtx<'_>) -> Option<SymbolKind> {
    // Try current package first
    if let Some(kind) = classify_type(ctx.package_items, path) {
        return Some(kind);
    }
    // Try interpreting the first segment as a foreign package name
    if path.len() >= 2 {
        let pkg_name = &path[0];
        let rest = &path[1..];
        if let Some(foreign_items) = ctx.all_package_items.get(pkg_name) {
            return classify_type(foreign_items, rest);
        }
    }
    None
}

// ── Context ──────────────────────────────────────────────────────────────────

/// Shared context threaded through all stream-expansion functions.
pub struct ExpandCtx<'ctx> {
    pub package_name: &'ctx Name,
    pub package_items: &'ctx PackageItems<'ctx>,
    /// All packages' items keyed by package name, for cross-package type resolution.
    pub all_package_items: &'ctx FxHashMap<Name, &'ctx PackageItems<'ctx>>,
    pub block_attrs: &'ctx FxHashMap<Vec<Name>, Vec<Name>>,
    pub alias_bodies: &'ctx FxHashMap<Vec<Name>, PpirTy>,
}

// ── Output Types ─────────────────────────────────────────────────────────────

/// Generated SAP attributes for a stream-expanded field.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SapAttrs {
    pub parse_without_null: TyAttrValue,
    pub pending_never: TyAttrValue,
    pub in_progress_never: TyAttrValue,
}

// ── Pending Default ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingDefault {
    Never,
    Null,
    EmptyArray,
    EmptyMap,
}

fn pending_default(ty: &PpirTy, ctx: &ExpandCtx<'_>, depth: u32) -> PendingDefault {
    match ty {
        PpirTy::Literal { .. } => PendingDefault::Never,

        PpirTy::Never { .. } => PendingDefault::Never,

        PpirTy::CannotBeStreamed { .. } => PendingDefault::Never,

        PpirTy::List { .. } => PendingDefault::EmptyArray,

        PpirTy::Map { .. } => PendingDefault::EmptyMap,

        PpirTy::Union { variants, .. } => union_pending_default(variants, ctx, depth),

        PpirTy::Named { path, .. } => {
            // Check if the named type has @@stream.must_exist
            if let Some(attrs) = lookup_block_attrs(path, ctx) {
                if attrs.iter().any(|a| a.as_str() == "stream.must_exist") {
                    return PendingDefault::Never;
                }
            }
            match classify_type_cross_pkg(path, ctx) {
                Some(SymbolKind::TypeAlias) => {
                    if depth < MAX_ALIAS_DEPTH {
                        let mut pkg_path = vec![ctx.package_name.clone()];
                        pkg_path.extend_from_slice(path);
                        if let Some(body) = ctx.alias_bodies.get(&pkg_path) {
                            return pending_default(body, ctx, depth + 1);
                        }
                    }
                    PendingDefault::Null // fallback if alias body not found or depth exceeded
                }
                _ => PendingDefault::Null, // class, enum, unknown
            }
        }

        // int, float, bool, string, null, optional, T$stream, unknown
        _ => PendingDefault::Null,
    }
}

fn union_pending_default(variants: &[PpirTy], ctx: &ExpandCtx<'_>, depth: u32) -> PendingDefault {
    for v in variants {
        let pd = pending_default(v, ctx, depth);
        if pd != PendingDefault::Never {
            return pd;
        }
    }
    PendingDefault::Never
}

// ── expand_partial ───────────────────────────────────────────────────────────

/// Recursive type expansion for "inside containers".
/// Per the BEP-006 v12 `expand_partial` table.
pub fn expand_partial(ty: &PpirTy, ctx: &ExpandCtx<'_>) -> PpirTy {
    let d = PpirTypeAttrs::default();
    match ty {
        // Primitives/literals/never/opaque/enum → unchanged
        PpirTy::Int { .. }
        | PpirTy::Float { .. }
        | PpirTy::String { .. }
        | PpirTy::Bool { .. }
        | PpirTy::Null { .. }
        | PpirTy::Never { .. }
        | PpirTy::Literal { .. }
        | PpirTy::CannotBeStreamed { .. } => ty.clone_without_attrs(),

        // Named types — depends on classification
        PpirTy::Named { path, .. } => {
            // Already *$stream → unchanged
            if path.last().is_some_and(|n| n.as_str().ends_with("$stream")) {
                return ty.clone_without_attrs();
            }
            match classify_type_cross_pkg(path, ctx) {
                Some(SymbolKind::Enum) => ty.clone_without_attrs(),
                Some(SymbolKind::Class | SymbolKind::TypeAlias) => {
                    let (bare_name, prefix) = path.split_last().expect("non-empty path");
                    PpirTy::Named {
                        path: prefix
                            .iter()
                            .cloned()
                            .chain(std::iter::once(SmolStr::new(format!("{bare_name}$stream"))))
                            .collect(),
                        attrs: d,
                    }
                }
                None => ty.clone_without_attrs(),
            }
        }

        // Containers → recurse into inner types
        PpirTy::List { inner, .. } => PpirTy::List {
            inner: Box::new(expand_partial(inner, ctx)),
            attrs: d,
        },

        PpirTy::Map { key, value, .. } => PpirTy::Map {
            key: key.clone(),
            value: Box::new(expand_partial(value, ctx)),
            attrs: d,
        },

        // Optional → null | expand_partial(inner)
        PpirTy::Optional { inner, .. } => PpirTy::Union {
            variants: vec![
                PpirTy::Null { attrs: d.clone() },
                expand_partial(inner, ctx),
            ],
            attrs: d,
        },

        // Union → expand each variant
        PpirTy::Union { variants, .. } => PpirTy::Union {
            variants: variants.iter().map(|v| expand_partial(v, ctx)).collect(),
            attrs: d,
        },
    }
}

// ── stream_expand ────────────────────────────────────────────────────────────

/// Internal enums for the `stream_expand` algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DefaultWhenPending {
    PrependNull,
    HasDefault,
    InherentlyNever,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InProgress {
    NotAllowed,
    Allowed,
}

/// The full BEP-006 v12 `stream_expand` algorithm.
///
/// Given `class C { f: T @stream.must_exist? @stream.done? }`, produces the
/// type and SAP attributes for `class C$stream { f: <type> <attrs> }`.
pub fn stream_expand(ty: &PpirTy, ctx: &ExpandCtx<'_>) -> (PpirTy, SapAttrs) {
    stream_expand_inner(ty, ctx, 0)
}

/// Max alias resolution depth to prevent infinite loops on cyclic aliases.
const MAX_ALIAS_DEPTH: u32 = 32;

fn stream_expand_inner(ty: &PpirTy, ctx: &ExpandCtx<'_>, depth: u32) -> (PpirTy, SapAttrs) {
    let mut must_exist = ty.attrs().stream_must_exist;
    let mut done = ty.attrs().stream_done;
    let d = PpirTypeAttrs::default();

    let (mut stream_type, default_when_pending, in_progress) = match ty {
        // Primitive/atomic types
        PpirTy::Int { .. } | PpirTy::Float { .. } | PpirTy::Bool { .. } => (
            ty.clone_without_attrs(),
            DefaultWhenPending::PrependNull,
            InProgress::NotAllowed,
        ),
        PpirTy::String { .. } => (
            PpirTy::String { attrs: d.clone() },
            DefaultWhenPending::PrependNull,
            InProgress::Allowed,
        ),
        PpirTy::Null { .. } => (
            PpirTy::Null { attrs: d.clone() },
            DefaultWhenPending::HasDefault,
            InProgress::Allowed,
        ),
        PpirTy::Literal { .. } => (
            ty.clone_without_attrs(),
            DefaultWhenPending::InherentlyNever,
            InProgress::NotAllowed,
        ),
        PpirTy::Never { .. } => (
            PpirTy::Never { attrs: d.clone() },
            DefaultWhenPending::InherentlyNever,
            InProgress::NotAllowed,
        ),
        PpirTy::CannotBeStreamed { .. } => (
            ty.clone_without_attrs(),
            DefaultWhenPending::InherentlyNever,
            InProgress::NotAllowed,
        ),

        // Named types
        PpirTy::Named { path, .. } => {
            // Already *$stream → treat like T$stream
            if path.last().is_some_and(|n| n.as_str().ends_with("$stream")) {
                (
                    ty.clone_without_attrs(),
                    DefaultWhenPending::PrependNull,
                    InProgress::Allowed,
                )
            } else {
                match classify_type_cross_pkg(path, ctx) {
                    Some(SymbolKind::Enum) => {
                        // Merge @@stream.* block attrs
                        merge_block_attrs(path, ctx, &mut must_exist, &mut done);
                        (
                            ty.clone_without_attrs(),
                            DefaultWhenPending::PrependNull,
                            InProgress::NotAllowed,
                        )
                    }
                    Some(SymbolKind::Class) => {
                        // Merge @@stream.* block attrs
                        merge_block_attrs(path, ctx, &mut must_exist, &mut done);
                        let (bare_name, prefix) = path.split_last().expect("non-empty path");
                        let stream_path: Vec<Name> = prefix
                            .iter()
                            .cloned()
                            .chain(std::iter::once(SmolStr::new(format!("{bare_name}$stream"))))
                            .collect();
                        (
                            PpirTy::Named {
                                path: stream_path,
                                attrs: d.clone(),
                            },
                            DefaultWhenPending::PrependNull,
                            InProgress::Allowed,
                        )
                    }
                    Some(SymbolKind::TypeAlias) => {
                        // Merge @@stream.* block attrs, then resolve alias recursively
                        merge_block_attrs(path, ctx, &mut must_exist, &mut done);
                        if depth < MAX_ALIAS_DEPTH {
                            let mut pkg_path = vec![ctx.package_name.clone()];
                            pkg_path.extend_from_slice(path);
                            if let Some(body) = ctx.alias_bodies.get(&pkg_path) {
                                // Set merged attrs on the resolved body and recurse
                                let mut resolved = body.clone();
                                resolved.attrs_mut().stream_must_exist = must_exist;
                                resolved.attrs_mut().stream_done = done;
                                return stream_expand_inner(&resolved, ctx, depth + 1);
                            }
                        }
                        // Fallback: treat like class (Name$stream)
                        let (bare_name, prefix) = path.split_last().expect("non-empty path");
                        let stream_path: Vec<Name> = prefix
                            .iter()
                            .cloned()
                            .chain(std::iter::once(SmolStr::new(format!("{bare_name}$stream"))))
                            .collect();
                        (
                            PpirTy::Named {
                                path: stream_path,
                                attrs: d.clone(),
                            },
                            DefaultWhenPending::PrependNull,
                            InProgress::Allowed,
                        )
                    }
                    None => {
                        // Unknown — treat conservatively
                        (
                            ty.clone_without_attrs(),
                            DefaultWhenPending::PrependNull,
                            InProgress::Allowed,
                        )
                    }
                }
            }
        }

        // Composites
        PpirTy::List { inner, .. } => (
            PpirTy::List {
                inner: Box::new(expand_partial(inner, ctx)),
                attrs: d.clone(),
            },
            DefaultWhenPending::HasDefault,
            InProgress::Allowed,
        ),
        PpirTy::Map { key, value, .. } => (
            PpirTy::Map {
                key: key.clone(),
                value: Box::new(expand_partial(value, ctx)),
                attrs: d.clone(),
            },
            DefaultWhenPending::HasDefault,
            InProgress::Allowed,
        ),

        // Optional → null | expand_partial(inner)
        PpirTy::Optional { inner, .. } => (
            PpirTy::Union {
                variants: vec![
                    PpirTy::Null { attrs: d.clone() },
                    expand_partial(inner, ctx),
                ],
                attrs: d.clone(),
            },
            DefaultWhenPending::HasDefault,
            InProgress::Allowed,
        ),

        // Union — special case
        PpirTy::Union { variants, .. } => {
            let expanded: Vec<PpirTy> = variants.iter().map(|v| expand_partial(v, ctx)).collect();
            let pd = union_pending_default(variants, ctx, depth);
            let dwp = match pd {
                PendingDefault::Never => DefaultWhenPending::InherentlyNever,
                PendingDefault::Null => {
                    let expanded_union = PpirTy::Union {
                        variants: expanded.clone(),
                        attrs: d.clone(),
                    };
                    if expanded_union.contains_null() {
                        DefaultWhenPending::HasDefault
                    } else {
                        DefaultWhenPending::PrependNull
                    }
                }
                PendingDefault::EmptyArray | PendingDefault::EmptyMap => {
                    DefaultWhenPending::HasDefault
                }
            };
            (
                PpirTy::Union {
                    variants: expanded,
                    attrs: d.clone(),
                },
                dwp,
                InProgress::Allowed,
            )
        }
    };

    // Compute attrs uniformly.
    let mut sap_attrs = SapAttrs::default();

    if must_exist == TyAttrValue::Set {
        sap_attrs.pending_never = TyAttrValue::Set;
    } else {
        match default_when_pending {
            DefaultWhenPending::PrependNull => {
                // Prepend null | stream_type
                stream_type = PpirTy::Union {
                    variants: vec![PpirTy::Null { attrs: d }, stream_type],
                    attrs: PpirTypeAttrs::default(),
                };
                sap_attrs.parse_without_null = TyAttrValue::Set;
            }
            DefaultWhenPending::InherentlyNever => {
                sap_attrs.pending_never = TyAttrValue::Set;
            }
            DefaultWhenPending::HasDefault => {
                // Nothing — default is already a member of the type
            }
        }
    }

    if done == TyAttrValue::Set || in_progress == InProgress::NotAllowed {
        sap_attrs.in_progress_never = TyAttrValue::Set;
    }

    (stream_type, sap_attrs)
}

/// Look up block attributes for a `PpirTy` path, using namespace-aware resolution
/// that mirrors `PackageItems::lookup_type`.
pub fn lookup_block_attrs<'a>(path: &[Name], ctx: &'a ExpandCtx<'_>) -> Option<&'a Vec<Name>> {
    if path.is_empty() {
        return None;
    }
    // Mirror PackageItems::lookup_type: try progressively longer namespace prefixes.
    for split in (0..path.len()).rev() {
        let ns_path = &path[..split];
        let item_name = &path[split];
        if let Some(ns) = ctx.package_items.namespaces.get(ns_path) {
            if ns.types.contains_key(item_name) {
                let mut qualified = vec![ctx.package_name.clone()];
                qualified.extend_from_slice(ns_path);
                qualified.push(item_name.clone());
                return ctx.block_attrs.get(&qualified);
            }
        }
    }
    None
}

/// Merge `@@stream.must_exist` / `@@stream.done` block attributes.
fn merge_block_attrs(
    path: &[Name],
    ctx: &ExpandCtx<'_>,
    must_exist: &mut TyAttrValue,
    done: &mut TyAttrValue,
) {
    if let Some(attrs) = lookup_block_attrs(path, ctx) {
        for attr in attrs {
            match attr.as_str() {
                "stream.must_exist" => *must_exist = must_exist.or(TyAttrValue::Set),
                "stream.done" => *done = done.or(TyAttrValue::Set),
                _ => {}
            }
        }
    }
}
