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
    if path.is_empty() {
        return None;
    }
    let item = path.last().unwrap();
    package_items
        .lookup_type(&path[..path.len() - 1], item)
        .and_then(|def| match def {
            Definition::Class(_) => Some(SymbolKind::Class),
            Definition::Enum(_) => Some(SymbolKind::Enum),
            Definition::TypeAlias(_) => Some(SymbolKind::TypeAlias),
            _ => None,
        })
}

/// Classify a type, falling back to `root.*` prefix handling, bare-name
/// namespace lookup, and cross-package lookup.
fn classify_type_cross_pkg(path: &[Name], ctx: &ExpandCtx<'_>) -> Option<SymbolKind> {
    // 1. Try current package directly
    if let Some(kind) = classify_type(ctx.package_items, path) {
        return Some(kind);
    }
    // 2. Handle `root.*` prefix (root namespace of current package)
    if path.len() >= 2 && path[0].as_str() == "root" {
        if let Some(kind) = classify_type(ctx.package_items, &path[1..]) {
            return Some(kind);
        }
    }
    // 3. Bare name in current (non-root) namespace
    if path.len() == 1 && !ctx.namespace_path.is_empty() {
        let mut ns_qualified: Vec<Name> = ctx.namespace_path.to_vec();
        ns_qualified.push(path[0].clone());
        if let Some(kind) = classify_type(ctx.package_items, &ns_qualified) {
            return Some(kind);
        }
    }
    // 4. Try interpreting the first segment as a foreign package name
    if path.len() >= 2 {
        let pkg_name = &path[0];
        let rest = &path[1..];
        if let Some(foreign_items) = ctx.all_package_items.get(pkg_name) {
            return classify_type(foreign_items, rest);
        }
    }
    None
}

// ── Namespace-aware key resolution ──────────────────────────────────────────

/// Resolve a path within a single package to its qualified key
/// `[package_name, ...namespace, item_name]`.
fn resolve_in_package(
    namespace: &[Name],
    item: &Name,
    pkg_name: &Name,
    pkg_items: &PackageItems<'_>,
) -> Option<Vec<Name>> {
    pkg_items.lookup_type(namespace, item).map(|_| {
        let mut key = vec![pkg_name.clone()];
        key.extend_from_slice(namespace);
        key.push(item.clone());
        key
    })
}

/// Resolve a PPIR type path to its qualified key `[package, ...ns, name]`.
/// Handles direct lookup, `root.*` prefix, bare names in non-root namespaces,
/// and cross-package references.
fn resolve_qualified_key(path: &[Name], ctx: &ExpandCtx<'_>) -> Option<Vec<Name>> {
    if path.is_empty() {
        return None;
    }
    let item = path.last().unwrap();
    // 1. Direct lookup in current package
    let ns = &path[..path.len() - 1];
    if let Some(key) = resolve_in_package(ns, item, ctx.package_name, ctx.package_items) {
        return Some(key);
    }
    // 2. Handle `root.*` prefix
    if path.len() >= 2 && path[0].as_str() == "root" {
        let after_root = &path[1..];
        let root_item = after_root.last().unwrap();
        let root_ns = &after_root[..after_root.len() - 1];
        if let Some(key) =
            resolve_in_package(root_ns, root_item, ctx.package_name, ctx.package_items)
        {
            return Some(key);
        }
    }
    // 3. Bare name in current (non-root) namespace
    if path.len() == 1 && !ctx.namespace_path.is_empty() {
        if let Some(key) = resolve_in_package(
            ctx.namespace_path,
            item,
            ctx.package_name,
            ctx.package_items,
        ) {
            return Some(key);
        }
    }
    // 4. Cross-package (first segment = package name)
    if path.len() >= 2 {
        if let Some(foreign_items) = ctx.all_package_items.get(&path[0]) {
            let after_pkg = &path[1..];
            let pkg_item = after_pkg.last().unwrap();
            let pkg_ns = &after_pkg[..after_pkg.len() - 1];
            return resolve_in_package(pkg_ns, pkg_item, &path[0], foreign_items);
        }
    }
    None
}

/// When a type alias in one namespace resolves to a type in a different
/// namespace, the resulting `Named` path (and paths inside unions/lists/etc.)
/// must be qualified so that `lower_type_expr_in_ns` can find them from the
/// caller's namespace. Prepends `root.` to single-segment `Named` paths when
/// the alias namespace differs from the caller namespace.
fn requalify_for_caller(ty: PpirTy, alias_ns: &[Name], caller_ns: &[Name]) -> PpirTy {
    if alias_ns == caller_ns {
        return ty;
    }
    match ty {
        PpirTy::Named {
            path,
            generic_args,
            attrs,
        } if path.len() == 1 && path[0].as_str() != "root" => {
            let mut qualified = Vec::with_capacity(alias_ns.len() + 2);
            qualified.push(SmolStr::from("root"));
            qualified.extend(alias_ns.iter().cloned());
            qualified.extend(path);
            PpirTy::Named {
                path: qualified,
                generic_args: generic_args
                    .into_iter()
                    .map(|ga| requalify_for_caller(ga, alias_ns, caller_ns))
                    .collect(),
                attrs,
            }
        }
        PpirTy::Named {
            path,
            generic_args,
            attrs,
        } => PpirTy::Named {
            path,
            generic_args: generic_args
                .into_iter()
                .map(|ga| requalify_for_caller(ga, alias_ns, caller_ns))
                .collect(),
            attrs,
        },
        PpirTy::Union { variants, attrs } => PpirTy::Union {
            variants: variants
                .into_iter()
                .map(|v| requalify_for_caller(v, alias_ns, caller_ns))
                .collect(),
            attrs,
        },
        PpirTy::List { inner, attrs } => PpirTy::List {
            inner: Box::new(requalify_for_caller(*inner, alias_ns, caller_ns)),
            attrs,
        },
        PpirTy::Optional { inner, attrs } => PpirTy::Optional {
            inner: Box::new(requalify_for_caller(*inner, alias_ns, caller_ns)),
            attrs,
        },
        PpirTy::Map { key, value, attrs } => PpirTy::Map {
            key: Box::new(requalify_for_caller(*key, alias_ns, caller_ns)),
            value: Box::new(requalify_for_caller(*value, alias_ns, caller_ns)),
            attrs,
        },
        other => other,
    }
}

// ── Context ──────────────────────────────────────────────────────────────────

/// Shared context threaded through all stream-expansion functions.
pub struct ExpandCtx<'ctx> {
    pub package_name: &'ctx Name,
    pub namespace_path: &'ctx [Name],
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
                        if let Some(key) = resolve_qualified_key(path, ctx) {
                            if let Some(body) = ctx.alias_bodies.get(&key) {
                                return pending_default(body, ctx, depth + 1);
                            }
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
        PpirTy::Named {
            path, generic_args, ..
        } => {
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
                        generic_args: generic_args
                            .iter()
                            .map(|ga| expand_partial(ga, ctx))
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

    // @stream.done: the entire type stays as-is (no $stream conversion at any depth),
    // because the field won't be populated until streaming completes.
    if done == TyAttrValue::Set {
        let sap_attrs = SapAttrs {
            in_progress_never: TyAttrValue::Set,
            pending_never: if must_exist == TyAttrValue::Set {
                TyAttrValue::Set
            } else {
                TyAttrValue::Unset
            },
            ..SapAttrs::default()
        };
        return (ty.clone_without_attrs(), sap_attrs);
    }

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
        PpirTy::Named {
            path, generic_args, ..
        } => {
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
                        // Thread the original generic args through, so that
                        // `Foo<X>` becomes `Foo$stream<expand_partial(X)>` and
                        // matches the synthesized class's generic arity.
                        let stream_args: Vec<PpirTy> = generic_args
                            .iter()
                            .map(|ga| expand_partial(ga, ctx))
                            .collect();
                        (
                            PpirTy::Named {
                                path: stream_path,
                                generic_args: stream_args,
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
                            if let Some(key) = resolve_qualified_key(path, ctx) {
                                if let Some(body) = ctx.alias_bodies.get(&key) {
                                    // The alias body's paths are relative to the alias
                                    // definition's namespace (key = [pkg, ...ns, name]).
                                    // Recurse with the alias's namespace so that
                                    // classify_type / resolve_qualified_key resolve
                                    // the body's bare names correctly.
                                    let alias_ns = key[1..key.len() - 1].to_vec();
                                    let alias_ctx = ExpandCtx {
                                        namespace_path: &alias_ns,
                                        ..*ctx
                                    };
                                    let mut resolved = body.clone();
                                    resolved.attrs_mut().stream_must_exist = must_exist;
                                    resolved.attrs_mut().stream_done = done;
                                    let (result_ty, sap) =
                                        stream_expand_inner(&resolved, &alias_ctx, depth + 1);
                                    // The result's Named paths are relative to alias_ns.
                                    // If the caller is in a different namespace, qualify
                                    // them so lower_type_expr_in_ns can resolve them.
                                    return (
                                        requalify_for_caller(
                                            result_ty,
                                            &alias_ns,
                                            ctx.namespace_path,
                                        ),
                                        sap,
                                    );
                                }
                            }
                        }
                        // Fallback: treat like class (Name$stream)
                        let (bare_name, prefix) = path.split_last().expect("non-empty path");
                        let stream_path: Vec<Name> = prefix
                            .iter()
                            .cloned()
                            .chain(std::iter::once(SmolStr::new(format!("{bare_name}$stream"))))
                            .collect();
                        let stream_args: Vec<PpirTy> = generic_args
                            .iter()
                            .map(|ga| expand_partial(ga, ctx))
                            .collect();
                        (
                            PpirTy::Named {
                                path: stream_path,
                                generic_args: stream_args,
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

/// Look up block attributes for a `PpirTy` path, using namespace-aware resolution.
pub fn lookup_block_attrs<'a>(path: &[Name], ctx: &'a ExpandCtx<'_>) -> Option<&'a Vec<Name>> {
    resolve_qualified_key(path, ctx).and_then(|key| ctx.block_attrs.get(&key))
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
