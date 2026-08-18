//! Stream expansion algorithm (BEP-006 v12).
//!
//! Implements `stream_expand` and `expand_partial` from the spec.

use std::convert::Infallible;

use baml_base::{Name, attr::TyAttrValue};
use baml_compiler2_hir::{
    contributions::Definition,
    nameres::{self, ForeignLookup, TypePathResolution},
    package::PackageItems,
};
use baml_type::{BuiltinTypeName, PrimitiveType};
use rustc_hash::FxHashMap;
use smol_str::SmolStr;

use crate::ty::{PpirTy, PpirTypeAttrs};

// ── Symbol Classification ────────────────────────────────────────────────────

/// What a `Named` path's stream behavior keys off - derived from ONE
/// [`nameres`] resolution, the same chain type lowering uses, so expansion
/// cannot disagree with the type system about what a name denotes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Class,
    Enum,
    TypeAlias,
}

/// Expansion's [`ForeignLookup`]: plain PRE-expansion tables for every
/// package (expansion runs while producing the post-expansion ones), no
/// interface view. Kind-classification through raw tables agrees with type
/// lowering's interface-checked view on every program that compiles: the
/// exported interface is a subset that never changes an item's kind.
struct ExpandForeign<'x, 'db> {
    all_package_items: &'x FxHashMap<Name, &'x PackageItems<'db>>,
}

impl<'db> ForeignLookup<'db> for ExpandForeign<'_, 'db> {
    type Res = Infallible;

    fn lookup_type(
        &self,
        package: &Name,
        namespace: &[Name],
        item: &Name,
    ) -> Option<TypePathResolution<'db, Infallible>> {
        self.all_package_items
            .get(package)?
            .lookup_type(namespace, item)
            .map(TypePathResolution::Def)
    }

    fn lookup_value(
        &self,
        package: &Name,
        namespace: &[Name],
        item: &Name,
    ) -> Option<Definition<'db>> {
        self.all_package_items
            .get(package)?
            .lookup_value(namespace, item)
    }

    fn baml_shorthand_type(
        &self,
        namespace: &[Name],
        item: &Name,
    ) -> Option<TypePathResolution<'db, Infallible>> {
        self.all_package_items
            .get(&Name::new("baml"))?
            .lookup_type(namespace, item)
            .map(TypePathResolution::Def)
    }

    fn baml_shorthand_value(&self, namespace: &[Name], item: &Name) -> Option<Definition<'db>> {
        self.all_package_items
            .get(&Name::new("baml"))?
            .lookup_value(namespace, item)
    }

    fn is_stream_base(res: &Infallible) -> bool {
        match *res {}
    }
}

/// One resolution of a `Named` path, shared by every consumer in this module:
/// the stream rules key off the KIND, and alias-body/block-attr lookups derive
/// the canonical `[package, namespace.., name]` key from the DEFINITION
/// itself - the same derivation `collect_alias_bodies`/`collect_block_attrs`
/// use - instead of re-walking the written path a second time.
enum ResolvedNamed<'db> {
    /// A builtin-scope hit (`string`, `int`, media, ...). `json` never
    /// surfaces here: it canonicalizes to its stdlib alias definition below.
    Builtin(BuiltinTypeName),
    Def(SymbolKind, Definition<'db>),
}

fn resolve_named<'db>(path: &[Name], ctx: &ExpandCtx<'_, 'db>) -> Option<ResolvedNamed<'db>> {
    let resolver = nameres::Resolver {
        package_items: ctx.package_items,
        ns_context: ctx.namespace_path,
        foreign: ExpandForeign {
            all_package_items: ctx.all_package_items,
        },
    };
    match resolver.resolve_type_path(path)? {
        TypePathResolution::Builtin(builtin) => Some(ResolvedNamed::Builtin(builtin)),
        TypePathResolution::Def(def) => {
            let kind = match def {
                Definition::Class(_) => SymbolKind::Class,
                Definition::Enum(_) => SymbolKind::Enum,
                Definition::TypeAlias(_) => SymbolKind::TypeAlias,
                _ => return None,
            };
            Some(ResolvedNamed::Def(kind, def))
        }
        TypePathResolution::Foreign(never) => match never {},
    }
}

/// The canonical `[package, namespace.., name]` key of a resolved definition,
/// matching the keys `collect_alias_bodies` / `collect_block_attrs` build.
fn def_key<'db>(def: Definition<'db>, name: &Name, ctx: &ExpandCtx<'_, 'db>) -> Vec<Name> {
    let info = baml_compiler2_hir::file_package::file_package(ctx.db, def.file(ctx.db));
    let mut key = vec![info.package.clone()];
    key.extend(info.namespace_path.iter().cloned());
    key.push(name.clone());
    key
}

fn resolve_qualified_key(path: &[Name], ctx: &ExpandCtx<'_, '_>) -> Option<Vec<Name>> {
    match resolve_named(path, ctx)? {
        ResolvedNamed::Def(_, def) => Some(def_key(def, path.last()?, ctx)),
        ResolvedNamed::Builtin(_) => None,
    }
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
            associated_type_bindings,
            attrs,
        } if path.len() == 1
            && path[0].as_str() != "root"
            // A builtin-scope name (`int`, `string`, ...) is namespace-
            // independent by construction - it resolves identically in the
            // alias's namespace and the caller's - so requalifying it would
            // manufacture a nonexistent declaration path (`root.<ns>.int`).
            && nameres::builtin_type_scope(&path[0]).is_none() =>
        {
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
                associated_type_bindings: associated_type_bindings
                    .into_iter()
                    .map(|(name, value)| (name, requalify_for_caller(value, alias_ns, caller_ns)))
                    .collect(),
                attrs,
            }
        }
        PpirTy::Named {
            path,
            generic_args,
            associated_type_bindings,
            attrs,
        } => PpirTy::Named {
            path,
            generic_args: generic_args
                .into_iter()
                .map(|ga| requalify_for_caller(ga, alias_ns, caller_ns))
                .collect(),
            associated_type_bindings: associated_type_bindings
                .into_iter()
                .map(|(name, value)| (name, requalify_for_caller(value, alias_ns, caller_ns)))
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
pub struct ExpandCtx<'ctx, 'db> {
    pub db: &'db dyn crate::Db,
    pub package_name: &'ctx Name,
    pub namespace_path: &'ctx [Name],
    pub package_items: &'ctx PackageItems<'db>,
    /// All packages' items keyed by package name, for cross-package type resolution.
    pub all_package_items: &'ctx FxHashMap<Name, &'ctx PackageItems<'db>>,
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

fn pending_default(ty: &PpirTy, ctx: &ExpandCtx<'_, '_>, depth: u32) -> PendingDefault {
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
            match resolve_named(path, ctx) {
                Some(ResolvedNamed::Def(SymbolKind::TypeAlias, def)) => {
                    if depth < MAX_ALIAS_DEPTH {
                        let key = def_key(def, path.last().expect("non-empty path"), ctx);
                        if let Some(body) = ctx.alias_bodies.get(&key) {
                            return pending_default(body, ctx, depth + 1);
                        }
                    }
                    PendingDefault::Null // fallback if alias body not found or depth exceeded
                }
                // class, enum, builtin, unknown
                _ => PendingDefault::Null,
            }
        }

        // int, float, bool, string, null, optional, T$stream, unknown
        _ => PendingDefault::Null,
    }
}

fn union_pending_default(
    variants: &[PpirTy],
    ctx: &ExpandCtx<'_, '_>,
    depth: u32,
) -> PendingDefault {
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
pub fn expand_partial(ty: &PpirTy, ctx: &ExpandCtx<'_, '_>) -> PpirTy {
    let d = PpirTypeAttrs::default();
    match ty {
        // Primitives/literals/never/opaque/enum → unchanged
        PpirTy::Int { .. }
        | PpirTy::Bigint { .. }
        | PpirTy::Float { .. }
        | PpirTy::String { .. }
        | PpirTy::Bool { .. }
        | PpirTy::Null { .. }
        | PpirTy::Never { .. }
        | PpirTy::Literal { .. }
        | PpirTy::CannotBeStreamed { .. } => ty.clone_without_attrs(),

        // Named types — depends on classification
        PpirTy::Named {
            path,
            generic_args,
            associated_type_bindings,
            ..
        } => {
            // Already *$stream → unchanged
            if path.last().is_some_and(|n| n.as_str().ends_with("$stream")) {
                return ty.clone_without_attrs();
            }
            match resolve_named(path, ctx) {
                Some(ResolvedNamed::Def(SymbolKind::Enum, _)) => ty.clone_without_attrs(),
                Some(ResolvedNamed::Def(SymbolKind::Class | SymbolKind::TypeAlias, _)) => {
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
                        associated_type_bindings: associated_type_bindings
                            .iter()
                            .map(|(name, value)| (name.clone(), expand_partial(value, ctx)))
                            .collect(),
                        attrs: d,
                    }
                }
                // Builtins (primitives) and unresolved names stay unchanged.
                Some(ResolvedNamed::Builtin(_)) | None => ty.clone_without_attrs(),
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

        // Optional → expand_partial(inner) | null
        PpirTy::Optional { inner, .. } => PpirTy::Union {
            variants: vec![
                expand_partial(inner, ctx),
                PpirTy::Null { attrs: d.clone() },
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
pub fn stream_expand(ty: &PpirTy, ctx: &ExpandCtx<'_, '_>) -> (PpirTy, SapAttrs) {
    stream_expand_inner(ty, ctx, 0)
}

/// Max alias resolution depth to prevent infinite loops on cyclic aliases.
const MAX_ALIAS_DEPTH: u32 = 32;

fn stream_expand_inner(ty: &PpirTy, ctx: &ExpandCtx<'_, '_>, depth: u32) -> (PpirTy, SapAttrs) {
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
        PpirTy::Int { .. } | PpirTy::Bigint { .. } | PpirTy::Float { .. } | PpirTy::Bool { .. } => {
            (
                ty.clone_without_attrs(),
                DefaultWhenPending::PrependNull,
                InProgress::NotAllowed,
            )
        }
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
            path,
            generic_args,
            associated_type_bindings,
            ..
        } => {
            // Already *$stream → treat like T$stream
            if path.last().is_some_and(|n| n.as_str().ends_with("$stream")) {
                (
                    ty.clone_without_attrs(),
                    DefaultWhenPending::PrependNull,
                    InProgress::Allowed,
                )
            } else {
                match resolve_named(path, ctx) {
                    // Builtins take the same rules the dedicated PpirTy
                    // variants take above: this arm exists so a builtin
                    // spelled as a PATH (post-intercept-removal, or via the
                    // shared chain's shadowing rules) streams identically to
                    // one lowered as a dedicated node.
                    Some(ResolvedNamed::Builtin(builtin)) => {
                        let primitive = match builtin {
                            BuiltinTypeName::Primitive(primitive) => primitive,
                            // `json` canonicalizes to its alias definition in
                            // resolve_named; intrinsics are never in scope.
                            BuiltinTypeName::Json
                            | BuiltinTypeName::Void
                            | BuiltinTypeName::Never
                            | BuiltinTypeName::Unknown => {
                                unreachable!("not producible by resolve_named")
                            }
                        };
                        match primitive {
                            PrimitiveType::Int
                            | PrimitiveType::Bigint
                            | PrimitiveType::Float
                            | PrimitiveType::Bool => (
                                ty.clone_without_attrs(),
                                DefaultWhenPending::PrependNull,
                                InProgress::NotAllowed,
                            ),
                            PrimitiveType::String => (
                                ty.clone_without_attrs(),
                                DefaultWhenPending::PrependNull,
                                InProgress::Allowed,
                            ),
                            PrimitiveType::Null => (
                                ty.clone_without_attrs(),
                                DefaultWhenPending::HasDefault,
                                InProgress::Allowed,
                            ),
                            // Media and byte buffers cannot be streamed.
                            PrimitiveType::Uint8Array
                            | PrimitiveType::Image
                            | PrimitiveType::Audio
                            | PrimitiveType::Video
                            | PrimitiveType::Pdf => (
                                ty.clone_without_attrs(),
                                DefaultWhenPending::InherentlyNever,
                                InProgress::NotAllowed,
                            ),
                        }
                    }
                    Some(ResolvedNamed::Def(SymbolKind::Enum, _)) => {
                        // Merge @@stream.* block attrs
                        merge_block_attrs(path, ctx, &mut must_exist, &mut done);
                        (
                            ty.clone_without_attrs(),
                            DefaultWhenPending::PrependNull,
                            InProgress::NotAllowed,
                        )
                    }
                    Some(ResolvedNamed::Def(SymbolKind::Class, _)) => {
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
                                associated_type_bindings: associated_type_bindings
                                    .iter()
                                    .map(|(name, value)| (name.clone(), expand_partial(value, ctx)))
                                    .collect(),
                                attrs: d.clone(),
                            },
                            DefaultWhenPending::PrependNull,
                            InProgress::Allowed,
                        )
                    }
                    Some(ResolvedNamed::Def(SymbolKind::TypeAlias, def)) => {
                        // Merge @@stream.* block attrs, then resolve alias recursively
                        merge_block_attrs(path, ctx, &mut must_exist, &mut done);
                        if depth < MAX_ALIAS_DEPTH {
                            let key = def_key(def, path.last().expect("non-empty path"), ctx);
                            {
                                if let Some(body) = ctx.alias_bodies.get(&key) {
                                    // The alias body's paths are relative to the alias
                                    // definition's namespace (key = [pkg, ...ns, name]).
                                    // Recurse with the alias's namespace so that
                                    // resolve_named resolves the body's bare
                                    // names correctly.
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
                                associated_type_bindings: associated_type_bindings
                                    .iter()
                                    .map(|(name, value)| (name.clone(), expand_partial(value, ctx)))
                                    .collect(),
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

        // Optional → expand_partial(inner) | null
        PpirTy::Optional { inner, .. } => (
            PpirTy::Union {
                variants: vec![
                    expand_partial(inner, ctx),
                    PpirTy::Null { attrs: d.clone() },
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
                // Make the field nullable so a not-yet-streamed value can be
                // null. Null goes last (`T | null`) to match the `?` lowering.
                stream_type = PpirTy::Union {
                    variants: vec![stream_type, PpirTy::Null { attrs: d }],
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
pub fn lookup_block_attrs<'a>(path: &[Name], ctx: &'a ExpandCtx<'_, '_>) -> Option<&'a Vec<Name>> {
    resolve_qualified_key(path, ctx).and_then(|key| ctx.block_attrs.get(&key))
}

/// Merge `@@stream.must_exist` / `@@stream.done` block attributes.
fn merge_block_attrs(
    path: &[Name],
    ctx: &ExpandCtx<'_, '_>,
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
