//! `TypeExpr → Ty` lowering using package-level name resolution.

use baml_compiler2_ast::{TypeExpr, TypeExprKind};
use baml_compiler2_hir::{
    contributions::Definition,
    package::{PackageId, PackageItems},
};
use rustc_hash::FxHashSet;

use crate::{
    infer_context::TirTypeError,
    ty::{Freshness, FunctionParamMode, FunctionParamTy, MediaKind, QualifiedTypeName, Ty, TyAttr},
};

fn is_supported_map_key_type(
    db: &dyn crate::Db,
    ty: &Ty,
    seen_aliases: &mut FxHashSet<QualifiedTypeName>,
) -> bool {
    match ty {
        Ty::String { .. }
        | Ty::Never { .. }
        | Ty::Unknown { .. }
        | Ty::Error { .. }
        | Ty::TypeVar(..) => true,
        Ty::Literal(baml_base::Literal::String(_), _, _) => true,
        Ty::Union(members, _) => members
            .iter()
            .all(|member| is_supported_map_key_type(db, member, seen_aliases)),
        Ty::TypeAlias(qtn, _) => {
            if !seen_aliases.insert(qtn.clone()) {
                return false;
            }
            let supported = resolve_type_alias_target(db, qtn)
                .is_some_and(|expanded| is_supported_map_key_type(db, &expanded, seen_aliases));
            seen_aliases.remove(qtn);
            supported
        }
        _ => false,
    }
}

fn resolve_type_alias_target(db: &dyn crate::Db, qtn: &QualifiedTypeName) -> Option<Ty> {
    let pkg_id = PackageId::new(db, qtn.package().clone());
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
    let Some(Definition::TypeAlias(alias_loc)) = pkg_items.lookup_type(qtn.namespace(), qtn.name())
    else {
        return None;
    };
    Some(
        crate::inference::resolve_type_alias(db, alias_loc)
            .ty
            .clone(),
    )
}

/// Resolve an AST `TypeExpr` to a `Ty` using package-level name resolution.
///
/// Names are resolved against `package_items`: classes, enums, and type aliases
/// are looked up in the type namespace. Unresolved names become `Ty::Unknown`
/// and push an `UnresolvedType` diagnostic to `diagnostics`.
/// The package for each resolved type is derived from the **definition's** file,
/// not the referencing file.
pub fn lower_type_expr<'db>(
    db: &'db dyn crate::Db,
    type_expr: &TypeExpr,
    package_items: &PackageItems<'db>,
    generic_params: &[baml_base::Name],
    diagnostics: &mut Vec<TirTypeError>,
) -> Ty {
    lower_type_expr_in_ns(
        db,
        type_expr,
        package_items,
        &[],
        generic_params,
        diagnostics,
    )
}

/// Like [`lower_type_expr`], but resolves unqualified names relative to
/// `ns_context` first (e.g. `["fs"]`), falling back to the package root.
///
/// Use this when lowering type expressions from a function/class signature
/// that lives in a sub-namespace of its package. For example, `File` in
/// `baml/fs.baml` (namespace `["fs"]`) resolves via `lookup_type(&["fs", "File"])`.
/// Public wrapper for `substitute_self` so callers in other crates
/// can pre-resolve `Self` before invoking [`lower_type_expr_in_ns`].
pub fn substitute_self_in(type_expr: &TypeExpr, replacement: &TypeExpr) -> TypeExpr {
    substitute_self(type_expr, replacement)
}

/// Like [`substitute_self_in`], but keeps `Self.Assoc` visible to generic
/// binding substitution. Interface member lowering installs bindings such as
/// `Item -> int`; eagerly rewriting `Self.Item` to `Iterator.Item` would hide
/// that concrete associated type from the lowering pass.
pub fn substitute_self_in_preserving_associated_projections(
    type_expr: &TypeExpr,
    replacement: &TypeExpr,
) -> TypeExpr {
    substitute_self_preserving_associated_projections(type_expr, replacement)
}

fn projection_chain_from_root(
    root: TypeExpr,
    members: &[baml_base::Name],
    attrs: &[baml_compiler2_ast::RawAttribute],
) -> TypeExpr {
    members
        .iter()
        .enumerate()
        .fold(root, |base, (idx, member)| {
            let span = base.span;
            TypeExprKind::AssociatedTypeProjection {
                base: Box::new(base),
                interface: None,
                member: member.clone(),
                attrs: if idx + 1 == members.len() {
                    attrs.to_vec()
                } else {
                    Vec::new()
                },
            }
            .at(span)
        })
}

/// BEP-044 generic bounds: walk `type_expr` and replace any single-
/// segment `Path` whose name appears in `subst` with the matching
/// replacement `TypeExpr`. Used at MIR lowering to substitute bounded
/// type-vars (`T extends Named`) with their bound (`Named`) so the
/// runtime sees a concrete type to dispatch on.
pub fn substitute_paths_in(
    type_expr: &TypeExpr,
    subst: &std::collections::HashMap<baml_base::Name, TypeExpr>,
) -> TypeExpr {
    if subst.is_empty() {
        return type_expr.clone();
    }
    substitute_paths_walk(type_expr, subst)
}

fn substitute_paths_walk(
    ty: &TypeExpr,
    subst: &std::collections::HashMap<baml_base::Name, TypeExpr>,
) -> TypeExpr {
    match &ty.kind {
        TypeExprKind::Path {
            segments,
            generic_args,
            associated_type_bindings,
            attrs,
        } => {
            if generic_args.is_empty()
                && associated_type_bindings.is_empty()
                && let Some(replacement) = subst.get(&segments[0])
            {
                if segments.len() == 1 {
                    return replacement.clone();
                }
                return projection_chain_from_root(replacement.clone(), &segments[1..], attrs);
            }
            TypeExprKind::Path {
                segments: segments.clone(),
                generic_args: generic_args
                    .iter()
                    .map(|a| substitute_paths_walk(a, subst))
                    .collect(),
                associated_type_bindings: associated_type_bindings
                    .iter()
                    .map(|binding| baml_compiler2_ast::AssociatedTypeBinding {
                        name: binding.name.clone(),
                        ty: Box::new(substitute_paths_walk(&binding.ty, subst)),
                    })
                    .collect(),
                attrs: attrs.clone(),
            }
            .at(ty.span)
        }
        TypeExprKind::AssociatedTypeProjection {
            base,
            interface,
            member,
            attrs,
        } => TypeExprKind::AssociatedTypeProjection {
            base: Box::new(substitute_paths_walk(base, subst)),
            interface: interface
                .as_ref()
                .map(|interface| Box::new(substitute_paths_walk(interface, subst))),
            member: member.clone(),
            attrs: attrs.clone(),
        }
        .at(ty.span),
        TypeExprKind::List { inner, attrs } => TypeExprKind::List {
            inner: Box::new(substitute_paths_walk(inner, subst)),
            attrs: attrs.clone(),
        }
        .at(ty.span),
        TypeExprKind::Optional { inner, attrs } => TypeExprKind::Optional {
            inner: Box::new(substitute_paths_walk(inner, subst)),
            attrs: attrs.clone(),
        }
        .at(ty.span),
        TypeExprKind::Map { key, value, attrs } => TypeExprKind::Map {
            key: Box::new(substitute_paths_walk(key, subst)),
            value: Box::new(substitute_paths_walk(value, subst)),
            attrs: attrs.clone(),
        }
        .at(ty.span),
        TypeExprKind::Union { variants, attrs } => TypeExprKind::Union {
            variants: variants
                .iter()
                .map(|v| substitute_paths_walk(v, subst))
                .collect(),
            attrs: attrs.clone(),
        }
        .at(ty.span),
        TypeExprKind::Function {
            params,
            ret,
            throws,
            attrs,
        } => TypeExprKind::Function {
            params: params
                .iter()
                .map(|param| {
                    let mut param = param.clone();
                    param.ty = substitute_paths_walk(&param.ty, subst);
                    param
                })
                .collect(),
            ret: Box::new(substitute_paths_walk(ret, subst)),
            throws: throws
                .as_ref()
                .map(|ty| Box::new(substitute_paths_walk(ty, subst))),
            attrs: attrs.clone(),
        }
        .at(ty.span),
        _ => ty.clone(),
    }
}

/// BEP-044: walk `type_expr` and replace any `Self` reference with
/// `replacement`. Used by signature lowering to pre-resolve `Self` to
/// the enclosing class/interface's type expression before regular
/// resolution runs.
/// The source span of the first `Self` reference appearing anywhere in
/// `type_expr` (a `Path` whose leading segment is `Self`, at any nesting depth),
/// or `None` if it mentions no `Self`.
///
/// `Self` is a signature-only construct: every signature/declaration site
/// resolves it up front with [`substitute_self`] against a replacement it
/// supplies (the enclosing type, the impl receiver, or the interface's `Self`
/// bound). A method *body* has no such replacement — a class body cannot name
/// its own instantiation as a value type, and an interface default method
/// cannot name the implementor — so body-position type lowering uses this to
/// reject `Self` with a purposeful diagnostic instead of leaving it to resolve
/// to a misbehaving type variable (interface default methods) or a bare
/// "unresolved type" (class bodies). Mirrors `substitute_self`'s traversal so
/// no `Self`-bearing position is missed.
pub fn self_reference_span(type_expr: &TypeExpr) -> Option<text_size::TextRange> {
    match &type_expr.kind {
        TypeExprKind::Path {
            segments,
            generic_args,
            associated_type_bindings,
            ..
        } => {
            if segments.first().is_some_and(|s| s.as_str() == "Self") {
                return Some(type_expr.span);
            }
            generic_args
                .iter()
                .find_map(self_reference_span)
                .or_else(|| {
                    associated_type_bindings
                        .iter()
                        .find_map(|binding| self_reference_span(&binding.ty))
                })
        }
        TypeExprKind::AssociatedTypeProjection {
            base, interface, ..
        } => self_reference_span(base).or_else(|| {
            interface
                .as_ref()
                .and_then(|interface| self_reference_span(interface))
        }),
        TypeExprKind::List { inner, .. } | TypeExprKind::Optional { inner, .. } => {
            self_reference_span(inner)
        }
        TypeExprKind::Map { key, value, .. } => {
            self_reference_span(key).or_else(|| self_reference_span(value))
        }
        TypeExprKind::Union { variants, .. } => variants.iter().find_map(self_reference_span),
        TypeExprKind::Function {
            params,
            ret,
            throws,
            ..
        } => params
            .iter()
            .find_map(|param| self_reference_span(&param.ty))
            .or_else(|| self_reference_span(ret))
            .or_else(|| throws.as_ref().and_then(|ty| self_reference_span(ty))),
        _ => None,
    }
}

fn substitute_self(type_expr: &TypeExpr, replacement: &TypeExpr) -> TypeExpr {
    match &type_expr.kind {
        TypeExprKind::Path {
            segments,
            generic_args,
            associated_type_bindings,
            attrs,
        } => {
            if generic_args.is_empty()
                && associated_type_bindings.is_empty()
                && segments[0].as_str() == "Self"
            {
                if segments.len() == 1 {
                    return replacement.clone();
                }
                return projection_chain_from_root(replacement.clone(), &segments[1..], attrs);
            }
            TypeExprKind::Path {
                segments: segments.clone(),
                generic_args: generic_args
                    .iter()
                    .map(|a| substitute_self(a, replacement))
                    .collect(),
                associated_type_bindings: associated_type_bindings
                    .iter()
                    .map(|binding| baml_compiler2_ast::AssociatedTypeBinding {
                        name: binding.name.clone(),
                        ty: Box::new(substitute_self(&binding.ty, replacement)),
                    })
                    .collect(),
                attrs: attrs.clone(),
            }
            .at(type_expr.span)
        }
        TypeExprKind::AssociatedTypeProjection {
            base,
            interface,
            member,
            attrs,
        } => TypeExprKind::AssociatedTypeProjection {
            base: Box::new(substitute_self(base, replacement)),
            interface: interface
                .as_ref()
                .map(|interface| Box::new(substitute_self(interface, replacement))),
            member: member.clone(),
            attrs: attrs.clone(),
        }
        .at(type_expr.span),
        TypeExprKind::List { inner, attrs } => TypeExprKind::List {
            inner: Box::new(substitute_self(inner, replacement)),
            attrs: attrs.clone(),
        }
        .at(type_expr.span),
        TypeExprKind::Optional { inner, attrs } => TypeExprKind::Optional {
            inner: Box::new(substitute_self(inner, replacement)),
            attrs: attrs.clone(),
        }
        .at(type_expr.span),
        TypeExprKind::Map { key, value, attrs } => TypeExprKind::Map {
            key: Box::new(substitute_self(key, replacement)),
            value: Box::new(substitute_self(value, replacement)),
            attrs: attrs.clone(),
        }
        .at(type_expr.span),
        TypeExprKind::Union { variants, attrs } => TypeExprKind::Union {
            variants: variants
                .iter()
                .map(|v| substitute_self(v, replacement))
                .collect(),
            attrs: attrs.clone(),
        }
        .at(type_expr.span),
        TypeExprKind::Function {
            params,
            ret,
            throws,
            attrs,
        } => TypeExprKind::Function {
            params: params
                .iter()
                .map(|param| {
                    let mut param = param.clone();
                    param.ty = substitute_self(&param.ty, replacement);
                    param
                })
                .collect(),
            ret: Box::new(substitute_self(ret, replacement)),
            throws: throws
                .as_ref()
                .map(|ty| Box::new(substitute_self(ty, replacement))),
            attrs: attrs.clone(),
        }
        .at(type_expr.span),
        _ => type_expr.clone(),
    }
}

fn substitute_self_preserving_associated_projections(
    type_expr: &TypeExpr,
    replacement: &TypeExpr,
) -> TypeExpr {
    match &type_expr.kind {
        TypeExprKind::Path {
            segments,
            generic_args,
            associated_type_bindings,
            attrs,
        } => {
            if generic_args.is_empty()
                && associated_type_bindings.is_empty()
                && segments[0].as_str() == "Self"
            {
                if segments.len() == 1 {
                    return replacement.clone();
                }
                return TypeExprKind::Path {
                    segments: segments.clone(),
                    generic_args: Vec::new(),
                    associated_type_bindings: Vec::new(),
                    attrs: attrs.clone(),
                }
                .at(type_expr.span);
            }
            TypeExprKind::Path {
                segments: segments.clone(),
                generic_args: generic_args
                    .iter()
                    .map(|a| substitute_self_preserving_associated_projections(a, replacement))
                    .collect(),
                associated_type_bindings: associated_type_bindings
                    .iter()
                    .map(|binding| baml_compiler2_ast::AssociatedTypeBinding {
                        name: binding.name.clone(),
                        ty: Box::new(substitute_self_preserving_associated_projections(
                            &binding.ty,
                            replacement,
                        )),
                    })
                    .collect(),
                attrs: attrs.clone(),
            }
            .at(type_expr.span)
        }
        TypeExprKind::AssociatedTypeProjection {
            base,
            interface,
            member,
            attrs,
        } => TypeExprKind::AssociatedTypeProjection {
            base: Box::new(substitute_self_preserving_associated_projections(
                base,
                replacement,
            )),
            interface: interface.as_ref().map(|interface| {
                Box::new(substitute_self_preserving_associated_projections(
                    interface,
                    replacement,
                ))
            }),
            member: member.clone(),
            attrs: attrs.clone(),
        }
        .at(type_expr.span),
        TypeExprKind::List { inner, attrs } => TypeExprKind::List {
            inner: Box::new(substitute_self_preserving_associated_projections(
                inner,
                replacement,
            )),
            attrs: attrs.clone(),
        }
        .at(type_expr.span),
        TypeExprKind::Optional { inner, attrs } => TypeExprKind::Optional {
            inner: Box::new(substitute_self_preserving_associated_projections(
                inner,
                replacement,
            )),
            attrs: attrs.clone(),
        }
        .at(type_expr.span),
        TypeExprKind::Map { key, value, attrs } => TypeExprKind::Map {
            key: Box::new(substitute_self_preserving_associated_projections(
                key,
                replacement,
            )),
            value: Box::new(substitute_self_preserving_associated_projections(
                value,
                replacement,
            )),
            attrs: attrs.clone(),
        }
        .at(type_expr.span),
        TypeExprKind::Union { variants, attrs } => TypeExprKind::Union {
            variants: variants
                .iter()
                .map(|v| substitute_self_preserving_associated_projections(v, replacement))
                .collect(),
            attrs: attrs.clone(),
        }
        .at(type_expr.span),
        TypeExprKind::Function {
            params,
            ret,
            throws,
            attrs,
        } => TypeExprKind::Function {
            params: params
                .iter()
                .map(|param| {
                    let mut param = param.clone();
                    param.ty =
                        substitute_self_preserving_associated_projections(&param.ty, replacement);
                    param
                })
                .collect(),
            ret: Box::new(substitute_self_preserving_associated_projections(
                ret,
                replacement,
            )),
            throws: throws.as_ref().map(|ty| {
                Box::new(substitute_self_preserving_associated_projections(
                    ty,
                    replacement,
                ))
            }),
            attrs: attrs.clone(),
        }
        .at(type_expr.span),
        _ => type_expr.clone(),
    }
}

/// Build a `TypeExprKind::Path` referring to a single named type, so
/// `substitute_self` can swap `Self` for the enclosing class /
/// interface name.
pub fn type_expr_for_name(name: baml_base::Name) -> TypeExpr {
    TypeExprKind::Path {
        segments: vec![name],
        generic_args: Vec::new(),
        associated_type_bindings: Vec::new(),
        attrs: Vec::new(),
    }
    .at(text_size::TextRange::default())
}

fn can_be_associated_type_projection_base(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::Class(..)
            | Ty::Interface(..)
            | Ty::TypeAlias(..)
            | Ty::TypeVar(..)
            | Ty::AssociatedTypeProjection { .. }
    )
}

/// Lower an associated-type projection's explicit `as I` qualifier (already
/// lowered to `interface_ty`) into the interface *constraint* it denotes,
/// diagnosing a qualifier that resolves to something other than an interface.
///
/// Shared by both type-expr lowering paths ([`lower_type_expr_in_ns`] and
/// `generics::lower_type_expr_with_generics`) so the qualifier is validated
/// identically regardless of which one runs — the whole point of doing this in
/// the compiler rather than the LSP.
///
/// A `None` qualifier (an unqualified `Base.Member` projection) is accepted
/// today and returns `None`. This is transitional: we intend to always resolve a
/// (possibly parameterized) interface at compile time and drop the `Option`, at
/// which point a missing qualifier becomes its own diagnostic here too.
pub(crate) fn lower_explicit_projection_qualifier(
    db: &dyn crate::Db,
    interface_ty: Option<Ty>,
    member: &baml_base::Name,
    diagnostics: &mut Vec<TirTypeError>,
) -> Option<Box<baml_type::Interface>> {
    let interface_ty = interface_ty?;
    match interface_ty.as_interface() {
        Some(interface) => {
            // Own-only: the explicit qualifier must declare `member` *itself*.
            // Interfaces do not inherit associated types through `requires` (it's
            // a bound, not inheritance), so `(Foo as RequiresIterator).Item` — with
            // `Item` declared on the required `Iterator` — is an error, matching the
            // resolver (which only resolves a member on the exact qualifier).
            if !interface_declares_member(db, &interface.name, member) {
                diagnostics.push(TirTypeError::UnknownAssociatedType {
                    member: member.clone(),
                    container: interface.name.clone(),
                    container_is_interface: true,
                });
            }
            Some(Box::new(interface))
        }
        None => {
            // The qualifier lowered to a non-interface type. An *unresolved*
            // qualifier path already reported `UnresolvedType` (and lowers to
            // `Unknown`/`Error`), so only a qualifier that resolved to a concrete
            // non-interface — a class, alias, etc. — is diagnosed here, avoiding a
            // double report.
            if !matches!(interface_ty, Ty::Unknown { .. } | Ty::Error { .. }) {
                diagnostics.push(TirTypeError::NonInterfaceProjectionQualifier);
            }
            None
        }
    }
}

/// Whether interface `iface_qtn` declares `member` as one of its *own* associated
/// types (not via `requires` — interfaces don't inherit). Returns `true` when the
/// interface can't be resolved, leaving that to the qualifier-path diagnostics.
fn interface_declares_member(
    db: &dyn crate::Db,
    iface_qtn: &QualifiedTypeName,
    member: &baml_base::Name,
) -> bool {
    let pkg_id = baml_compiler2_hir::package::PackageId::new(db, iface_qtn.package().clone());
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
    let Some(baml_compiler2_hir::contributions::Definition::Interface(iface_loc)) =
        pkg_items.lookup_type(iface_qtn.namespace(), iface_qtn.name())
    else {
        return true;
    };
    baml_compiler2_hir::file_item_tree(db, iface_loc.file(db))
        .interfaces
        .get(&iface_loc.id(db))
        .is_some_and(|iface| {
            iface
                .associated_types
                .iter()
                .any(|assoc| assoc.name == *member)
        })
}

pub fn lower_type_expr_in_ns<'db>(
    db: &'db dyn crate::Db,
    type_expr: &TypeExpr,
    package_items: &PackageItems<'db>,
    ns_context: &[baml_base::Name],
    generic_params: &[baml_base::Name],
    diagnostics: &mut Vec<TirTypeError>,
) -> Ty {
    match &type_expr.kind {
        TypeExprKind::Path {
            segments,
            generic_args,
            associated_type_bindings,
            ..
        } => {
            let item = segments.last().expect("non-empty path");
            let seg_ns = &segments[..segments.len() - 1];
            // When we have a namespace context, try the qualified path first.
            // e.g. for ns_context=["fs"], segments=["File"], try namespace ["fs"] item "File".
            let resolved = if !ns_context.is_empty() {
                let ns: Vec<baml_base::Name> =
                    ns_context.iter().chain(seg_ns.iter()).cloned().collect();
                package_items.lookup_type(&ns, item)
            } else {
                package_items.lookup_type(seg_ns, item)
            };
            // Cross-package fallback: if not found in the current package and
            // the first segment is a known package name, look in that package
            // using the remaining segments. This supports synthetic type
            // references like `baml.llm.PromptAst` in companion functions.
            let resolved = resolved.or_else(|| {
                if segments.len() >= 2 {
                    if segments[0].as_str() == "root" {
                        package_items.lookup_type(&segments[1..segments.len() - 1], item)
                    } else {
                        let pkg_id = PackageId::new(db, segments[0].clone());
                        let pkg = baml_compiler2_ppir::package_items(db, pkg_id);
                        pkg.lookup_type(&segments[1..segments.len() - 1], item)
                    }
                } else {
                    None
                }
            });
            // `$stream` companion fallback. PPIR synthesizes a `<T>$stream`
            // class/alias for every class and alias, but those synthetics live
            // only in the per-file item tree — not in the cross-file
            // `package_items` resolution index — so a written `T$stream`
            // reference (e.g. in a function signature) would otherwise erase to
            // `Ty::Unknown`. Resolve the base type; the `Definition::Class` /
            // `TypeAlias` arms below re-qualify it under the original `$stream`
            // name (via `short`), producing the companion's `Class`/`TypeAlias`
            // type. `$` is reserved for synthetics, so this never shadows a
            // user-declared name.
            let resolved = resolved.or_else(|| {
                let base = item.as_str().strip_suffix("$stream")?;
                let base_name = baml_base::Name::new(base);
                let base_def = if !ns_context.is_empty() {
                    let ns: Vec<baml_base::Name> =
                        ns_context.iter().chain(seg_ns.iter()).cloned().collect();
                    package_items.lookup_type(&ns, &base_name)
                } else {
                    package_items.lookup_type(seg_ns, &base_name)
                }
                .or_else(|| {
                    if segments.len() >= 2 {
                        if segments[0].as_str() == "root" {
                            package_items.lookup_type(&segments[1..segments.len() - 1], &base_name)
                        } else {
                            let pkg_id = PackageId::new(db, segments[0].clone());
                            let pkg = baml_compiler2_ppir::package_items(db, pkg_id);
                            pkg.lookup_type(&segments[1..segments.len() - 1], &base_name)
                        }
                    } else {
                        None
                    }
                })?;
                // Only classes and aliases get a `$stream` companion.
                matches!(base_def, Definition::Class(_) | Definition::TypeAlias(_))
                    .then_some(base_def)
            });

            if let Some(def) = resolved {
                let short = segments.last().expect("non-empty path");
                match def {
                    Definition::Class(class_loc) => {
                        // Collect and lower generic args, storing them in Ty::Class.
                        let lowered_args: Vec<Ty> = generic_args
                            .iter()
                            .map(|ga| {
                                lower_type_expr_in_ns(
                                    db,
                                    ga,
                                    package_items,
                                    ns_context,
                                    generic_params,
                                    diagnostics,
                                )
                            })
                            .collect();

                        let class_tree =
                            baml_compiler2_ppir::file_item_tree(db, class_loc.file(db));
                        let expected_type_args = class_tree
                            .classes
                            .get(&class_loc.id(db))
                            .map(|class| class.generic_params.len())
                            .unwrap_or(0);
                        // A bare generic name (`generic_args.is_empty()`) is left
                        // unchecked here: it is a deliberate wildcard in several
                        // positions (`reflect.type_of<Box>()`, construction
                        // `Box { .. }` where args infer from fields, an interface's
                        // own `Self` type). Object construction that cannot infer
                        // its args is reported by `infer_object_expr`
                        // (`CannotInferTypeParameter`) instead.
                        if !generic_args.is_empty() && generic_args.len() != expected_type_args {
                            diagnostics.push(TirTypeError::WrongNumberOfTypeArgs {
                                type_name: short.clone(),
                                expected: expected_type_args,
                                got: generic_args.len(),
                            });
                        }

                        // BEP-034: `baml.future.Future<T, E>` resolves to the
                        // dedicated `Ty::Future` variant rather than the
                        // generic `Ty::Class`. The class declaration exists
                        // as a regular `.baml` file (for method dispatch via
                        // the standard PackageBaml path), but `spawn` /
                        // `await` keys off `Ty::Future` directly. Mirrors
                        // how `int[]` resolves to `Ty::List` even though
                        // `class Array<T>` is a regular class declaration.
                        let qtn = qualify_def(db, def, short);
                        if qtn.name().as_str() == "Future"
                            && qtn.package().as_str() == "baml"
                            && qtn.namespace().len() == 1
                            && qtn.namespace()[0].as_str() == "future"
                            && lowered_args.len() == 2
                        {
                            return Ty::Future(
                                Box::new(lowered_args[0].clone()),
                                Box::new(lowered_args[1].clone()),
                                TyAttr::default(),
                            );
                        }

                        Ty::Class(qtn, lowered_args, TyAttr::default())
                    }
                    Definition::Interface(iface_loc) => {
                        // Same generic-arg handling as `Class` — interface
                        // parameters are valid in the same positions.
                        let lowered_args: Vec<Ty> = generic_args
                            .iter()
                            .map(|ga| {
                                lower_type_expr_in_ns(
                                    db,
                                    ga,
                                    package_items,
                                    ns_context,
                                    generic_params,
                                    diagnostics,
                                )
                            })
                            .collect();
                        let iface_tree =
                            baml_compiler2_ppir::file_item_tree(db, iface_loc.file(db));
                        let expected_type_args = iface_tree
                            .interfaces
                            .get(&iface_loc.id(db))
                            .map(|iface| iface.generic_params.len())
                            .unwrap_or(0);
                        if !generic_args.is_empty() && generic_args.len() != expected_type_args {
                            diagnostics.push(TirTypeError::WrongNumberOfTypeArgs {
                                type_name: short.clone(),
                                expected: expected_type_args,
                                got: generic_args.len(),
                            });
                        }
                        let known_associated_types: FxHashSet<baml_base::Name> = iface_tree
                            .interfaces
                            .get(&iface_loc.id(db))
                            .map(|iface| {
                                iface
                                    .associated_types
                                    .iter()
                                    .map(|assoc| assoc.name.clone())
                                    .collect()
                            })
                            .unwrap_or_default();
                        let mut seen_associated_bindings = FxHashSet::default();
                        let lowered_associated_bindings: Vec<(baml_base::Name, Ty)> =
                            associated_type_bindings
                                .iter()
                                .map(|binding| {
                                    if !known_associated_types.contains(&binding.name) {
                                        diagnostics.push(TirTypeError::UnresolvedType {
                                            name: binding.name.clone(),
                                            suggestions: known_associated_types
                                                .iter()
                                                .map(ToString::to_string)
                                                .collect(),
                                            span: type_expr.span,
                                        });
                                    }
                                    if !seen_associated_bindings.insert(binding.name.clone()) {
                                        diagnostics.push(TirTypeError::TypeMismatch {
                                            expected: Ty::Unknown {
                                                attr: TyAttr::default(),
                                            },
                                            got: Ty::Unknown {
                                                attr: TyAttr::default(),
                                            },
                                        });
                                    }
                                    (
                                        binding.name.clone(),
                                        lower_type_expr_in_ns(
                                            db,
                                            &binding.ty,
                                            package_items,
                                            ns_context,
                                            generic_params,
                                            diagnostics,
                                        ),
                                    )
                                })
                                .collect();
                        Ty::Interface(
                            qualify_def(db, def, short),
                            lowered_args,
                            lowered_associated_bindings,
                            TyAttr::default(),
                        )
                    }
                    Definition::Enum(_) => {
                        // Enums are not generic — validate args and emit a diagnostic if any were supplied.
                        for ga in generic_args {
                            let _ = lower_type_expr_in_ns(
                                db,
                                ga,
                                package_items,
                                ns_context,
                                generic_params,
                                diagnostics,
                            );
                        }
                        if !generic_args.is_empty() {
                            diagnostics.push(TirTypeError::TypeIsNotGeneric {
                                type_name: short.clone(),
                                kind: "enum",
                            });
                        }
                        Ty::Enum(qualify_def(db, def, short), TyAttr::default())
                    }
                    Definition::TypeAlias(_) => {
                        // Type aliases are not generic — validate args and emit a diagnostic if any were supplied.
                        for ga in generic_args {
                            let _ = lower_type_expr_in_ns(
                                db,
                                ga,
                                package_items,
                                ns_context,
                                generic_params,
                                diagnostics,
                            );
                        }
                        if !generic_args.is_empty() {
                            diagnostics.push(TirTypeError::TypeIsNotGeneric {
                                type_name: short.clone(),
                                kind: "type alias",
                            });
                        }
                        Ty::TypeAlias(qualify_def(db, def, short), TyAttr::default())
                    }
                    // Let bindings are values, not types — produce Unknown in a type position.
                    _ => Ty::Unknown {
                        attr: TyAttr::default(),
                    },
                }
            } else {
                // Check if this is a generic type parameter (e.g. T, K, V).
                if segments.len() == 1 {
                    if generic_params.iter().any(|p| *p == segments[0]) {
                        return Ty::TypeVar(segments[0].clone(), TyAttr::default());
                    }
                }
                // Enum-variant fallback: a path like `Status.Active` (or
                // `pkg.ns.Status.Active`) won't resolve as a type — `Active`
                // isn't a type, it's a variant. Try interpreting the last
                // segment as the variant and the rest as the enum's path.
                if segments.len() >= 2 {
                    let (variant, enum_path) = segments.split_last().unwrap();
                    let enum_short = enum_path.last().unwrap();
                    let enum_seg_ns = &enum_path[..enum_path.len() - 1];
                    let enum_resolved = if !ns_context.is_empty() {
                        let ns: Vec<baml_base::Name> = ns_context
                            .iter()
                            .chain(enum_seg_ns.iter())
                            .cloned()
                            .collect();
                        package_items.lookup_type(&ns, enum_short)
                    } else {
                        package_items.lookup_type(enum_seg_ns, enum_short)
                    };
                    let enum_resolved = enum_resolved.or_else(|| {
                        if enum_path.len() >= 2 {
                            if enum_path[0].as_str() == "root" {
                                package_items
                                    .lookup_type(&enum_path[1..enum_path.len() - 1], enum_short)
                            } else {
                                let pkg_id = PackageId::new(db, enum_path[0].clone());
                                let pkg = baml_compiler2_ppir::package_items(db, pkg_id);
                                pkg.lookup_type(&enum_path[1..enum_path.len() - 1], enum_short)
                            }
                        } else {
                            None
                        }
                    });
                    if let Some(def @ Definition::Enum(enum_loc)) = enum_resolved {
                        // Verify the variant actually exists on the enum;
                        // otherwise `Status.Typo` would silently produce a
                        // bogus `Ty::EnumVariant` and downstream code would
                        // never see `UnresolvedType`.
                        let item_tree = baml_compiler2_ppir::file_item_tree(db, enum_loc.file(db));
                        let enum_data = &item_tree[enum_loc.id(db)];
                        if enum_data.variants.iter().any(|v| v.name == *variant) {
                            return Ty::EnumVariant(
                                qualify_def(db, def, enum_short),
                                variant.clone(),
                                TyAttr::default(),
                            );
                        }
                    }
                }
                // Associated type projection fallback: after ordinary type
                // paths and enum variants have had first refusal, treat
                // `Base.Member` as shorthand for an associated type projection.
                // This preserves enum disambiguation (`Status.Active`) and
                // still accepts aliases, type variables, concrete classes,
                // interfaces, and nested projections as projection bases.
                if segments.len() >= 2
                    && generic_args.is_empty()
                    && associated_type_bindings.is_empty()
                {
                    let member = segments.last().expect("non-empty path").clone();
                    let base_expr = TypeExprKind::Path {
                        segments: segments[..segments.len() - 1].to_vec(),
                        generic_args: Vec::new(),
                        associated_type_bindings: Vec::new(),
                        attrs: Vec::new(),
                    }
                    .at(type_expr.span);
                    let mut base_diags = Vec::new();
                    let base_ty = lower_type_expr_in_ns(
                        db,
                        &base_expr,
                        package_items,
                        ns_context,
                        generic_params,
                        &mut base_diags,
                    );
                    if base_diags.is_empty() && can_be_associated_type_projection_base(&base_ty) {
                        return Ty::AssociatedTypeProjection {
                            base: Box::new(base_ty),
                            interface: None,
                            member,
                            attr: TyAttr::default(),
                        };
                    }
                }
                let name_str = segments
                    .iter()
                    .map(smol_str::SmolStr::as_str)
                    .collect::<Vec<_>>()
                    .join(".");
                // Scan all namespaces for the item name to build "did you mean" suggestions.
                // Only do this for single-segment bare names — multi-segment paths already
                // encode the intended namespace.
                let mut suggestions = Vec::new();
                if segments.len() == 1 {
                    for (ns_path, ns_items) in &package_items.namespaces {
                        if ns_items.types.contains_key(item) {
                            if ns_path.is_empty() {
                                suggestions.push(format!("root.{item}"));
                            } else {
                                let ns_str = ns_path
                                    .iter()
                                    .map(smol_str::SmolStr::as_str)
                                    .collect::<Vec<_>>()
                                    .join(".");
                                suggestions.push(format!("root.{ns_str}.{item}"));
                            }
                        }
                    }
                    suggestions.sort();
                }
                diagnostics.push(TirTypeError::UnresolvedType {
                    name: baml_base::Name::new(&name_str),
                    suggestions,
                    span: type_expr.span,
                });
                Ty::Unknown {
                    attr: TyAttr::default(),
                }
            }
        }
        TypeExprKind::Int { .. } => Ty::Int {
            attr: TyAttr::default(),
        },
        TypeExprKind::Bigint { .. } => Ty::Bigint {
            attr: TyAttr::default(),
        },
        TypeExprKind::Float { .. } => Ty::Float {
            attr: TyAttr::default(),
        },
        TypeExprKind::String { .. } => Ty::String {
            attr: TyAttr::default(),
        },
        TypeExprKind::Bool { .. } => Ty::Bool {
            attr: TyAttr::default(),
        },
        TypeExprKind::Null { .. } => Ty::Null {
            attr: TyAttr::default(),
        },
        TypeExprKind::Never { .. } => Ty::Never {
            attr: TyAttr::default(),
        },
        TypeExprKind::Void { .. } => Ty::Void {
            attr: TyAttr::default(),
        },
        TypeExprKind::Uint8Array { .. } => Ty::Uint8Array {
            attr: TyAttr::default(),
        },
        TypeExprKind::Media { kind, .. } => match kind {
            baml_base::MediaKind::Image => Ty::Media(MediaKind::Image, TyAttr::default()),
            baml_base::MediaKind::Audio => Ty::Media(MediaKind::Audio, TyAttr::default()),
            baml_base::MediaKind::Video => Ty::Media(MediaKind::Video, TyAttr::default()),
            baml_base::MediaKind::Pdf => Ty::Media(MediaKind::Pdf, TyAttr::default()),
            // Generic media — treated as unknown for type resolution purposes
            baml_base::MediaKind::Generic => Ty::Unknown {
                attr: TyAttr::default(),
            },
        },
        // `T?` is sugar for `T | null` — lower it directly to a nullable union.
        TypeExprKind::Optional { inner, .. } => Ty::optional(lower_type_expr_in_ns(
            db,
            inner,
            package_items,
            ns_context,
            generic_params,
            diagnostics,
        )),
        TypeExprKind::List { inner, .. } => Ty::List(
            Box::new(lower_type_expr_in_ns(
                db,
                inner,
                package_items,
                ns_context,
                generic_params,
                diagnostics,
            )),
            TyAttr::default(),
        ),
        TypeExprKind::Map { key, value, .. } => {
            let key_ty = lower_type_expr_in_ns(
                db,
                key,
                package_items,
                ns_context,
                generic_params,
                diagnostics,
            );
            let value_ty = lower_type_expr_in_ns(
                db,
                value,
                package_items,
                ns_context,
                generic_params,
                diagnostics,
            );
            let supported_key_type =
                is_supported_map_key_type(db, &key_ty, &mut FxHashSet::default());
            if !supported_key_type {
                diagnostics.push(TirTypeError::InvalidMapKeyType {
                    key: key_ty.clone(),
                });
            }
            Ty::Map {
                key: Box::new(key_ty),
                value: Box::new(value_ty),
                attr: TyAttr::default(),
            }
        }
        TypeExprKind::Union {
            variants: members, ..
        } => Ty::Union(
            members
                .iter()
                .map(|m| {
                    lower_type_expr_in_ns(
                        db,
                        m,
                        package_items,
                        ns_context,
                        generic_params,
                        diagnostics,
                    )
                })
                .collect(),
            TyAttr::default(),
        ),
        TypeExprKind::Function {
            params,
            ret,
            throws,
            ..
        } => {
            // A function type carries no generics of its own; its type variables
            // come from the enclosing context (`generic_params`).
            Ty::Function {
                params: params
                    .iter()
                    .map(|p| FunctionParamTy {
                        name: p.name.clone(),
                        ty: lower_type_expr_in_ns(
                            db,
                            &p.ty,
                            package_items,
                            ns_context,
                            generic_params,
                            diagnostics,
                        ),
                        mode: if p.optional {
                            FunctionParamMode::Optional
                        } else {
                            FunctionParamMode::Required
                        },
                    })
                    .collect(),
                ret: Box::new(lower_type_expr_in_ns(
                    db,
                    ret,
                    package_items,
                    ns_context,
                    generic_params,
                    diagnostics,
                )),
                throws: Box::new(
                    throws
                        .as_deref()
                        .map(|throws| {
                            lower_type_expr_in_ns(
                                db,
                                throws,
                                package_items,
                                ns_context,
                                generic_params,
                                diagnostics,
                            )
                        })
                        .unwrap_or(Ty::Never {
                            attr: TyAttr::default(),
                        }),
                ),
                attr: TyAttr::default(),
            }
        }
        TypeExprKind::AssociatedTypeProjection {
            base,
            interface,
            member,
            ..
        } => {
            let base_ty = lower_type_expr_in_ns(
                db,
                base,
                package_items,
                ns_context,
                generic_params,
                diagnostics,
            );
            let interface_ty = interface.as_ref().map(|interface| {
                lower_type_expr_in_ns(
                    db,
                    interface,
                    package_items,
                    ns_context,
                    generic_params,
                    diagnostics,
                )
            });
            Ty::AssociatedTypeProjection {
                base: Box::new(base_ty),
                interface: lower_explicit_projection_qualifier(
                    db,
                    interface_ty,
                    member,
                    diagnostics,
                ),
                member: member.clone(),
                attr: TyAttr::default(),
            }
        }
        TypeExprKind::Literal { value: lit, .. } => {
            Ty::Literal(lit.clone(), Freshness::Regular, TyAttr::default())
        }
        TypeExprKind::BuiltinUnknown { .. } => Ty::BuiltinUnknown {
            attr: TyAttr::default(),
        },
        TypeExprKind::Error { .. } | TypeExprKind::Unknown { .. } => Ty::Unknown {
            attr: TyAttr::default(),
        },
        // The wildcard `_` lowers to the inference hole, to be filled during
        // checking (a generic-arg slot from its initializer, a `throws` member
        // from the inferred effective throw set).
        TypeExprKind::Infer { .. } => Ty::Infer {
            attr: TyAttr::default(),
        },
        // Dedicated Ty::Type variant — see ty.rs doc comment for design rationale.
        TypeExprKind::Type { .. } => Ty::Type {
            attr: TyAttr::default(),
        },
        // `$rust_type` — opaque Rust-managed state field type.
        TypeExprKind::Rust { .. } => Ty::RustType {
            attr: TyAttr::default(),
        },
    }
}

/// Derive the qualified name for a type from its Definition's file location.
pub fn qualify_def(
    db: &dyn crate::Db,
    def: Definition,
    name: &baml_base::Name,
) -> QualifiedTypeName {
    let file = def.file(db);
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    QualifiedTypeName::new(pkg_info.package, pkg_info.namespace_path, name.clone())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use baml_base::Name;
    use baml_compiler2_ast::{FunctionTypeParam, TextRange, TypeExpr, TypeExprKind};
    use baml_compiler2_hir::package::PackageItems;
    use baml_workspace::Project;
    use rustc_hash::FxHashMap;

    use super::*;

    #[salsa::db]
    #[derive(Clone, Default)]
    struct TestDb {
        storage: salsa::Storage<TestDb>,
        project: Option<Project>,
    }

    impl TestDb {
        fn init(&mut self) {
            self.project = Some(Project::new(self, PathBuf::from("."), Vec::new()));
        }
    }

    #[salsa::db]
    impl salsa::Database for TestDb {}

    #[salsa::db]
    impl baml_workspace::Db for TestDb {
        fn project(&self) -> Project {
            self.project.expect("TestDb not initialized")
        }
    }

    #[salsa::db]
    impl baml_compiler2_hir::Db for TestDb {}

    #[salsa::db]
    impl baml_compiler2_ppir::Db for TestDb {}

    #[salsa::db]
    impl crate::Db for TestDb {}

    fn path(name: &str) -> TypeExpr {
        path_segments(&[name])
    }

    fn path_segments(names: &[&str]) -> TypeExpr {
        TypeExprKind::Path {
            segments: names.iter().map(|name| Name::new(*name)).collect(),
            generic_args: vec![],
            associated_type_bindings: vec![],
            attrs: vec![],
        }
        .at(TextRange::default())
    }

    #[test]
    fn substitute_paths_recurses_into_function_type() {
        let type_expr = TypeExprKind::Function {
            params: vec![FunctionTypeParam {
                name: Some(Name::new("value")),
                optional: false,
                ty: path("T"),
            }],
            ret: Box::new(path("T")),
            throws: Some(Box::new(path("E"))),
            attrs: vec![],
        }
        .at(TextRange::default());
        let mut subst = std::collections::HashMap::new();
        subst.insert(
            Name::new("T"),
            TypeExprKind::String { attrs: vec![] }.at(TextRange::default()),
        );
        subst.insert(
            Name::new("E"),
            TypeExprKind::Bool { attrs: vec![] }.at(TextRange::default()),
        );

        let TypeExprKind::Function {
            params,
            ret,
            throws,
            ..
        } = substitute_paths_in(&type_expr, &subst).kind
        else {
            panic!("expected function type");
        };

        assert!(matches!(params[0].ty.kind, TypeExprKind::String { .. }));
        assert!(matches!(ret.kind, TypeExprKind::String { .. }));
        assert!(matches!(
            throws.as_ref().map(|t| &t.kind),
            Some(TypeExprKind::Bool { .. })
        ));
    }

    #[test]
    fn substitute_paths_rewrites_substituted_root_member_path_to_projection() {
        let type_expr = path_segments(&["T", "Item"]);
        let replacement = path("Box");
        let mut subst = std::collections::HashMap::new();
        subst.insert(Name::new("T"), replacement.clone());

        let TypeExprKind::AssociatedTypeProjection {
            base,
            interface,
            member,
            ..
        } = substitute_paths_in(&type_expr, &subst).kind
        else {
            panic!("expected associated type projection");
        };

        assert_eq!(*base, replacement);
        assert!(interface.is_none());
        assert_eq!(member, Name::new("Item"));
    }

    #[test]
    fn substitute_self_recurses_into_function_type() {
        let type_expr = TypeExprKind::Function {
            params: vec![FunctionTypeParam {
                name: Some(Name::new("value")),
                optional: false,
                ty: path("Self"),
            }],
            ret: Box::new(path("Self")),
            throws: Some(Box::new(path("Self"))),
            attrs: vec![],
        }
        .at(TextRange::default());
        let replacement = TypeExprKind::Path {
            segments: vec![Name::new("Named")],
            generic_args: vec![TypeExprKind::Int { attrs: vec![] }.at(TextRange::default())],
            associated_type_bindings: vec![],
            attrs: vec![],
        }
        .at(TextRange::default());

        let TypeExprKind::Function {
            params,
            ret,
            throws,
            ..
        } = substitute_self_in(&type_expr, &replacement).kind
        else {
            panic!("expected function type");
        };

        assert_eq!(params[0].ty, replacement);
        assert_eq!(*ret, replacement);
        assert_eq!(throws.map(|ty| *ty), Some(replacement));
    }

    #[test]
    fn substitute_self_rewrites_member_path_to_projection() {
        let type_expr = path_segments(&["Self", "Record"]);
        let replacement = path("UserRepository");

        let TypeExprKind::AssociatedTypeProjection {
            base,
            interface,
            member,
            ..
        } = substitute_self_in(&type_expr, &replacement).kind
        else {
            panic!("expected associated type projection");
        };

        assert_eq!(*base, replacement);
        assert!(interface.is_none());
        assert_eq!(member, Name::new("Record"));
    }

    #[test]
    fn lower_function_type_preserves_parameter_optionality() {
        let mut db = TestDb::default();
        db.init();
        let package_items = PackageItems {
            namespaces: FxHashMap::default(),
            extra: None,
        };
        let type_expr = TypeExprKind::Function {
            params: vec![
                FunctionTypeParam {
                    name: Some(Name::new("query")),
                    optional: false,
                    ty: TypeExprKind::String { attrs: vec![] }.at(TextRange::default()),
                },
                FunctionTypeParam {
                    name: Some(Name::new("limit")),
                    optional: true,
                    ty: TypeExprKind::Int { attrs: vec![] }.at(TextRange::default()),
                },
            ],
            ret: Box::new(TypeExprKind::Bool { attrs: vec![] }.at(TextRange::default())),
            throws: None,
            attrs: vec![],
        }
        .at(TextRange::default());
        let mut diagnostics = Vec::new();

        let ty = lower_type_expr_in_ns(&db, &type_expr, &package_items, &[], &[], &mut diagnostics);

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {diagnostics:?}"
        );
        let Ty::Function { params, .. } = ty else {
            panic!("expected function type, got {ty:?}");
        };
        let params: Vec<&FunctionParamTy> = params.iter().collect();

        assert_eq!(params[0].name.as_deref(), Some("query"));
        assert_eq!(params[0].mode, FunctionParamMode::Required);
        assert_eq!(params[1].name.as_deref(), Some("limit"));
        assert_eq!(params[1].mode, FunctionParamMode::Optional);
    }

    #[test]
    fn explicit_projection_qualifier_accepts_interface() {
        let mut db = TestDb::default();
        db.init();
        let mut diags = Vec::new();
        let iface = Ty::Interface(
            QualifiedTypeName::from_dotted_path("user.Iterator"),
            Vec::new(),
            Vec::new(),
            TyAttr::default(),
        );
        // The interface isn't defined in this bare db, so the own-member check
        // can't resolve it and conservatively accepts (no spurious member error).
        // Real member-checking is covered by the `interfaces_associated_types`
        // integration tests.
        let member = Name::new("Item");
        assert!(
            lower_explicit_projection_qualifier(&db, Some(iface), &member, &mut diags).is_some()
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn explicit_projection_qualifier_rejects_non_interface() {
        let db = TestDb::default();
        let mut diags = Vec::new();
        let class = Ty::Class(
            QualifiedTypeName::from_dotted_path("user.Foo"),
            Vec::new(),
            TyAttr::default(),
        );
        let member = Name::new("Item");
        assert!(
            lower_explicit_projection_qualifier(&db, Some(class), &member, &mut diags).is_none(),
            "a class qualifier must not yield an interface"
        );
        assert!(
            matches!(
                diags.as_slice(),
                [TirTypeError::NonInterfaceProjectionQualifier]
            ),
            "expected NonInterfaceProjectionQualifier, got {diags:?}"
        );
    }

    #[test]
    fn explicit_projection_qualifier_does_not_double_report_already_erroneous() {
        // An unresolved qualifier path already emitted `UnresolvedType` and lowers
        // to `Unknown`, so the helper must not add a second diagnostic on top.
        let db = TestDb::default();
        let mut diags = Vec::new();
        let member = Name::new("Item");
        assert!(
            lower_explicit_projection_qualifier(
                &db,
                Some(Ty::Unknown {
                    attr: TyAttr::default()
                }),
                &member,
                &mut diags,
            )
            .is_none()
        );
        assert!(
            diags.is_empty(),
            "an already-erroneous qualifier must not double-report, got {diags:?}"
        );
    }

    #[test]
    fn explicit_projection_qualifier_none_is_unqualified() {
        let db = TestDb::default();
        let mut diags = Vec::new();
        let member = Name::new("Item");
        assert!(lower_explicit_projection_qualifier(&db, None, &member, &mut diags).is_none());
        assert!(diags.is_empty());
    }
}
