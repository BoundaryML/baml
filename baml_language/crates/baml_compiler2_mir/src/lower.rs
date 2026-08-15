use std::collections::{HashMap, HashSet};

use baml_base::{Name, TypePath};
use baml_type::{
    ParamTy, PrimitiveType, RealizedTy, ResolvedAliases, RuntimeGenericLayout, RuntimeTy, TyAttr,
    TyTemplate, TyTemplateInterface, TypeName,
};
use indexmap::IndexMap;

use crate::{
    builder::MirBuilder,
    ir::{
        AggregateKind, BasicBlock, BinOp, BlockId, CatchRegion, Constant, IndexKind, IntrinsicOp,
        ItemRef, Local, LocalDecl, LogLevel, MirFunction, MirFunctionBody, MirFunctionKind,
        Operand, Place, Rvalue, StatementKind, Terminator,
    },
    optimize,
};

/// Classifies what kind of switch a match/catch expression lowers to.
///
/// `Integer` and `EnumDiscriminant` are currently implemented.
/// `TypeTag` dispatches class-type and primitive-type match arms via runtime
/// type tags, using `Rvalue::TypeTag` for the switch operand.
enum SwitchKind {
    Integer,
    EnumDiscriminant(QualifiedTypeName),
    TypeTag,
}

/// What happens in the otherwise block of a switch.
#[derive(Clone, Copy)]
enum SwitchOtherwise {
    /// Match expression: goto join (non-exhaustive) or unreachable (exhaustive).
    Match { is_exhaustive: bool },
    /// Catch expression: rethrow unmatched errors.
    /// If `needs_throw_if_panic` is true, insert a `throw_if_panic` guard before wildcard body.
    Catch {
        error_local: Local,
        needs_throw_if_panic: bool,
    },
}

struct LoopContext {
    break_target: BlockId,
    continue_target: BlockId,
    /// Depth of `defer_stack` at loop entry (BEP-042). `break`/`continue`
    /// replay the defers declared inside the loop body so far (down to this
    /// depth) before jumping.
    defer_depth: usize,
}

#[derive(Clone, Copy)]
struct CatchContext {
    unwind_target: BlockId,
    error_local: Local,
}

// ─── Type conversion: TIR RuntimeTy → baml_type::RuntimeTy ────────────────────────────────

use baml_type::{
    FunctionParamMode, FunctionParamTy as Tir2FunctionParamTy, QualifiedTypeName, Ty as Tir2Ty,
};

/// Build the [`ResolvedAliases`] type-alias environment for a package,
/// including dependency packages. The pure erasure that consumes it lives in
/// `baml_type` ([`ResolvedAliases::convert`]), wrapped compiler-side by
/// `convert_tir_ty_for_runtime`; only this db-querying constructor stays
/// compiler-side.
pub fn resolved_aliases_for_package(
    db: &dyn crate::Db,
    pkg_id: baml_compiler2_hir::package::PackageId,
) -> ResolvedAliases {
    use baml_compiler2_hir::package::{package_dependencies, package_items};

    let pkg_items = package_items(db, pkg_id);
    let mut aliases = collect_type_aliases(db, pkg_items);
    for &dep_id in package_dependencies(db, pkg_id) {
        aliases.extend(collect_type_aliases(db, package_items(db, dep_id)));
    }
    ResolvedAliases::from_aliases(aliases)
}

/// Every type alias a package declares, resolved to its (one-level) value
/// through `hir_ty`'s lowering, keyed by qualified name.
fn collect_type_aliases<'db>(
    db: &'db dyn crate::Db,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
) -> HashMap<QualifiedTypeName, Tir2Ty> {
    use baml_compiler2_hir::contributions::Definition;
    let mut aliases = HashMap::new();
    for ns in pkg_items.namespaces.values() {
        for (name, def) in &ns.types {
            if let Definition::TypeAlias(loc) = def {
                let value = baml_compiler2_hir_ty::lower::type_alias_value(db, *loc).to_plain();
                aliases.insert(qualify_def(db, Definition::TypeAlias(*loc), name), value);
            }
        }
    }
    aliases
}

// ─── RuntimeTy → TyTemplate conversion for already-resolved RuntimeTy values ──────────────

/// Lower a type expression, treating the `bindings` map's keys as the in-scope type variables,
/// then substitute those bindings. Threads the in-scope typevar `bounds` so a `T.member`
/// projection resolves through `T`'s bound rather than erasing to `unknown`. Replaces the
/// removed `lower_type_expr_with_generics` (a bare lower-then-substitute that dropped bounds);
/// `bounds` is required, so bound-erasure is unrepresentable at the call sites.
fn lower_ty_with_bindings<'db>(
    db: &'db dyn crate::Db,
    expr: &baml_compiler2_ast::TypeExpr,
    pkg_items: &'db baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
    bindings: &FxHashMap<ParamTy, Tir2Ty>,
    bounds: &FxHashMap<ParamTy, Vec<baml_type::interned::InterfaceRef>>,
) -> Tir2Ty {
    // The AST node lowers through hir's firewall into a scratch store,
    // then through hir_ty's ONE type-lowering road (S16: MIR's
    // declaration-site lowering re-pointed off TIR). Errors are the
    // sentinel in the lowered type, not a diag side-channel (S17's).
    let mut builder = baml_compiler2_hir::type_ref::TypeRefBuilder::new();
    let id = builder.lower(expr);
    let (store, _spans) = builder.finish();
    lower_ref_with_bindings(db, &store, id, pkg_items, namespace_path, bindings, bounds)
}

/// The `TypeRef`-arena twin of [`lower_ty_with_bindings`], for callers holding
/// firewall data (`*_data(…).type_refs` + a `TypeRefId`) rather than an AST node.
/// Identical behavior — lowers through `lower_type_ref` instead of `lower_type_expr`.
fn lower_ref_with_bindings<'db>(
    db: &'db dyn crate::Db,
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
    id: baml_compiler2_hir::type_ref::TypeRefId,
    pkg_items: &'db baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
    bindings: &FxHashMap<ParamTy, Tir2Ty>,
    bounds: &FxHashMap<ParamTy, Vec<baml_type::interned::InterfaceRef>>,
) -> Tir2Ty {
    let generic_params: Vec<ParamTy> = bindings.keys().cloned().collect();
    let ctx =
        baml_compiler2_hir_ty::lower::lower_ctx_for_package(db, pkg_items, namespace_path.to_vec())
            .with_frame(generic_params)
            .with_bounds(bounds.clone());
    let lowered = ctx.lower_type_ref(store, id).to_plain();
    baml_type_runtime::substitute_ty(&lowered, bindings)
}

/// The IN-SCOPE twins of the binding wrappers above: explicit frame, no
/// substitution - the same `hir_ty` road for sites that keep their type
/// variables rigid (turbofish args, annotations, dispatch targets).
#[allow(clippy::too_many_arguments)]
fn lower_ref_in_scope<'db>(
    db: &'db dyn crate::Db,
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
    id: baml_compiler2_hir::type_ref::TypeRefId,
    pkg_items: &'db baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
    generic_params: &[ParamTy],
    bounds: &FxHashMap<ParamTy, Vec<baml_type::interned::InterfaceRef>>,
    self_ty: Option<Tir2Ty>,
) -> Tir2Ty {
    lower_ref_in_scope_at(
        db,
        store,
        id,
        pkg_items,
        namespace_path,
        generic_params,
        bounds,
        self_ty,
        baml_compiler2_hir_ty::lower::TypePosition::Existential,
    )
}

/// [`lower_ref_in_scope`] at an explicit position - for `implements` /
/// dispatch targets, which are constraint heads (written pins only, no
/// existential completeness demands).
#[allow(clippy::too_many_arguments)]
fn lower_ref_in_scope_at<'db>(
    db: &'db dyn crate::Db,
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
    id: baml_compiler2_hir::type_ref::TypeRefId,
    pkg_items: &'db baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
    generic_params: &[ParamTy],
    bounds: &FxHashMap<ParamTy, Vec<baml_type::interned::InterfaceRef>>,
    self_ty: Option<Tir2Ty>,
    position: baml_compiler2_hir_ty::lower::TypePosition,
) -> Tir2Ty {
    let ctx =
        baml_compiler2_hir_ty::lower::lower_ctx_for_package(db, pkg_items, namespace_path.to_vec())
            .with_frame(generic_params.to_vec())
            .with_bounds(bounds.clone())
            .with_self_ty(self_ty.map(|ty| baml_type::interned::Ty::from_plain(&ty)));
    ctx.lower_type_ref_at(store, id, position).to_plain()
}

/// The AST twin of [`lower_ref_in_scope`] (scratch firewall store).
#[allow(clippy::too_many_arguments)]
fn lower_expr_in_scope<'db>(
    db: &'db dyn crate::Db,
    expr: &baml_compiler2_ast::TypeExpr,
    pkg_items: &'db baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
    generic_params: &[ParamTy],
    bounds: &FxHashMap<ParamTy, Vec<baml_type::interned::InterfaceRef>>,
    self_ty: Option<Tir2Ty>,
) -> Tir2Ty {
    let mut builder = baml_compiler2_hir::type_ref::TypeRefBuilder::new();
    let id = builder.lower(expr);
    let (store, _spans) = builder.finish();
    lower_ref_in_scope(
        db,
        &store,
        id,
        pkg_items,
        namespace_path,
        generic_params,
        bounds,
        self_ty,
    )
}

/// The transitive `requires` closure of an interface, as declaration
/// locations (BFS from the root, the root first) - resolution through
/// the same hir road every written interface reference takes.
fn interface_requires_closure_locs<'db>(
    db: &'db dyn crate::Db,
    root: baml_compiler2_hir::loc::InterfaceLoc<'db>,
) -> Vec<baml_compiler2_hir::loc::InterfaceLoc<'db>> {
    let mut out = Vec::new();
    let mut seen: FxHashSet<baml_compiler2_hir::loc::InterfaceLoc<'db>> = FxHashSet::default();
    let mut queue = std::collections::VecDeque::from([root]);
    while let Some(loc) = queue.pop_front() {
        if !seen.insert(loc) {
            continue;
        }
        out.push(loc);
        let iface = baml_compiler2_ppir::item_data::interface_data(db, loc);
        let pkg = baml_compiler2_hir::file_package::file_package(db, loc.file(db));
        let pkg_items = baml_compiler2_ppir::package_items(
            db,
            baml_compiler2_hir::package::PackageId::new(db, pkg.package.clone()),
        );
        for &parent in &iface.requires {
            if let Some(parent_loc) = resolve_ref_to_interface_loc(
                db,
                &iface.type_refs,
                parent,
                pkg_items,
                &pkg.namespace_path,
            ) {
                queue.push_back(parent_loc);
            }
        }
    }
    out
}

/// Literals widened to their bases REGARDLESS of freshness - impl
/// dispatch is by base type (`hir_ty`'s operand-dispatch discipline; a
/// `"x"`-typed receiver IS a string at runtime).
fn widen_literal_bases(ty: &Tir2Ty) -> Tir2Ty {
    match ty {
        Tir2Ty::Literal(lit, _, attr) => {
            Tir2Ty::from_primitive(baml_type::PrimitiveType::from_literal(lit), attr.clone())
        }
        Tir2Ty::Union(members, attr) => Tir2Ty::Union(
            members.iter().map(widen_literal_bases).collect(),
            attr.clone(),
        ),
        Tir2Ty::List(inner, attr) => {
            Tir2Ty::List(Box::new(widen_literal_bases(inner)), attr.clone())
        }
        _ => ty.clone(),
    }
}

/// A `recv.NAME(..)` / `Prefix.NAME(..)` callee shape - the desugar
/// trigger the engines and this lowering share.
fn is_sugar_callee(expr: &baml_compiler2_ast::Expr, name: &str) -> bool {
    match expr {
        baml_compiler2_ast::Expr::MemberAccess { member, .. } => member.as_str() == name,
        baml_compiler2_ast::Expr::Path(segments) => {
            segments.len() >= 2
                && segments
                    .last()
                    .is_some_and(|segment| segment.as_str() == name)
        }
        _ => false,
    }
}

/// Resolve a written interface reference to its declaration: the `hir_ty`
/// lowering road (written pins only) plus the facts definition lookup.
fn resolve_ref_to_interface_loc<'db>(
    db: &'db dyn crate::Db,
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
    target: baml_compiler2_hir::type_ref::TypeRefId,
    pkg_items: &'db baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
) -> Option<baml_compiler2_hir::loc::InterfaceLoc<'db>> {
    let lowered = lower_ref_in_scope_at(
        db,
        store,
        target,
        pkg_items,
        namespace_path,
        &[],
        &FxHashMap::default(),
        None,
        baml_compiler2_hir_ty::lower::TypePosition::ConstraintHead,
    );
    let Tir2Ty::Interface(qtn, ..) = lowered else {
        return None;
    };
    match baml_compiler2_hir_ty::facts::Facts::new(db).definition_of(&qtn) {
        Some(baml_compiler2_hir::contributions::Definition::Interface(loc)) => Some(loc),
        _ => None,
    }
}

/// A definition's fully qualified name - its declaring file's package
/// and namespace plus the short name (the spelling the runtime tags
/// use).
fn qualify_def(
    db: &dyn crate::Db,
    def: baml_compiler2_hir::contributions::Definition<'_>,
    name: &Name,
) -> QualifiedTypeName {
    let file = def.file(db);
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    QualifiedTypeName::new(pkg_info.package, pkg_info.namespace_path, name.clone())
}

/// An interned interface bound as the plain interface TYPE - the
/// dispatch view MIR's per-param bound map holds.
fn plain_interface_ty(bound: &baml_type::interned::InterfaceRef) -> Tir2Ty {
    baml_type::interned::Ty::intern(baml_type::interned::TyKind::Interface(
        bound.name.clone(),
        bound.generics.clone(),
        bound.associated_types.clone(),
        baml_type::TyAttr::default(),
    ))
    .to_plain()
}

/// Whether `ty` contains an associated-type projection node at any depth.
///
/// The companion of [`baml_type_runtime::contains_typevar`] for the
/// other symbolic node kind: `contains_typevar` sees a projection only through
/// its base/interface *type variables*, so a projection whose base is already
/// concrete (`(int as Foo).Assoc` left unresolved by inference) slips past it.
/// Template lowering must route either symbolic kind through the structured
/// template arms rather than asserting realizedness.
fn tir_contains_projection(ty: &Tir2Ty) -> bool {
    match ty {
        Tir2Ty::AssociatedTypeProjection { .. } => true,
        Tir2Ty::List(inner, _) | Tir2Ty::EvolvingList(inner, _) => tir_contains_projection(inner),
        Tir2Ty::Map {
            key: k, value: v, ..
        }
        | Tir2Ty::EvolvingMap(k, v, _) => tir_contains_projection(k) || tir_contains_projection(v),
        Tir2Ty::Union(members, _) => members.iter().any(tir_contains_projection),
        Tir2Ty::Class(_, type_args, _) => type_args.iter().any(tir_contains_projection),
        Tir2Ty::Interface(_, type_args, associated_bindings, _) => {
            type_args.iter().any(tir_contains_projection)
                || associated_bindings
                    .iter()
                    .any(|(_, ty)| tir_contains_projection(ty))
        }
        Tir2Ty::Future(value, error, _) => {
            tir_contains_projection(value) || tir_contains_projection(error)
        }
        Tir2Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            params
                .iter()
                .any(|param| tir_contains_projection(&param.ty))
                || tir_contains_projection(ret)
                || tir_contains_projection(throws)
        }
        _ => false,
    }
}

/// Whether `ty` contains any symbolic node — a type variable or an
/// associated-type projection — that has no realized runtime form and must be
/// lowered through the structured template arms.
fn tir_contains_symbolic(ty: &Tir2Ty) -> bool {
    baml_type_runtime::contains_typevar(ty) || tir_contains_projection(ty)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TemplateMode {
    Value,
    Pattern,
}

fn lower_tir_template(
    ty: &Tir2Ty,
    resolved: &ResolvedAliases,
    generic_layout: &RuntimeGenericLayout,
    mode: TemplateMode,
) -> Option<TyTemplate> {
    match ty {
        Tir2Ty::AssociatedTypeProjection {
            base,
            interface,
            member,
            ..
        } => {
            if matches!(&**base, Tir2Ty::TypeVar(param, _) if param.as_str() == "Self")
                && let Some(index) = generic_layout.slot_by_name(member)
            {
                return Some(TyTemplate::TypeArgRef(index));
            }
            Some(TyTemplate::AssociatedTypeProjection {
                base: Box::new(lower_tir_template(base, resolved, generic_layout, mode)?),
                interface: Box::new(baml_type::TyTemplateInterface {
                    name: interface.name.clone(),
                    generics: interface
                        .generics
                        .iter()
                        .map(|ty| lower_tir_template(ty, resolved, generic_layout, mode))
                        .collect::<Option<Vec<_>>>()?,
                    associated_types: interface
                        .associated_types
                        .iter()
                        .map(|(name, ty)| {
                            Some((
                                name.clone(),
                                lower_tir_template(ty, resolved, generic_layout, mode)?,
                            ))
                        })
                        .collect::<Option<Vec<_>>>()?,
                }),
                member: member.clone(),
                attr: TyAttr::default(),
            })
        }
        Tir2Ty::TypeVar(param, _) => {
            if let Some(index) = generic_layout.slot(param) {
                Some(TyTemplate::TypeArgRef(index))
            } else if mode == TemplateMode::Value {
                unreachable!("type variable not found in type args: {param}")
            } else {
                None
            }
        }
        Tir2Ty::List(inner, _) | Tir2Ty::EvolvingList(inner, _) => Some(TyTemplate::list(
            lower_tir_template(inner, resolved, generic_layout, mode)?,
        )),
        Tir2Ty::Map {
            key: k, value: v, ..
        }
        | Tir2Ty::EvolvingMap(k, v, _) => Some(TyTemplate::map(
            lower_tir_template(k, resolved, generic_layout, mode)?,
            lower_tir_template(v, resolved, generic_layout, mode)?,
        )),
        Tir2Ty::Union(parts, _) => Some(TyTemplate::union(
            parts
                .iter()
                .map(|ty| lower_tir_template(ty, resolved, generic_layout, mode))
                .collect::<Option<Vec<_>>>()?,
        )),
        Tir2Ty::Class(qtn, type_args, attr) => {
            if mode == TemplateMode::Pattern || type_args.iter().any(tir_contains_symbolic) {
                Some(TyTemplate::class(
                    qtn.clone(),
                    type_args
                        .iter()
                        .map(|ty| lower_tir_template(ty, resolved, generic_layout, mode))
                        .collect::<Option<Vec<_>>>()?,
                ))
            } else {
                let resolved_args: Vec<RuntimeTy> =
                    type_args.iter().map(|ty| resolved.convert(ty)).collect();
                Some(realized_leaf_template(&RuntimeTy::Class(
                    qtn.clone(),
                    resolved_args,
                    attr.clone(),
                )))
            }
        }
        Tir2Ty::Interface(qtn, type_args, associated_bindings, attr) => {
            if mode == TemplateMode::Pattern
                || type_args.iter().any(tir_contains_symbolic)
                || associated_bindings
                    .iter()
                    .any(|(_, ty)| tir_contains_symbolic(ty))
            {
                Some(TyTemplate::interface(
                    qtn.clone(),
                    type_args
                        .iter()
                        .map(|ty| lower_tir_template(ty, resolved, generic_layout, mode))
                        .collect::<Option<Vec<_>>>()?,
                    associated_bindings
                        .iter()
                        .map(|(name, ty)| {
                            Some((
                                name.clone(),
                                lower_tir_template(ty, resolved, generic_layout, mode)?,
                            ))
                        })
                        .collect::<Option<Vec<_>>>()?,
                ))
            } else {
                let resolved_args: Vec<RuntimeTy> =
                    type_args.iter().map(|ty| resolved.convert(ty)).collect();
                let resolved_bindings = associated_bindings
                    .iter()
                    .map(|(name, ty)| (name.clone(), resolved.convert(ty)))
                    .collect();
                Some(realized_leaf_template(&RuntimeTy::Interface(
                    qtn.clone(),
                    resolved_args,
                    resolved_bindings,
                    attr.clone(),
                )))
            }
        }
        Tir2Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            let mode = TemplateMode::Value;
            Some(TyTemplate::Function {
                params: params
                    .iter()
                    .map(|param| {
                        Some(baml_type::TyTemplateFunctionParamTy {
                            name: param.name.clone(),
                            ty: lower_tir_template(&param.ty, resolved, generic_layout, mode)?,
                            mode: param.mode,
                        })
                    })
                    .collect::<Option<Vec<_>>>()?,
                ret: Box::new(lower_tir_template(ret, resolved, generic_layout, mode)?),
                throws: Box::new(lower_tir_template(throws, resolved, generic_layout, mode)?),
                attr: TyAttr::default(),
            })
        }
        Tir2Ty::Future(value, error, _) => Some(TyTemplate::Future(
            Box::new(lower_tir_template(
                value,
                resolved,
                generic_layout,
                TemplateMode::Value,
            )?),
            Box::new(lower_tir_template(
                error,
                resolved,
                generic_layout,
                TemplateMode::Value,
            )?),
            TyAttr::default(),
        )),
        other => Some(realized_leaf_template(&resolved.convert(other))),
    }
}

/// Convert a `Tir2Ty` to `TyTemplate`, mapping each type variable to its
/// canonical runtime frame index.
pub fn tir2_to_template(
    ty: &Tir2Ty,
    resolved: &ResolvedAliases,
    generic_params: &[ParamTy],
) -> TyTemplate {
    let ty = baml_type_runtime::erase_typevars_matching(ty, &|param| {
        baml_type::is_synthetic_effect_param(param.name())
    });
    let generic_layout = RuntimeGenericLayout::new(generic_params);
    lower_tir_template(&ty, resolved, &generic_layout, TemplateMode::Value)
        .unwrap_or_else(|| unreachable!("value template lowering is infallible"))
}

/// [`tir2_to_template`] for a type MIR reads back from TIR rather than lowers
/// itself.
///
/// Value-mode lowering treats a type variable with no slot in this frame as a
/// compiler invariant violation, which is right for a type MIR just lowered
/// against that same frame. A type recorded by inference has a wider
/// provenance — it can name a variable belonging to a scope this frame does
/// not carry, and it can still hold a recovery sentinel where inference had no
/// answer — so answering `None` lets the caller fall back to what it can derive
/// itself instead of tripping that invariant, or the realizedness one, on user
/// code.
fn tir2_to_template_in_frame(
    ty: &Tir2Ty,
    resolved: &ResolvedAliases,
    generic_params: &[ParamTy],
) -> Option<TyTemplate> {
    if baml_type_runtime::contains_error_recovery(ty) {
        return None;
    }
    let ty = baml_type_runtime::erase_typevars_matching(ty, &|param| {
        baml_type::is_synthetic_effect_param(param.name())
    });
    let generic_layout = RuntimeGenericLayout::new(generic_params);
    if baml_type_runtime::contains_typevar_where(&ty, &|param| generic_layout.slot(param).is_none())
    {
        return None;
    }
    lower_tir_template(&ty, resolved, &generic_layout, TemplateMode::Value)
}

/// Lower a realized interface **constraint** to its template form: each generic
/// argument and associated binding lowered individually, so an argument that is an
/// enclosing generic becomes a `TypeArgRef` and reaches the runtime resolver
/// realized against the caller's frame.
///
/// Deliberately not `tir2_to_template` over a `Ty::Interface`. That lowers the
/// interface as a *type*, and yields two different shapes for one meaning — a
/// `TyTemplate::Interface` when some argument is symbolic, a `Concrete` leaf when
/// none is. A dispatch site names the interface it resolves *through*, which is a
/// constraint, not an existential; carrying it as one gives a single shape and
/// makes a non-interface in that position unrepresentable.
pub(crate) fn tir2_interface_to_template(
    name: &baml_type::TypeName,
    args: &[Tir2Ty],
    assoc: &[(Name, Tir2Ty)],
    resolved: &ResolvedAliases,
    generic_params: &[ParamTy],
) -> TyTemplateInterface {
    TyTemplateInterface::new(
        name.clone(),
        args.iter()
            .map(|a| tir2_to_template(a, resolved, generic_params))
            .collect(),
        assoc
            .iter()
            .map(|(n, t)| (n.clone(), tir2_to_template(t, resolved, generic_params)))
            .collect(),
    )
}

/// A resolved `RuntimeTy` with no residual type variables, as a leaf template:
/// it narrows to a [`RealizedTy`] (proving realizedness) and widens into a
/// `Concrete`-equivalent `TyTemplate`. Panics if the type still contains a type
/// variable — callers guarantee realizedness (see the `tir_contains_symbolic`
/// gate), so a failure is a compiler invariant violation, not silent erasure.
fn realized_leaf_template(ty: &RuntimeTy) -> TyTemplate {
    TyTemplate::from(
        RealizedTy::try_from(ty.clone())
            .unwrap_or_else(|e| unreachable!("realized-leaf template must be realized: {e}")),
    )
}

/// Convert a TIR pattern type into a complete [`TyTemplate`], failing closed
/// when a type variable has no runtime frame slot.
fn tir2_to_pattern_template(
    ty: &Tir2Ty,
    resolved: &ResolvedAliases,
    generic_params: &[ParamTy],
) -> Option<TyTemplate> {
    let generic_layout = RuntimeGenericLayout::new(generic_params);
    lower_tir_template(ty, resolved, &generic_layout, TemplateMode::Pattern)
}

/// Convert an already-resolved `baml_type::RuntimeTy` into a complete
/// [`TyTemplate`], or `None` when any position is unresolvable.
///
/// This is used for `IsType` pattern-matching where the pattern type comes
/// through `convert_tir_ty_for_runtime`, which keeps type variables faithful —
/// so a pattern like `(int) -> T` still carries `T` by name here. A realized
/// type becomes a `Concrete`-equivalent leaf (the VM's tag / class-identity
/// fast paths); a composite decomposes structurally.
///
/// FIXME(typevar-templates): a residual `TypeVar`/projection yields `None` —
/// validated here at construction, and emitted as a fail-closed branch by
/// `emit_pattern_template_test` — rather than being erased to a type. This
/// conversion has no frame context, so even a frame-resolvable type variable
/// is unresolvable *here*. No known TIR path still delivers one: every
/// type-variable-carrying pattern (including `Self`, which interface-owned
/// bodies carry at frame slot 0) routes through `tir2_to_pattern_template`
/// instead. Once that is verified across every body kind, collapse this into
/// the infallible realized conversion.
pub(crate) fn ty_to_pattern_template_from_resolved_ty(ty: &RuntimeTy) -> Option<TyTemplate> {
    match ty {
        // See the FIXME above: an unresolved type variable or projection is
        // reported at construction rather than smuggled by name.
        RuntimeTy::TypeVar(..) | RuntimeTy::AssociatedTypeProjection { .. } => None,
        RuntimeTy::List(inner, _) => Some(TyTemplate::list(
            ty_to_pattern_template_from_resolved_ty(inner)?,
        )),
        RuntimeTy::Map { key, value, .. } => Some(TyTemplate::map(
            ty_to_pattern_template_from_resolved_ty(key)?,
            ty_to_pattern_template_from_resolved_ty(value)?,
        )),
        RuntimeTy::Union(members, _) => Some(TyTemplate::union(
            members
                .iter()
                .map(ty_to_pattern_template_from_resolved_ty)
                .collect::<Option<Vec<_>>>()?,
        )),
        RuntimeTy::Class(tn, args, _) if !args.is_empty() => Some(TyTemplate::class(
            tn.clone(),
            args.iter()
                .map(ty_to_pattern_template_from_resolved_ty)
                .collect::<Option<Vec<_>>>()?,
        )),
        // Interfaces decompose so the membership test keeps its instantiation
        // (args + associated bindings).
        RuntimeTy::Interface(tn, args, assoc, _) => Some(TyTemplate::interface(
            tn.clone(),
            args.iter()
                .map(ty_to_pattern_template_from_resolved_ty)
                .collect::<Option<Vec<_>>>()?,
            assoc
                .iter()
                .map(|(name, ty)| {
                    Some((name.clone(), ty_to_pattern_template_from_resolved_ty(ty)?))
                })
                .collect::<Option<Vec<_>>>()?,
        )),
        RuntimeTy::Function {
            params,
            ret,
            throws,
            ..
        } => Some(TyTemplate::Function {
            params: params
                .iter()
                .map(|p| {
                    Some(baml_type::TyTemplateFunctionParamTy {
                        name: p.name.clone(),
                        ty: ty_to_pattern_template_from_resolved_ty(&p.ty)?,
                        mode: p.mode,
                    })
                })
                .collect::<Option<Vec<_>>>()?,
            ret: Box::new(ty_to_pattern_template_from_resolved_ty(ret)?),
            throws: Box::new(ty_to_pattern_template_from_resolved_ty(throws)?),
            attr: TyAttr::default(),
        }),
        RuntimeTy::Future(value, error, _) => Some(TyTemplate::Future(
            Box::new(ty_to_pattern_template_from_resolved_ty(value)?),
            Box::new(ty_to_pattern_template_from_resolved_ty(error)?),
            TyAttr::default(),
        )),
        // A realized leaf (incl. a monomorphic class): narrow and widen. A
        // non-realized value would be a composite handled above or a residual
        // type variable handled at the top — a failure is a compiler invariant
        // violation, not silent erasure.
        other => Some(realized_leaf_template(other)),
    }
}

/// Flatten a `RuntimeTy` union into its leaf members (recursively), pushing each
/// non-union leaf into `out`. A recursive alias is kept as a single `TypeAlias`
/// leaf — it is not expanded — matching `ResolvedAliases::convert`.
fn flatten_runtime_union(ty: &RuntimeTy, out: &mut Vec<RuntimeTy>) {
    match ty {
        RuntimeTy::Union(members, _) => {
            for m in members {
                flatten_runtime_union(m, out);
            }
        }
        other => out.push(other.clone()),
    }
}

/// A scrutinee member is *opaque* for a coarse-tag soundness proof when it could
/// dynamically resolve to a container or class instantiation its static type
/// does not reveal — a type alias (`json` keeps its `json[]`/`map` arms hidden
/// behind an opaque leaf), a type variable, `unknown`, an interface (a container
/// or class may implement it), an associated projection, or `void`. A coarse
/// type-tag test can never be *proven* equivalent to the structural test against
/// such a member, so a tag-sufficiency proof must fail closed on it and emit the
/// element/arg-precise structural test instead.
///
/// Exhaustive on purpose: a new `RuntimeTy` variant must force a deliberate
/// classification here rather than silently defaulting to "safe".
fn member_is_opaque_for_tag_proof(m: &RuntimeTy) -> bool {
    match m {
        RuntimeTy::TypeAlias(..)
        | RuntimeTy::TypeVar(..)
        | RuntimeTy::BuiltinUnknown { .. }
        | RuntimeTy::AssociatedTypeProjection { .. }
        | RuntimeTy::Interface(..)
        | RuntimeTy::Void { .. } => true,
        // A union should have been flattened before this check; recurse
        // defensively so a nested one can't smuggle an opaque member past it.
        RuntimeTy::Union(members, _) => members.iter().any(member_is_opaque_for_tag_proof),
        // Every concrete leaf carries a fixed runtime tag we can reason about.
        RuntimeTy::Int { .. }
        | RuntimeTy::Bigint { .. }
        | RuntimeTy::Float { .. }
        | RuntimeTy::String { .. }
        | RuntimeTy::Bool { .. }
        | RuntimeTy::Null { .. }
        | RuntimeTy::Uint8Array { .. }
        | RuntimeTy::Media(..)
        | RuntimeTy::Class(..)
        | RuntimeTy::Enum(..)
        | RuntimeTy::EnumVariant(..)
        | RuntimeTy::Literal(..)
        | RuntimeTy::Function { .. }
        | RuntimeTy::Future(..)
        | RuntimeTy::List(..)
        | RuntimeTy::Map { .. }
        | RuntimeTy::RustType { .. }
        | RuntimeTy::Type { .. }
        | RuntimeTy::Resource { .. }
        | RuntimeTy::PromptAst { .. }
        | RuntimeTy::Never { .. } => false,
    }
}

/// A parametric shape whose every instantiation collapses onto one coarse type
/// tag. Two arms at different instantiations therefore dedup onto the same
/// jump-table slot, and the first swallows the rest — the conflation
/// [`parametric_arm_tag_sufficient`] has to rule out. Values of *different*
/// shapes carry different tags and can never collide.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TagConflatedShape {
    List,
    Map,
    /// A `Future<T, E>`: every instantiation is `FUTURE`-tagged, and a spawned
    /// future carries the `<T, E>` its spawn site was typed at, so the tag alone
    /// no longer pins which arm a value belongs to.
    Future,
}

/// The [`TagConflatedShape`] of `ty`, or `None` when its tag already pins its
/// values (a primitive, a monomorphic class, …).
fn tag_conflated_shape(ty: &RuntimeTy) -> Option<TagConflatedShape> {
    match ty {
        RuntimeTy::List(..) => Some(TagConflatedShape::List),
        RuntimeTy::Map { .. } => Some(TagConflatedShape::Map),
        RuntimeTy::Future(..) => Some(TagConflatedShape::Future),
        _ => None,
    }
}

/// Whether a coarse `LIST`/`MAP`/`FUTURE` type-tag test for the parametric arm
/// `arm` is a *provably sound* substitute for the argument-discriminating
/// structural test, for values of the scrutinee's resolved static type
/// `scrutinee`.
///
/// Generic-argument positions are invariant (`TYPE_SYSTEM.md` "Variance"): `int[]`,
/// `string[]`, and `json[]` are mutually unrelated types, as are
/// `Future<int, never>` and `Future<string, never>`. So the coarse "any list" /
/// "any map" / "any future" tag is sound only when no value the scrutinee admits
/// could carry that tag with different type arguments than the arm. That holds
/// when every same-shape member of the scrutinee shares the arm's constructor
/// identity, and every other member carries a tag that provably can't be the
/// arm's — a different parametric shape or a concrete leaf.
///
/// We **fail closed**: an opaque member (see [`member_is_opaque_for_tag_proof`])
/// could hide an instantiation with differing arguments, so it blocks the proof
/// and routes the arm through the structural matcher. In particular a `json`
/// scrutinee (an opaque alias leaf) no longer makes an element-specific `int[]`
/// arm "tag-sufficient" — that was fail-*open* covariance, matching any array.
///
/// Returns `false` for a non-parametric `arm` (the caller only asks about one).
fn parametric_arm_tag_sufficient(arm: &RuntimeTy, scrutinee: &RuntimeTy) -> bool {
    use baml_compiler2_hir_ty::exhaustiveness::ty_ctor_identity;
    let Some(arm_shape) = tag_conflated_shape(arm) else {
        return false;
    };
    let arm_id = ty_ctor_identity(arm.as_ty());
    let mut members = Vec::new();
    flatten_runtime_union(scrutinee, &mut members);
    members.iter().all(|m| match tag_conflated_shape(m) {
        // Same shape: sound only for the arm's exact instantiation.
        Some(shape) if shape == arm_shape => ty_ctor_identity(m.as_ty()) == arm_id,
        // A different parametric shape carries a tag that can't be the arm's.
        Some(_) => true,
        // Anything else is sound iff it cannot secretly be a differing
        // instantiation — i.e. iff it is not opaque. Every concrete leaf carries
        // a tag that can't be the arm's, so it passes.
        None => !member_is_opaque_for_tag_proof(m),
    })
}

/// Whether the coarse type tag fully discriminates values reaching a switch arm
/// whose (already union-flattened) member type is `member`, given the enclosing
/// match scrutinee's resolved static type when known.
///
/// Type tags conflate every list, every map, and every instantiation of a
/// generic class, so a tag-keyed jump table sends *any* same-tag value to the
/// first arm carrying that tag. A member is tag-sufficient only when no value
/// the scrutinee admits could be conflated onto it:
///
/// - a list/map/future member defers to [`parametric_arm_tag_sufficient`] (which
///   deliberately treats opaque scrutinee members as non-blockers, preserving
///   the pre-structural erased semantics the chain path keeps for them); with
///   no scrutinee the equivalence is unprovable, so it fails closed to the
///   precise chain;
/// - a parametric class member requires every same-class member of the
///   scrutinee to share its constructor identity (`Foo<int>` is sufficient only
///   if the scrutinee cannot hold a `Foo<string>`), and — being a gate for the
///   arg-precise `ClassWithTypeArgs` chain test rather than a legacy-semantics
///   bridge — fails closed on opaque scrutinee members that could dynamically
///   hold another instantiation;
/// - every other member (primitive, monomorphic class, enum, …) keeps its
///   existing tag behavior.
fn switch_member_tag_sufficient(member: &RuntimeTy, scrutinee: Option<&RuntimeTy>) -> bool {
    use baml_compiler2_hir_ty::exhaustiveness::ty_ctor_identity;
    match member {
        RuntimeTy::List(..) | RuntimeTy::Map { .. } | RuntimeTy::Future(..) => {
            scrutinee.is_some_and(|scrutinee| parametric_arm_tag_sufficient(member, scrutinee))
        }
        // Every enum collapses onto the single shared `ENUM` type tag, so the
        // jump table can key an enum-type arm only when no *other* enum type
        // reaches this match — otherwise two enum arms dedup onto that one tag
        // and the first swallows the rest. When several enum types coexist the
        // arm falls to the precise chain, which dispatches on enum-pointer
        // identity (`is Color` compares the value's enum object).
        RuntimeTy::Enum(name, _) => {
            let Some(scrutinee) = scrutinee else {
                return false;
            };
            let mut flat = Vec::new();
            flatten_runtime_union(scrutinee, &mut flat);
            flat.iter().all(|m| match enum_type_name(m) {
                // Another enum type shares this arm's `ENUM` tag → collision.
                Some(m_name) => m_name == name,
                // A concrete non-enum member carries a different runtime tag and
                // can't collide, but an opaque member could dynamically be a
                // *different* enum (also `ENUM`-tagged) — so it fails closed, like
                // the container and class branches.
                None => !member_is_opaque_for_tag_proof(m),
            })
        }
        // A specific variant type (`Color.Red`) needs variant-level discrimination
        // the shared `ENUM` tag can't express, so always take the precise chain.
        RuntimeTy::EnumVariant(..) => false,
        RuntimeTy::Class(name, args, _) => {
            // Reflection-kind classes are transparent views over an
            // `Object::Type`: their precise `is` test inspects the wrapped
            // realized type, while `TypeTag` sees only the outer `TYPE` tag.
            // A tag switch would therefore miss every kind arm.
            if baml_type::type_kind::is_type_kind_class(name) {
                return false;
            }
            if args.is_empty() {
                return true;
            }
            let Some(scrutinee) = scrutinee else {
                return false;
            };
            let member_id = ty_ctor_identity(member.as_ty());
            let mut flat = Vec::new();
            flatten_runtime_union(scrutinee, &mut flat);
            flat.iter().all(|m| match m {
                RuntimeTy::Class(m_name, _, _) if m_name == name => {
                    ty_ctor_identity(m.as_ty()) == member_id
                }
                // A different class dispatches on a different tag.
                RuntimeTy::Class(..) => true,
                // An opaque member may admit another instantiation of this
                // class at runtime; the proof fails closed.
                _ => !member_is_opaque_for_tag_proof(m),
            })
        }
        _ => true,
    }
}

/// The enum type name of an enum-shaped type. A bare enum (`Color`) and any of
/// its variants (`Color.Red`) collapse onto the same shared `ENUM` type tag, so
/// both count as "an enum of this type" for the tag-collision check in
/// [`switch_member_tag_sufficient`].
fn enum_type_name(ty: &RuntimeTy) -> Option<&TypeName> {
    match ty {
        RuntimeTy::Enum(name, _) | RuntimeTy::EnumVariant(name, _, _) => Some(name),
        _ => None,
    }
}

// ─── def_to_item_ref helper ──────────────────────────────────────────────────

use baml_compiler2_hir::{
    compiler2_all_files, contributions::Definition, file_package::file_package,
};

pub fn def_to_item_ref<'db>(db: &'db dyn crate::Db, def: Definition<'db>) -> ItemRef {
    use baml_compiler2_ppir::item_data::{
        ImplSubjectData, MethodOwner, class_data, client_data, enum_data, function_data,
        impl_block_data, interface_data, let_data, method_owner, retry_policy_data,
        template_string_data, test_data, type_alias_data,
    };
    let pkg_info = file_package(db, def.file(db));

    let name: Name = match def {
        Definition::Function(loc) => function_data(db, loc).name.clone(),
        Definition::Class(loc) => class_data(db, loc).name.clone(),
        Definition::Enum(loc) => enum_data(db, loc).name.clone(),
        Definition::Interface(loc) => interface_data(db, loc).name.clone(),
        Definition::TypeAlias(loc) => type_alias_data(db, loc).name.clone(),
        Definition::TemplateString(loc) => template_string_data(db, loc).name.clone(),
        Definition::Client(loc) => client_data(db, loc).name.clone(),
        Definition::Test(loc) => test_data(db, loc).name.clone(),
        Definition::RetryPolicy(loc) => retry_policy_data(db, loc).name.clone(),
        Definition::Let(loc) => let_data(db, loc).name.clone(),
    };

    // Function definitions: a method needs a Method-shaped ItemRef so it gets a
    // distinct global slot keyed on its owner's name (instead of colliding with
    // same-named free functions in the package).
    if let Definition::Function(func_loc) = def {
        match method_owner(db, func_loc) {
            Some(MethodOwner::Class(class_loc)) => {
                return method_item_ref(db, class_loc, func_loc);
            }
            Some(MethodOwner::Interface(iface_loc)) => {
                return ItemRef::Method {
                    package: pkg_info.package.clone(),
                    namespace: pkg_info.namespace_path,
                    class: interface_data(db, iface_loc).name.clone(),
                    name,
                };
            }
            Some(MethodOwner::FreeImpl(impl_loc)) => {
                let block = impl_block_data(db, impl_loc);
                if let ImplSubjectData::Free { for_target, .. } = &block.subject {
                    return ItemRef::Method {
                        package: pkg_info.package.clone(),
                        namespace: pkg_info.namespace_path,
                        class: Name::new(format!(
                            "{}$for${}",
                            block.type_refs.display(block.interface_target),
                            block.type_refs.display(*for_target)
                        )),
                        name,
                    };
                }
            }
            None => {}
        }
    }

    ItemRef::Free {
        package: pkg_info.package.clone(),
        namespace: pkg_info.namespace_path,
        name,
    }
}

fn scoped_implements_method_name<'db>(
    db: &'db dyn crate::Db,
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
    method_name: &Name,
) -> Name {
    baml_compiler2_ppir::item_data::method_interface_target(db, func_loc)
        .as_ref()
        .map(|target| {
            Name::new(format!(
                "{}.{method_name}",
                target.type_refs.display(target.target)
            ))
        })
        .unwrap_or_else(|| method_name.clone())
}

fn method_item_ref<'db>(
    db: &'db dyn crate::Db,
    class_loc: baml_compiler2_hir::loc::ClassLoc<'db>,
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
) -> ItemRef {
    use baml_compiler2_ppir::item_data::{class_data, function_data};
    let pkg_info = file_package(db, class_loc.file(db));
    let class = class_data(db, class_loc).name.clone();
    let method_name = function_data(db, func_loc).name.clone();
    ItemRef::Method {
        package: pkg_info.package,
        namespace: pkg_info.namespace_path,
        class,
        name: scoped_implements_method_name(db, func_loc, &method_name),
    }
}

/// Convert a `MemberResolution` (from TIR) into an `ItemRef` (for MIR).
///
/// Only `Method` and `Free` variants are callable — callers must guard against
/// `Field` and `Variant` variants before calling this function.
fn resolution_to_item_ref(
    db: &dyn crate::Db,
    res: &crate::inference_provider::MemberResolution<'_>,
) -> Option<ItemRef> {
    use crate::inference_provider::MemberResolution;
    match res {
        MemberResolution::Free { func_loc } => {
            let pkg_info = file_package(db, func_loc.file(db));
            let func_data = baml_compiler2_ppir::item_data::function_data(db, *func_loc);
            Some(ItemRef::Free {
                package: pkg_info.package,
                namespace: pkg_info.namespace_path,
                name: func_data.name.clone(),
            })
        }
        MemberResolution::BoundMethod {
            class_loc,
            func_loc,
        }
        | MemberResolution::UnboundMethod {
            class_loc,
            func_loc,
        } => Some(method_item_ref(db, *class_loc, *func_loc)),
        MemberResolution::InterfaceVirtualMethod { iface_loc, method } => {
            // A virtual interface-method call: the ItemRef names the interface + method, and
            // the runtime dispatches on the receiver's actual impl.
            let pkg_info = file_package(db, iface_loc.file(db));
            let iface_data = baml_compiler2_ppir::item_data::interface_data(db, *iface_loc);
            Some(ItemRef::Method {
                package: pkg_info.package,
                namespace: pkg_info.namespace_path,
                class: iface_data.name.clone(),
                name: method.clone(),
            })
        }
        MemberResolution::InterfaceConcreteMethod { impl_loc, func_loc } => {
            // A statically-resolved interface-method call. Route it through the interface's
            // method ref — the runtime dispatches on the concrete receiver's registered impl,
            // which is correct. (A direct static call to `func_loc` is a valid optimization,
            // not required for correctness; it is deferred.)
            let block = baml_compiler2_ppir::item_data::impl_block_data(db, *impl_loc);
            let impl_pkg = file_package(db, impl_loc.file(db));
            let impl_pkg_items = baml_compiler2_ppir::package_items(
                db,
                baml_compiler2_hir::package::PackageId::new(db, impl_pkg.package.clone()),
            );
            let iface_loc = resolve_ref_to_interface_loc(
                db,
                &block.type_refs,
                block.interface_target,
                impl_pkg_items,
                &impl_pkg.namespace_path,
            )?;
            let pkg_info = file_package(db, iface_loc.file(db));
            let iface_data = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
            let func_data = baml_compiler2_ppir::item_data::function_data(db, *func_loc);
            Some(ItemRef::Method {
                package: pkg_info.package,
                namespace: pkg_info.namespace_path,
                class: iface_data.name.clone(),
                name: func_data.name.clone(),
            })
        }
        MemberResolution::External(external) => {
            use baml_compiler2_hir_ty::callable::ExternalCallTarget;
            Some(match &external.target {
                ExternalCallTarget::Free {
                    package,
                    namespace,
                    name,
                } => ItemRef::Free {
                    package: package.clone(),
                    namespace: namespace.clone(),
                    name: name.clone(),
                },
                ExternalCallTarget::Method {
                    package,
                    namespace,
                    class,
                    name,
                } => ItemRef::Method {
                    package: package.clone(),
                    namespace: namespace.clone(),
                    class: class.clone(),
                    name: name.clone(),
                },
                ExternalCallTarget::Interface { interface, method } => ItemRef::Method {
                    package: interface.package().clone(),
                    namespace: interface.namespace().clone(),
                    class: interface.name().clone(),
                    name: method.clone(),
                },
            })
        }
        MemberResolution::Field { .. }
        | MemberResolution::Variant { .. }
        | MemberResolution::InterfaceVirtualField { .. }
        | MemberResolution::ExternalField { .. }
        | MemberResolution::ExternalVariant { .. }
        | MemberResolution::ExternalInterfaceVirtualField { .. } => None,
    }
}

/// The statically-known callable body of a member resolution, if any. Interface **virtual**
/// methods dispatch at runtime and have no static body (`None`); a concrete interface method
/// carries its resolved `func_loc`. Field / variant / virtual-field resolutions are not
/// callable. Centralizes the `func_loc` extraction the call-lowering paths share.
fn resolution_func_loc<'db>(
    res: &crate::inference_provider::MemberResolution<'db>,
) -> Option<baml_compiler2_hir::loc::FunctionLoc<'db>> {
    use crate::inference_provider::MemberResolution;
    match res {
        MemberResolution::Free { func_loc }
        | MemberResolution::BoundMethod { func_loc, .. }
        | MemberResolution::UnboundMethod { func_loc, .. }
        | MemberResolution::InterfaceConcreteMethod { func_loc, .. } => Some(*func_loc),
        MemberResolution::External(_)
        | MemberResolution::InterfaceVirtualMethod { .. }
        | MemberResolution::Field { .. }
        | MemberResolution::Variant { .. }
        | MemberResolution::InterfaceVirtualField { .. }
        | MemberResolution::ExternalField { .. }
        | MemberResolution::ExternalVariant { .. }
        | MemberResolution::ExternalInterfaceVirtualField { .. } => None,
    }
}

// ─── LoweringContext ─────────────────────────────────────────────────────────

// Re-use ExprId from baml_compiler2_ast (already imported above via ExprId)
use baml_compiler2_ast::{
    AssignOp as AstAssignOp, AstSourceMap, BinaryOp as AstBinaryOp, CallArg, Expr as AstExpr,
    ExprBody as AstExprBody, ExprId as AstExprId, Literal as AstLiteral, PatId as AstPatId,
    Pattern as AstPattern, Stmt as AstStmt, StmtId as AstStmtId, TypeArg as AstTypeArg,
    TypeExpr as AstTypeExpr, TypeExprKind as AstTypeExprKind, UnaryOp as AstUnaryOp,
};
use baml_compiler2_hir::{
    body::{FunctionBody, LetBody, let_body, let_body_source_map},
    loc::{FunctionLoc, LetLoc},
    package::{PackageId, package_items},
    scope::FileScopeId,
    semantic_index::{
        BindingId, DefinitionSite, ExprMetadataKey, ExprMetadataScope as MetadataScope,
        PathResolution,
    },
};
use baml_compiler2_ppir::{
    file_semantic_index,
    resolve::{ResolvedName, resolve_name_at_in_scope},
};
use rustc_hash::{FxHashMap, FxHashSet};

type ClassFieldIndices = IndexMap<TypeName, IndexMap<String, usize>>;
type ClassFieldTypes = IndexMap<TypeName, IndexMap<String, RuntimeTy>>;
type EnumVariantIndices = IndexMap<QualifiedTypeName, IndexMap<String, usize>>;
type InterfaceTypeView = (TypeName, Vec<Tir2Ty>, Vec<(Name, Tir2Ty)>);

/// A virtual field access resolved down to what the instruction actually carries:
/// the receiver, the interface the access travels on the wire, and the field's index
/// in *that* interface's own declared field list.
///
/// Holding these together is what lets the write path resolve every fallible step
/// before it lowers a single operand — see [`LoweringContext::virtual_field_assign_target`].
struct VirtualFieldTarget {
    receiver: Local,
    field: Name,
    /// The interface that *declares* `field`, realized at the receiver — never the
    /// child interface a `requires` closure was entered through.
    iface: TyTemplateInterface,
    field_index: u32,
}

/// Lower the generic arguments of an interface target held as a `TypeRefId` in
/// `store` (e.g. the `<int>` in `implements Slot<int>`). Non-`Path` targets
/// contribute no arguments.
fn lower_ref_interface_target_args<'db>(
    db: &'db dyn crate::Db,
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
    target: baml_compiler2_hir::type_ref::TypeRefId,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
    generic_params: &[ParamTy],
    bounds: &FxHashMap<ParamTy, Vec<baml_type::interned::InterfaceRef>>,
) -> Vec<Tir2Ty> {
    match &store[target].kind {
        baml_compiler2_hir::type_ref::TypeRefKind::Path { generic_args, .. } => generic_args
            .iter()
            .map(|&arg| {
                lower_ref_in_scope(
                    db,
                    store,
                    arg,
                    pkg_items,
                    namespace_path,
                    generic_params,
                    bounds,
                    None,
                )
            })
            .collect(),
        _ => Vec::new(),
    }
}

struct PackagePopulation<'a> {
    class_fields: &'a mut ClassFieldIndices,
    class_field_types: &'a mut ClassFieldTypes,
    enum_variants: &'a mut EnumVariantIndices,
}

/// All package-invariant data needed to construct a [`LoweringContext`]: the
/// class/enum/interface schema maps plus resolved type aliases.
///
/// `LoweringContext::new` runs once per function, but every function in a
/// package sees the *same* schema. Building this inline per function made MIR
/// lowering `O(functions × classes)` — each function re-lowered every class
/// field type (`populate_from_package`) and recomputed every alias
/// (`ResolvedAliases::for_package`, which also re-runs `find_recursive_aliases`
/// over the whole project). Computing it once in the [`package_lowering_data`]
/// Salsa query collapses that to `O(classes)` total; the maps are then borrowed
/// (not cloned) into each `LoweringContext`.
#[derive(Clone, PartialEq, Eq, Default)]
struct PackageLoweringData {
    class_fields: ClassFieldIndices,
    class_field_types: ClassFieldTypes,
    enum_variants: EnumVariantIndices,
    resolved_aliases: ResolvedAliases,
    /// Every method name any in-scope interface (own package + dependency
    /// closure) declares, required or default. Fast pre-filter for
    /// [`LoweringContext::dispatch_target_for_concrete`]: a member name absent
    /// here can never dispatch through an interface impl, so the (hot) impl
    /// enumeration is skipped for the overwhelmingly common plain-method /
    /// field-access case.
    interface_method_names: FxHashSet<Name>,
}

/// # Safety
///
/// Mirrors [`baml_compiler2_hir::package::PackageItems`]'s impl. The contained
/// maps hold no Salsa-interned (`'db`) data, so storing them by value is sound;
/// `maybe_update` uses `PartialEq` for proper Salsa early-cutoff.
#[allow(unsafe_code)]
unsafe impl salsa::Update for PackageLoweringData {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        // SAFETY: `old_pointer` is valid, aligned, and Salsa-owned.
        #[allow(unsafe_code)]
        let old = unsafe { &*old_pointer };
        if old == &new_value {
            false
        } else {
            #[allow(unsafe_code)]
            unsafe {
                std::ptr::drop_in_place(old_pointer);
                std::ptr::write(old_pointer, new_value);
            }
            true
        }
    }
}

/// Project-wide class → runtime type-tag map. See
/// [`class_type_tags_for_project`].
#[derive(Debug, PartialEq)]
struct ProjectClassTypeTags {
    tags: IndexMap<TypeName, i64>,
}

/// Mirrors [`PackageLoweringData`]'s impl: the map holds no Salsa-interned
/// (`'db`) data, so storing it by value is sound; `maybe_update` uses
/// `PartialEq` for proper Salsa early-cutoff.
#[allow(unsafe_code)]
unsafe impl salsa::Update for ProjectClassTypeTags {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        // SAFETY: `old_pointer` is valid, aligned, and Salsa-owned.
        #[allow(unsafe_code)]
        let old = unsafe { &*old_pointer };
        if old == &new_value {
            false
        } else {
            #[allow(unsafe_code)]
            unsafe {
                std::ptr::drop_in_place(old_pointer);
                std::ptr::write(old_pointer, new_value);
            }
            true
        }
    }
}

/// Build `class_type_tags` for every class in the project, once, memoized by
/// Salsa.
///
/// Tags are content-addressed (`typetag::class_type_tag` over the
/// fully-qualified name), so they match the `class.type_tag` values the
/// emitter assigns by construction — no iteration-order coupling — and a
/// class keeps its tag regardless of what other code exists.
///
/// This was previously an untracked helper called from every
/// `LoweringContext` construction — i.e. the whole project's item trees were
/// walked, and every class name re-rendered and re-hashed, once per lowered
/// function/let (see `crates/tools_compile_profile/README.md`, July 2026
/// audit, item #4). `project` is only the memo key; the body's file/item
/// reads are tracked as dependencies through `db` as usual.
#[salsa::tracked(returns(ref))]
fn class_type_tags_for_project(
    db: &dyn crate::Db,
    _project: baml_workspace::Project,
) -> ProjectClassTypeTags {
    use baml_compiler2_ppir::item_data::{class_data, file_classes};
    let all_files = compiler2_all_files(db);
    let mut tags: IndexMap<TypeName, i64> = IndexMap::new();

    for file in &all_files {
        let pkg_info = file_package(db, *file);

        for class_loc in file_classes(db, *file) {
            let class = class_data(db, *class_loc);
            let class_qtn = QualifiedTypeName::new(
                pkg_info.package.clone(),
                pkg_info.namespace_path.clone(),
                class.name.clone(),
            );
            let type_tag = baml_type::typetag::class_type_tag(&class_qtn.render_dotted(false));
            // Use entry to avoid overwriting if the same class appears via multiple paths
            // (e.g., both FQ and short names). First encounter wins — consistent with emit.rs.
            tags.entry(class_qtn).or_insert(type_tag);
        }
    }

    // Mounted packages have no source files, but use the same
    // content-addressed tag derived from their fully-qualified class name.
    for pkg_name in baml_compiler2_hir::package::mounted_package_names(db) {
        let Some(mounted) =
            baml_compiler2_hir_ty::package_interface::mounted_interface(db, &pkg_name)
        else {
            continue;
        };
        for types_in_ns in mounted.types.values() {
            for exported in types_in_ns.values() {
                if let baml_compiler2_hir_ty::package_interface::ExportedType::Class {
                    qtn, ..
                } = exported
                {
                    let type_tag = baml_type::typetag::class_type_tag(&qtn.render_dotted(false));
                    tags.entry(qtn.clone()).or_insert(type_tag);
                }
            }
        }
    }

    ProjectClassTypeTags { tags }
}

/// Build the package-invariant [`PackageLoweringData`] once per package,
/// memoized by Salsa and shared across every function's `LoweringContext`.
#[salsa::tracked(returns(ref))]
fn package_lowering_data<'db>(
    db: &'db dyn crate::Db,
    pkg_id: baml_compiler2_hir::package::PackageId<'db>,
) -> PackageLoweringData {
    use baml_compiler2_hir::package::{package_dependencies, package_dependency_closure};
    // The canonical (PPIR) item view: includes synthesized `*$stream` classes,
    // whose fields must be projectable like any other class's. TIR already
    // resolves types against this view; using HIR's pre-expansion view here
    // made MIR ICE on field access against a `$stream` partial.
    use baml_compiler2_ppir::package_items;

    let resolved_aliases = resolved_aliases_for_package(db, pkg_id);

    let mut class_fields = ClassFieldIndices::default();
    let mut class_field_types = ClassFieldTypes::default();
    let mut enum_variants = EnumVariantIndices::default();
    {
        let mut population = PackagePopulation {
            class_fields: &mut class_fields,
            class_field_types: &mut class_field_types,
            enum_variants: &mut enum_variants,
        };

        // Dependency packages first (e.g., "baml" builtins); current-package
        // items overwrite on collision.
        for &dep_id in package_dependencies(db, pkg_id) {
            let dep_name = dep_id.name(db);
            if let Some(mounted) =
                baml_compiler2_hir_ty::package_interface::mounted_interface(db, &dep_name)
            {
                LoweringContext::populate_from_mounted_package(
                    mounted,
                    &mut population,
                    &resolved_aliases,
                );
                continue;
            }
            let dep_items = package_items(db, dep_id);
            LoweringContext::populate_from_package(
                db,
                dep_items,
                &dep_name,
                &mut population,
                &resolved_aliases,
            );
        }

        let pkg_items = package_items(db, pkg_id);
        let pkg_name = pkg_id.name(db);
        LoweringContext::populate_from_package(
            db,
            pkg_items,
            &pkg_name,
            &mut population,
            &resolved_aliases,
        );
    }

    // Collect every interface-declared method name in scope (own package +
    // dependency closure). `dispatch_target_for_concrete` previously enumerated
    // every impl block in the closure (via `l1_impl_views_for_recv`, which probes
    // each impl's pattern against the receiver — running alias normalization /
    // subtype checks per probe) for EVERY method call and field access, just to
    // conclude "no impl provides this member" for the overwhelmingly common
    // non-interface member. This set answers that in one hash lookup. Names are
    // collected package-wide (not per receiver type), so the filter stays a
    // pure fast path: any name declared by ANY reachable interface still falls
    // through to the full impl enumeration + `mir_interface_declares_method`.
    let mut interface_method_names = FxHashSet::default();
    let mut all_pkgs = vec![pkg_id];
    all_pkgs.extend(package_dependency_closure(db, pkg_id).iter().copied());
    for pkg in all_pkgs {
        if let Some(mounted) =
            baml_compiler2_hir_ty::package_interface::mounted_interface(db, &pkg.name(db))
        {
            for types_in_ns in mounted.types.values() {
                for exported in types_in_ns.values() {
                    if let baml_compiler2_hir_ty::package_interface::ExportedType::Interface {
                        required_methods,
                        default_methods,
                        ..
                    } = exported
                    {
                        for method in required_methods.iter().chain(default_methods) {
                            interface_method_names.insert(method.name.clone());
                        }
                    }
                }
            }
            continue;
        }
        let items = package_items(db, pkg);
        for ns in items.namespaces.values() {
            for def in ns.types.values() {
                let baml_compiler2_hir::contributions::Definition::Interface(iface_loc) = def
                else {
                    continue;
                };
                let iface_data = baml_compiler2_ppir::item_data::interface_data(db, *iface_loc);
                for sig in &iface_data.required_methods {
                    interface_method_names.insert(sig.name.clone());
                }
                for &fn_loc in &iface_data.default_methods {
                    interface_method_names.insert(
                        baml_compiler2_ppir::item_data::function_data(db, fn_loc)
                            .name
                            .clone(),
                    );
                }
            }
        }
    }

    PackageLoweringData {
        class_fields,
        class_field_types,
        enum_variants,
        resolved_aliases,
        interface_method_names,
    }
}

#[derive(Clone, Copy)]
struct DispatchCallLowering<'a> {
    expr_id: AstExprId,
    args: &'a [AstExprId],
    runtime_id: Option<AstExprId>,
    dest: &'a Place,
}

type PatMetadataKey = (MetadataScope, AstPatId);

struct LoweringContext<'db> {
    db: &'db dyn crate::Db,
    builder: MirBuilder,
    locals: HashMap<Name, Local>,
    binding_locals: HashMap<BindingId, Local>,
    loop_context: Option<LoopContext>,
    catch_context: Option<CatchContext>,
    catch_rethrow_locals: Vec<Local>,
    exit_block: BlockId,

    // THE inference table store: converted once at construction into
    // MIR's own consumption vocabulary, whichever engine produced them.
    // Scopes outside this function answer `None`, and at least one caller
    // (`try_lower_to_string_fallback`) keys behavior on that absence.
    tables: crate::inference_provider::ProviderTables<'db>,
    // Function generic bounds, lowered in TIR space. MIR uses these to keep
    // bounded type variables ABI-erased while still lowering bound-member
    // access through the interface dispatch machinery. A bound *is* an interface
    // constraint, never a type (`TYPE_SYSTEM.md` "Interfaces"), so it is held as
    // `baml_type::Interface` — a non-interface bound is rejected at its
    // declaration and never reaches here. One entry per parameter holding its
    // full `extends A & B` conjunction: a member may be provided by any
    // conjunct, so dispatch searches them in declaration order.
    generic_param_bounds: FxHashMap<ParamTy, Vec<baml_type::Interface>>,

    // Package-shared memo for interface-dispatch candidate resolution. Shared
    // across every function the emit driver lowers in one package (fresh and
    // private when constructed via the uncached entry points).

    // TIR types of the in-scope lambda parameters, by name. TIR does not record
    // `path_segment_types` for a lambda-parameter receiver (`(a: T) -> a.m()`),
    // so interface dispatch on such a receiver falls back to this map to learn
    // its static type — e.g. a bounded type variable whose `extends` bound
    // names the dispatching interface (`a.compare(b)` where `T extends
    // Comparable`). Saved/restored across nested lambdas.
    lambda_param_tir_types: FxHashMap<Name, Tir2Ty>,

    /// The `(local, resolved static type)` of the value the enclosing `match` is
    /// scrutinizing, set for the duration of its arm lowering (save/restored
    /// across nested matches). A container arm's runtime type test consults this
    /// — guarded on the local matching — to decide whether a coarse `LIST`/`MAP`
    /// tag suffices in place of a structural element check (see
    /// [`parametric_arm_tag_sufficient`]). `None` outside a match, and the local
    /// guard keeps `is` / standalone tests element-precise even inside one.
    match_scrutinee: Option<(Local, RuntimeTy)>,

    /// Stable values read while testing patterns, keyed by pattern identity.
    /// Binding lowering reuses these exact values instead of rereading mutable
    /// captured cells or object fields after a successful test.
    tested_pattern_values: HashMap<PatMetadataKey, Local>,
    atomic_pattern_test: bool,

    // The FileScopeId of the expression body currently being lowered.
    // Updated when descending into lambda bodies (Phase 3+).
    current_scope: FileScopeId,
    // Metadata namespace for the expression arena currently being lowered.
    current_metadata_scope: MetadataScope,

    // AST expression body and source map
    body: AstExprBody,
    source_map: Option<AstSourceMap>,
    file: baml_base::SourceFile,
    func_loc: Option<FunctionLoc<'db>>,
    source_param_scope: Option<FileScopeId>,
    /// Raw function name from the item tree (e.g. `"Foo$render_prompt"`).
    /// Used to disambiguate companion scopes that share the same span.
    scope_func_name: Option<Name>,

    // Schema maps built from PackageItems.
    // class_fields and class_type_tags are keyed by TypeName (name + module_path)
    // so that e.g. baml.http.Request and a user-defined Request are distinct.
    // enum_variants is keyed by QualifiedTypeName for the same reason: distinct
    // namespaces can define enums with the same short name.
    // Borrowed from the package-keyed `package_lowering_data` query so every
    // function in a package shares one computation instead of rebuilding these
    // (see [`PackageLoweringData`]).
    class_fields: &'db ClassFieldIndices,
    class_field_types: &'db ClassFieldTypes,
    enum_variants: &'db EnumVariantIndices,
    /// Pre-computed type tags for class types, used by `SwitchKind::TypeTag`
    /// for union-type switch optimization (ported from MIR 1). Borrowed from
    /// the Salsa-memoized [`class_type_tags_for_project`] (was rebuilt per
    /// lowered function).
    class_type_tags: &'db IndexMap<TypeName, i64>,
    /// BEP-044: for every interface, the list of classes that implement it
    /// (directly or transitively through interface `requires`). Lets the field-access
    /// and method-call lowering paths emit a type-tag switch over the
    /// implementor set when the static receiver type is an interface.
    /// BEP-044: non-class concrete implementors, such as
    /// `implements Debuggable for int`. These are kept separate from
    /// `interface_implementors` because reflection/runtime metadata stores
    /// named classes, while dispatch can use primitive type tags directly.
    // Pre-computed type alias data for inline expansion in convert_tir_ty_for_runtime.
    // Borrowed from `package_lowering_data` (shared across every function in
    // the package) rather than cloned per context.
    resolved_aliases: &'db ResolvedAliases,

    /// All method names declared by in-scope interfaces — see
    /// [`PackageLoweringData::interface_method_names`]. Fast pre-filter in
    /// `dispatch_target_for_concrete`.
    interface_method_names: &'db FxHashSet<Name>,

    /// Stack of pending `defer` block bodies (BEP-042), parallel to
    /// lexical scopes. Each entry is the `AstExprId` of a defer body
    /// (an inline `Expr::Block`). Pushed by `lower_stmt`; replayed (LIFO,
    /// re-lowered inline) at every scope exit by `replay_defers_to_depth`.
    /// Swapped at lambda boundaries so a lambda body never replays the parent's
    /// defers.
    defer_stack: Vec<AstExprId>,

    // Counter for generating unique synthetic variable names (e.g. __for_idx, __for_idx_1)
    synthetic_name_counts: HashMap<String, usize>,

    // Lambda functions lowered during body traversal.
    // Collected here and moved into MirFunction.lambdas at the end of lowering.
    // Each entry is a fully-lowered MirFunction for one lambda expression.
    pending_lambdas: Vec<MirFunction>,

    // Generic params of the enclosing lambda(s), accumulated outermost-first.
    // Empty at top-level; `lower_lambda` extends it with the lambda's own
    // `generic_params` while lowering its body and restores it afterward.
    // `enclosing_generic_params()` appends this so that `reflect.type_of<T>`
    // (and other type-arg resolution) inside a generic lambda body resolves the
    // lambda's `T` to the correct `TypeArgRef` slot — `func_loc` only knows the
    // enclosing top-level function's (and class's) params, never a lambda's.
    lambda_generic_params: Vec<ParamTy>,

    /// Lexical `type T = unreflect(value)` parameters currently visible.
    runtime_type_binding_params: Vec<ParamTy>,

    // Capture map for the current lambda body.
    // `Some(map)` when lowering inside a lambda body; `None` for top-level functions.
    // Maps captured binding identity -> index into the closure's captures array.
    // Used by `lower_path_expr` to resolve references to captured variables as
    // `Place::Capture(idx)` instead of `Place::Local(_)`.
    capture_indices: Option<HashMap<BindingId, usize>>,

    // Bindings that were added to the current lambda's capture list transitively
    // because an inner lambda needed them but they were not in the HIR capture
    // list for this lambda. Collected by the parent `lower_lambda` call after
    // the body is lowered so it can extend the outer MakeClosure with extra captures.
    transitive_captures_needed: Vec<BindingId>,

    /// The tagged-template body-lambda parameters currently in scope (BEP-049
    /// §10 / M4e.1), mapped to the synthetic `BindingId::parameter` that
    /// `build_tagged_body_closure` assigns each. These are MIR-only locals — they
    /// have no HIR binding (the tag can't be resolved during the HIR walk), so
    /// `resolve_name_at_in_scope` returns `Unknown` for them. `lower_path_expr`
    /// consults this map to resolve them: directly from `self.locals` when the
    /// reference sits in the body closure itself, or — when a *nested* lambda
    /// inside the interpolations references one — via a transitive capture keyed
    /// on the stored `BindingId` (HIR can't list it, so the standard capture path
    /// misses it). Saved/restored around each closure body so it stays scoped to
    /// the right template.
    tagged_body_param_bindings: HashMap<Name, BindingId>,

    /// Stack of null-exit blocks for active `OptionalChain` scopes.
    /// When an `OptionalFieldAccess`/`OptionalIndex`/`OptionalCall` encounters null,
    /// it jumps to the top of this stack instead of creating its own null block.
    chain_null_exits: Vec<BlockId>,

    /// Optimization level controlling MIR-level transforms.
    /// At `OptLevel::Two`, constant folding and advanced transforms are applied.
    opt: crate::OptLevel,
}

#[allow(clippy::elidable_lifetime_names)]
impl<'db> LoweringContext<'db> {
    fn baml_iter_qtn(name: &str) -> QualifiedTypeName {
        QualifiedTypeName::new(Name::new("baml"), vec![Name::new("iter")], Name::new(name))
    }

    fn baml_iter_type_name(name: &str) -> TypeName {
        Self::baml_iter_qtn(name)
    }

    /// A `baml.ops.<name>` interface name (`Equals`, `Compare`, …).
    fn baml_ops_qtn(name: &str) -> QualifiedTypeName {
        QualifiedTypeName::new(Name::new("baml"), vec![Name::new("ops")], Name::new(name))
    }

    fn baml_iter_done_ty() -> RuntimeTy {
        RuntimeTy::Class(Self::baml_iter_type_name("Done"), vec![], TyAttr::default())
    }

    fn associated_binding_ty(bindings: &[(Name, Tir2Ty)], name: &str) -> Option<Tir2Ty> {
        bindings
            .iter()
            .find(|(binding_name, _)| binding_name.as_str() == name)
            .map(|(_, ty)| ty.clone())
    }

    fn substitute_class_params_in_interface_view(
        view: InterfaceTypeView,
        class_params: &[ParamTy],
        class_args: &[Tir2Ty],
    ) -> InterfaceTypeView {
        if class_params.is_empty() {
            return view;
        }
        let mut bindings = FxHashMap::default();
        for (param, arg) in class_params.iter().zip(class_args.iter()) {
            bindings.insert(param.clone(), arg.clone());
        }
        for param in class_params {
            bindings
                .entry(param.clone())
                .or_insert_with(|| Tir2Ty::TypeVar(param.clone(), TyAttr::default()));
        }

        let (tn, args, assoc) = view;
        let args = args
            .into_iter()
            .map(|ty| baml_type_runtime::substitute_ty(&ty, &bindings))
            .collect();
        let assoc = assoc
            .into_iter()
            .map(|(name, ty)| (name, baml_type_runtime::substitute_ty(&ty, &bindings)))
            .collect();
        (tn, args, assoc)
    }

    fn interface_view_for_class_tir_ty(
        &self,
        class_qtn: &QualifiedTypeName,
        class_args: &[Tir2Ty],
        target_tn: &TypeName,
    ) -> Option<InterfaceTypeView> {
        let class_tn = class_qtn.clone();
        let class_loc = self.resolve_class_loc_by_type_name(&class_tn)?;
        let class_data = baml_compiler2_ppir::item_data::class_data(self.db, class_loc);
        let class_params = baml_compiler2_hir_ty::lower::class_generic_frame(self.db, class_loc);

        for impl_block in &class_data.implements {
            let view = self.resolve_implements_target_view(
                impl_block.target,
                &impl_block.associated_type_bindings,
                class_loc,
            )?;
            let views = self.interface_closure_type_name_views(&view.0, &view.1, &view.2)?;
            for candidate in views {
                if candidate.0 == *target_tn {
                    return Some(Self::substitute_class_params_in_interface_view(
                        candidate,
                        &class_params,
                        class_args,
                    ));
                }
            }
        }
        None
    }

    /// The realized view of the `target_tn` interface a receiver of type `actual_ty`
    /// provides — through its own `requires` closure for an interface-existential
    /// receiver, or through one of its impls (own or `requires`-inherited views) for
    /// a concrete-headed receiver, enumerated via the canonical L1 substrate.
    fn realized_interface_view_for(
        &self,
        actual_ty: &Tir2Ty,
        target_tn: &TypeName,
    ) -> Option<InterfaceTypeView> {
        // An existential receiver carries its own view; `target_tn` may be the
        // interface itself or a `requires` parent (e.g. an `Iterator` value's
        // `Iterable` view).
        if let Tir2Ty::Interface(qtn, args, assoc, _) = actual_ty {
            return self
                .interface_closure_type_name_views(qtn, args, assoc)?
                .into_iter()
                .find(|(tn, _, _)| tn == target_tn);
        }
        for (name, generics, associated) in self.l1_impl_views_for_recv(actual_ty) {
            let Some(views) = self.interface_closure_type_name_views(&name, &generics, &associated)
            else {
                continue;
            };
            if let Some(view) = views.into_iter().find(|(tn, _, _)| tn == target_tn) {
                return Some(view);
            }
        }
        None
    }

    fn interface_view_for_tir_ty(
        &self,
        ty: &Tir2Ty,
        target_tn: &TypeName,
    ) -> Option<InterfaceTypeView> {
        match ty {
            Tir2Ty::Interface(qtn, args, assoc, _) => {
                let iface_tn = qtn.clone();
                self.interface_closure_type_name_views(&iface_tn, args, assoc)?
                    .into_iter()
                    .find(|(tn, _, _)| tn == target_tn)
            }
            Tir2Ty::Class(qtn, args, _) => self
                .interface_view_for_class_tir_ty(qtn, args, target_tn)
                .or_else(|| self.realized_interface_view_for(ty, target_tn)),
            Tir2Ty::TypeAlias(qtn, _) if !self.resolved_aliases.recursive.contains(qtn) => self
                .resolved_aliases
                .aliases
                .get(qtn)
                .and_then(|target| self.interface_view_for_tir_ty(target, target_tn))
                .or_else(|| self.realized_interface_view_for(ty, target_tn)),
            Tir2Ty::TypeVar(name, _) => self
                .generic_param_bounds
                .get(name)
                .into_iter()
                .flatten()
                .find_map(|bound| self.interface_view_for_tir_ty(&bound.to_ty(), target_tn)),
            Tir2Ty::AssociatedTypeProjection { .. } => {
                let resolved = self.resolve_ty_projections(ty);
                if &resolved != ty {
                    return self.interface_view_for_tir_ty(&resolved, target_tn);
                }
                self.resolve_projection_bounds(ty)
                    .iter()
                    .find_map(|bound| self.interface_view_for_tir_ty(bound, target_tn))
            }
            _ => self.realized_interface_view_for(ty, target_tn),
        }
    }

    fn iterable_view_for_tir_ty(&self, ty: &Tir2Ty) -> Option<InterfaceTypeView> {
        self.interface_view_for_tir_ty(ty, &Self::baml_iter_type_name("Iterable"))
    }

    fn lower_iterable_for_loop(
        &mut self,
        stmt_id: AstStmtId,
        binding: AstPatId,
        collection: AstExprId,
        body: AstExprId,
        iterable_view: InterfaceTypeView,
    ) {
        let saved_locals = self.locals.clone();
        let coll_ty = self.expr_ty(collection);
        let coll_local = self.builder.temp(coll_ty);
        self.lower_expr(collection, Place::local(coll_local));

        let (iterable_tn, iterable_args, iterable_assoc) = iterable_view;
        let item_tir_ty =
            Self::associated_binding_ty(&iterable_assoc, "Item").unwrap_or_else(|| {
                Tir2Ty::Unknown {
                    attr: TyAttr::default(),
                }
            });
        let elem_ty = self.convert_tir_ty_for_runtime(&item_tir_ty);

        let iterator_tn = Self::baml_iter_type_name("Iterator");
        let iter_method = Name::new("iter");
        let iter_local = self
            .builder
            .temp(self.convert_tir_ty_for_runtime(&Tir2Ty::Interface(
                Self::baml_iter_qtn("Iterator"),
                vec![],
                iterable_assoc.clone(),
                TyAttr::default(),
            )));
        // `collection.iter()`: open-world virtual dispatch on the collection's
        // `Iterable` view — the VM resolves the impl from the runtime concrete
        // type (containers included; array/map values carry their element types).
        self.emit_virtual_call(
            coll_local,
            &iterable_tn,
            &iterable_args,
            &iterable_assoc,
            &iter_method,
            body,
            &[],
            None,
            &Place::local(iter_local),
        );

        let bb_header = self.builder.create_block();
        let bb_body = self.builder.create_block();
        let bb_exit = self.builder.create_block();

        let prev_loop = self.loop_context.take();
        self.loop_context = Some(LoopContext {
            break_target: bb_exit,
            continue_target: bb_header,
            defer_depth: self.defer_stack.len(),
        });

        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_header);
        }

        self.builder.set_current_block(bb_header);
        let next_method = Name::new("next");
        let next_local = self.builder.temp(RuntimeTy::unknown());
        // `iterator.next()`: the iterator value's concrete adapter class resolves
        // its own `Iterator` impl at runtime.
        self.emit_virtual_call(
            iter_local,
            &iterator_tn,
            &[],
            &iterable_assoc,
            &next_method,
            body,
            &[],
            None,
            &Place::local(next_local),
        );
        self.emit_is_type_branch(next_local, Self::baml_iter_done_ty(), bb_exit, bb_body);

        self.builder.set_current_block(bb_body);
        let elem_local = self.builder.declare_local(None, elem_ty, None);
        self.builder.assign(
            Place::local(elem_local),
            Rvalue::Use(Operand::Copy(Place::Local(next_local))),
        );
        self.bind_pattern_with_fresh_cells(elem_local, binding);
        let names: Vec<Name> = self.body.patterns[binding]
            .bound_names(&self.body.patterns)
            .into_iter()
            .cloned()
            .collect();
        for name in names {
            if let Some(&local) = self.locals.get(&name)
                && let Some(binding_id) =
                    self.binding_id_for_statement_name(stmt_id, binding, &name)
            {
                self.binding_locals.insert(binding_id, local);
            }
        }

        let body_temp = self.builder.temp(RuntimeTy::Void {
            attr: TyAttr::default(),
        });
        self.lower_expr(body, Place::local(body_temp));

        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_header);
        }
        self.restore_locals_after_scope(saved_locals);

        self.loop_context = prev_loop;
        self.builder.set_current_block(bb_exit);
    }

    /// Populate `class_fields` and `enum_variants` from a single package's items.
    ///
    /// Note: `class_type_tags` is built separately via the project-wide
    /// [`class_type_tags_for_project`] query (tags are content-addressed, so
    /// they match the emitter's values without iteration-order coupling).
    fn populate_from_package(
        db: &'db dyn crate::Db,
        pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
        pkg_name: &Name,
        out: &mut PackagePopulation<'_>,
        resolved_aliases: &ResolvedAliases,
    ) {
        for (ns_names, ns) in &pkg_items.namespaces {
            // Build module_path: [pkg_name] ++ ns_names
            let mut module_path: Vec<Name> = vec![pkg_name.clone()];
            module_path.extend(ns_names.iter().cloned());

            for def in ns.types.values() {
                match def {
                    Definition::Class(class_loc) => {
                        let cfile = class_loc.file(db);
                        let class_data = baml_compiler2_ppir::item_data::class_data(db, *class_loc);

                        let class_qtn = QualifiedTypeName::new(
                            pkg_name.clone(),
                            ns_names.clone(),
                            class_data.name.clone(),
                        );
                        let tn = class_qtn.clone();

                        let mut fields = IndexMap::new();
                        let mut field_types = IndexMap::new();
                        let class_generic_params =
                            baml_compiler2_hir_ty::lower::class_generic_frame(db, *class_loc);
                        let pkg_ns = baml_compiler2_hir::file_package::file_package(db, cfile)
                            .namespace_path;
                        let mut idx_counter = 0usize;
                        let mut insert_field =
                            |name: &str,
                             type_ref: baml_compiler2_hir::type_ref::TypeRefId,
                             generic_params: &[ParamTy],
                             ns: &[Name],
                             fields: &mut IndexMap<String, usize>,
                             field_types: &mut IndexMap<String, RuntimeTy>|
                             -> Option<(usize, RuntimeTy)> {
                                if let Some(idx) = fields.get(name).copied() {
                                    return field_types.get(name).cloned().map(|ty| (idx, ty));
                                }
                                let idx = idx_counter;
                                fields.insert(name.to_string(), idx);
                                idx_counter += 1;
                                let tir_ty = lower_ref_in_scope(
                                    db,
                                    &class_data.type_refs,
                                    type_ref,
                                    pkg_items,
                                    ns,
                                    generic_params,
                                    &baml_compiler2_hir_ty::lower::class_generic_bounds(
                                        db, *class_loc,
                                    ),
                                    None,
                                );
                                let field_ty = resolved_aliases.convert(&tir_ty);
                                field_types.insert(name.to_string(), field_ty.clone());
                                Some((idx, field_ty))
                            };

                        for field in &class_data.fields {
                            insert_field(
                                field.name.as_str(),
                                field.type_ref,
                                &class_generic_params,
                                &pkg_ns,
                                &mut fields,
                                &mut field_types,
                            );
                        }
                        out.class_fields.insert(tn.clone(), fields);
                        out.class_field_types.insert(tn.clone(), field_types);
                    }
                    Definition::Enum(enum_loc) => {
                        let enum_data = baml_compiler2_ppir::item_data::enum_data(db, *enum_loc);
                        let enum_qtn = QualifiedTypeName::new(
                            pkg_name.clone(),
                            ns_names.clone(),
                            enum_data.name.clone(),
                        );

                        let mut variants = IndexMap::new();
                        for (idx, variant) in enum_data.variants.iter().enumerate() {
                            variants.insert(variant.name.to_string(), idx);
                        }
                        out.enum_variants.insert(enum_qtn, variants);
                    }
                    _ => {}
                }
            }
        }
    }

    /// Populate runtime schema maps from a source-less package interface.
    fn populate_from_mounted_package(
        mounted: &baml_compiler2_hir_ty::package_interface::PackageInterface,
        out: &mut PackagePopulation<'_>,
        resolved_aliases: &ResolvedAliases,
    ) {
        use baml_compiler2_hir_ty::package_interface::ExportedType;

        for types_in_ns in mounted.types.values() {
            for exported in types_in_ns.values() {
                match exported {
                    ExportedType::Class { qtn, fields, .. } => {
                        let mut field_indices = IndexMap::new();
                        let mut field_types = IndexMap::new();
                        for (idx, (name, ty, _attrs)) in fields.iter().enumerate() {
                            field_indices.insert(name.to_string(), idx);
                            field_types.insert(name.to_string(), resolved_aliases.convert(ty));
                        }
                        out.class_fields.insert(qtn.clone(), field_indices);
                        out.class_field_types.insert(qtn.clone(), field_types);
                    }
                    ExportedType::Enum { qtn, variants } => {
                        let mut variant_indices = IndexMap::new();
                        for (idx, variant) in variants.iter().enumerate() {
                            variant_indices.insert(variant.to_string(), idx);
                        }
                        out.enum_variants.insert(qtn.clone(), variant_indices);
                    }
                    ExportedType::Interface { .. } | ExportedType::TypeAlias { .. } => {}
                }
            }
        }
    }

    fn new(
        db: &'db dyn crate::Db,
        func_loc: FunctionLoc<'db>,
        expr_body: AstExprBody,
        source_map: Option<AstSourceMap>,
        opt: crate::OptLevel,
    ) -> Self {
        let file = func_loc.file(db);

        let func_data = baml_compiler2_ppir::item_data::function_data(db, func_loc);
        let index = file_semantic_index(db, file);
        // The scope this function opened, from the recorded item↔scope index.
        // Exact — no span match, so companion functions and synthesized `0..0`
        // functions (which the old scan special-cased) resolve correctly.
        let func_scope_id = baml_compiler2_ppir::item_data::function_scope(db, func_loc)
            .expect("every item-tree function has a recorded scope")
            .file_scope_id(db);

        // --- Collect per-scope TIR inference views (func + all descendants) ---
        // Borrows the Salsa-cached `infer_scope_types` results instead of
        // deep-copying every table into merged per-function maps (the old
        // scheme cloned the whole inference output of every function on each
        // construction). Lookups dispatch through the `tir_*` accessors.
        // Under the hir_ty provider this map stays EMPTY (TIR unconsulted);
        // the accessors read the converted tables instead.
        let tables = crate::inference_provider::ProviderTables::for_function(db, func_loc);

        // --- Build class_fields / enum_variants from PackageItems ---
        let pkg_info = file_package(db, file);
        let pkg_id = PackageId::new(db, pkg_info.package);
        // The per-param bound CONJUNCTIONS (the dispatch view), from hir_ty's
        // function_generic_bounds - the ONE declaration-bounds road (class
        // prefix, interface Self env with frame-pinned associated slots and
        // the Self bound, free-impl generics, own params). `T extends A & B`
        // keeps both conjuncts.
        let generic_param_bounds: FxHashMap<ParamTy, Vec<baml_type::Interface>> =
            baml_compiler2_hir_ty::lower::function_generic_bounds(db, func_loc)
                .into_iter()
                .map(|(param, conjunction)| {
                    (
                        param,
                        conjunction
                            .iter()
                            .filter_map(|bound| plain_interface_ty(bound).as_interface())
                            .collect(),
                    )
                })
                .collect();

        // Class/enum/interface schema + resolved aliases, memoized per package
        // (was rebuilt — and every class field re-lowered — per function).
        let pkg_data = package_lowering_data(db, pkg_id);

        // Tags are content-addressed over each class's fully-qualified name,
        // so they match the emitter's `class.type_tag` values by construction.
        // Memoized project-wide (was rebuilt here per lowered function).
        let class_type_tags = &class_type_tags_for_project(db, db.project()).tags;

        // --- Determine arity from function signature ---
        let sig = baml_compiler2_ppir::function_signature(db, func_loc);
        let arity = sig.params.len();

        // Detect if this function is a class method by checking the parent scope.
        // If so, qualify the function name as "ClassName.MethodName".
        let func_scope = &index.scopes[func_scope_id.index() as usize];
        let func_name = if let Some(parent_idx) = func_scope.parent {
            let parent = &index.scopes[parent_idx.index() as usize];
            if matches!(parent.kind, baml_compiler2_hir::scope::ScopeKind::Class) {
                if let Some(ref class_name) = parent.name {
                    Name::new(format!(
                        "{}.{}",
                        class_name.as_str(),
                        func_data.name.as_str()
                    ))
                } else {
                    func_data.name.clone()
                }
            } else {
                func_data.name.clone()
            }
        } else {
            func_data.name.clone()
        };

        LoweringContext {
            db,
            builder: MirBuilder::new(func_name, arity),
            locals: HashMap::new(),
            binding_locals: HashMap::new(),
            loop_context: None,
            catch_context: None,
            catch_rethrow_locals: Vec::new(),
            exit_block: BlockId(0), // placeholder; overwritten in lower_function_body
            tables,
            generic_param_bounds,
            lambda_param_tir_types: FxHashMap::default(),
            match_scrutinee: None,
            tested_pattern_values: HashMap::new(),
            atomic_pattern_test: false,
            current_scope: func_scope_id,
            current_metadata_scope: MetadataScope::Body(func_scope_id),
            body: expr_body,
            source_map,
            file,
            func_loc: Some(func_loc),
            source_param_scope: Some(func_scope_id),
            scope_func_name: Some(func_data.name.clone()),
            class_fields: &pkg_data.class_fields,
            class_field_types: &pkg_data.class_field_types,
            enum_variants: &pkg_data.enum_variants,
            class_type_tags,
            pending_lambdas: Vec::new(),
            lambda_generic_params: Vec::new(),
            runtime_type_binding_params: Vec::new(),
            capture_indices: None,
            transitive_captures_needed: Vec::new(),
            tagged_body_param_bindings: HashMap::new(),
            resolved_aliases: &pkg_data.resolved_aliases,
            interface_method_names: &pkg_data.interface_method_names,
            defer_stack: Vec::new(),
            synthetic_name_counts: HashMap::new(),
            chain_null_exits: Vec::new(),
            opt,
        }
    }

    /// Create a lowering context for a top-level let binding.
    ///
    /// The let binding has no parameters — arity 0, no `func_loc`.
    /// Type information is gathered from the `ScopeKind::Let` scope.
    fn new_for_let(
        db: &'db dyn crate::Db,
        let_loc: LetLoc<'db>,
        expr_body: AstExprBody,
        source_map: Option<AstSourceMap>,
        opt: crate::OptLevel,
    ) -> Self {
        let file = let_loc.file(db);

        let let_name = baml_compiler2_ppir::item_data::let_data(db, let_loc)
            .name
            .clone();
        let let_scope_id = baml_compiler2_ppir::item_data::let_scope(db, let_loc)
            .expect("every item-tree let has a recorded scope")
            .file_scope_id(db);

        // --- Collect per-scope TIR inference views (let + all descendants) ---
        // Borrows the Salsa-cached `infer_scope_types` results instead of
        // deep-copying every table into merged per-function maps (the old
        // scheme cloned the whole inference output of every let initializer on each
        // construction). Lookups dispatch through the `tir_*` accessors.
        // Under the hir_ty provider this map stays EMPTY (TIR unconsulted).
        let tables = crate::inference_provider::ProviderTables::for_let(db, let_loc);

        // --- Build class_fields / enum_variants from PackageItems ---
        let pkg_id = PackageId::new(db, file_package(db, file).package);

        // Class/enum/interface schema + resolved aliases, memoized per package
        // (was rebuilt — and every class field re-lowered — per let binding).
        let pkg_data = package_lowering_data(db, pkg_id);

        // Tags are content-addressed over each class's fully-qualified name,
        // so they match the emitter's `class.type_tag` values by construction.
        // Memoized project-wide (was rebuilt here per lowered function).
        let class_type_tags = &class_type_tags_for_project(db, db.project()).tags;

        LoweringContext {
            db,
            builder: MirBuilder::new(let_name.clone(), 0),
            locals: HashMap::new(),
            binding_locals: HashMap::new(),
            loop_context: None,
            catch_context: None,
            catch_rethrow_locals: Vec::new(),
            exit_block: BlockId(0), // placeholder; overwritten in lower_let_body_inner
            tables,
            generic_param_bounds: FxHashMap::default(),
            lambda_param_tir_types: FxHashMap::default(),
            match_scrutinee: None,
            tested_pattern_values: HashMap::new(),
            atomic_pattern_test: false,
            current_scope: let_scope_id,
            current_metadata_scope: MetadataScope::Body(let_scope_id),
            body: expr_body,
            source_map,
            file,
            func_loc: None,
            source_param_scope: None,
            scope_func_name: Some(let_name),
            class_fields: &pkg_data.class_fields,
            class_field_types: &pkg_data.class_field_types,
            enum_variants: &pkg_data.enum_variants,
            class_type_tags,
            resolved_aliases: &pkg_data.resolved_aliases,
            interface_method_names: &pkg_data.interface_method_names,
            defer_stack: Vec::new(),
            synthetic_name_counts: HashMap::new(),
            pending_lambdas: Vec::new(),
            lambda_generic_params: Vec::new(),
            runtime_type_binding_params: Vec::new(),
            capture_indices: None,
            transitive_captures_needed: Vec::new(),
            tagged_body_param_bindings: HashMap::new(),
            chain_null_exits: Vec::new(),
            opt,
        }
    }

    fn scope_is_descendant_or_self(
        index: &baml_compiler2_hir::semantic_index::FileSemanticIndex<'_>,
        scope_id: FileScopeId,
        ancestor_id: FileScopeId,
    ) -> bool {
        let mut current = Some(scope_id);
        while let Some(id) = current {
            if id == ancestor_id {
                return true;
            }
            current = index.scopes[id.index() as usize].parent;
        }
        false
    }

    fn binding_id_for_pattern_site_name(
        &self,
        pattern: AstPatId,
        site: DefinitionSite,
        name: &Name,
    ) -> Option<BindingId> {
        let index = file_semantic_index(self.db, self.file);
        let pattern_span = self
            .source_map
            .as_ref()
            .map(|source_map| source_map.pattern_span(pattern));

        for (scope_idx, bindings) in index.scope_bindings.iter().enumerate() {
            let scope_id = FileScopeId::new(u32::try_from(scope_idx).expect("scope id overflow"));
            if !Self::scope_is_descendant_or_self(index, scope_id, self.current_scope) {
                continue;
            }
            for (binding_idx, binding) in bindings.bindings.iter().enumerate() {
                let pattern_matches_name_range = pattern_span.is_none_or(|span| {
                    span == binding.name_range
                        || (span.start() <= binding.name_range.start()
                            && binding.name_range.end() <= span.end())
                });
                if binding.site == site
                    && binding.pattern == pattern
                    && binding.name == *name
                    && pattern_matches_name_range
                {
                    return Some(BindingId::local(scope_id, binding_idx));
                }
            }
        }
        None
    }

    fn any_pattern_binding_is_captured(&self, pattern: AstPatId, site: DefinitionSite) -> bool {
        let index = file_semantic_index(self.db, self.file);
        for (scope_idx, bindings) in index.scope_bindings.iter().enumerate() {
            let scope_id = FileScopeId::new(u32::try_from(scope_idx).expect("scope id overflow"));
            if !Self::scope_is_descendant_or_self(index, scope_id, self.current_scope) {
                continue;
            }
            for (binding_idx, binding) in bindings.bindings.iter().enumerate() {
                if binding.site == site && binding.pattern == pattern {
                    let binding_id = BindingId::local(scope_id, binding_idx);
                    if bindings.captured_bindings.contains(&binding_id) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn binding_id_for_statement_name(
        &self,
        stmt_id: AstStmtId,
        pattern: AstPatId,
        name: &Name,
    ) -> Option<BindingId> {
        self.binding_id_for_pattern_site_name(pattern, DefinitionSite::Statement(stmt_id), name)
    }

    fn record_pattern_binding_local(&mut self, pattern: AstPatId, name: &Name, local: Local) {
        if let Some(binding_id) = self.binding_id_for_pattern_site_name(
            pattern,
            DefinitionSite::PatternBinding(pattern),
            name,
        ) {
            self.binding_locals.insert(binding_id, local);
        }
    }

    // Catch-clause bindings (`catch (e, ctx)`) are registered in the semantic
    // index under `DefinitionSite::CatchBinding`, not `PatternBinding`, so the
    // catch-lowering paths must query with the matching site.
    fn record_catch_binding_local(&mut self, pattern: AstPatId, name: &Name, local: Local) {
        if let Some(binding_id) = self.binding_id_for_pattern_site_name(
            pattern,
            DefinitionSite::CatchBinding(pattern),
            name,
        ) {
            self.binding_locals.insert(binding_id, local);
        }
    }

    fn catch_binding_is_captured(&self, pattern: AstPatId) -> bool {
        self.any_pattern_binding_is_captured(pattern, DefinitionSite::CatchBinding(pattern))
    }

    fn path_resolution(&self, expr_id: AstExprId) -> Option<PathResolution> {
        let index = file_semantic_index(self.db, self.file);
        index.path_resolution(self.expr_metadata_key(expr_id))
    }

    fn hir_binding_id_for_path(&self, expr_id: AstExprId) -> Option<BindingId> {
        match self.path_resolution(expr_id)? {
            PathResolution::Local(binding_id) => Some(binding_id),
            PathResolution::Unknown => None,
        }
    }

    fn binding_id_for_path(&self, expr_id: AstExprId, name: &Name) -> Option<BindingId> {
        let hir_binding = self.hir_binding_id_for_path(expr_id);
        let Some(&tagged_binding) = self.tagged_body_param_bindings.get(name) else {
            return hir_binding;
        };
        let Some(hir_binding) = hir_binding else {
            return Some(tagged_binding);
        };

        // Tagged-template body parameters are synthetic MIR bindings. Prefer a
        // HIR binding only when it is inside the tagged body scope, where it
        // represents a real lexical shadow.
        let index = file_semantic_index(self.db, self.file);
        if index
            .ancestor_scopes(hir_binding.scope)
            .contains(&tagged_binding.scope)
        {
            Some(hir_binding)
        } else {
            Some(tagged_binding)
        }
    }

    fn local_for_path(&self, expr_id: AstExprId, name: &Name) -> Option<Local> {
        let binding_id = self.binding_id_for_path(expr_id, name)?;
        self.binding_locals.get(&binding_id).copied()
    }

    fn place_for_path(&mut self, expr_id: AstExprId, name: &Name) -> Option<Place> {
        let binding_id = self.binding_id_for_path(expr_id, name)?;
        if let Some(&local) = self.binding_locals.get(&binding_id) {
            return Some(Place::Local(local));
        }
        if let Some(capture) = self
            .capture_indices
            .as_ref()
            .and_then(|captures| captures.get(&binding_id).copied())
        {
            return Some(Place::Capture(capture));
        }
        if self.tagged_body_param_bindings.get(name) == Some(&binding_id) {
            return Some(Place::Capture(self.ensure_transitive_capture(binding_id)));
        }
        None
    }

    /// Return the current lambda's capture index for `binding_id`, allocating a
    /// fresh one (and signalling the parent to forward it via
    /// `transitive_captures_needed`) when it isn't captured yet. Mirrors the
    /// transitive-capture branch of `lower_lambda`'s capture-operand loop, for
    /// callers that discover a needed capture while lowering an expression
    /// (e.g. a tagged-body param referenced from a nested lambda).
    fn ensure_transitive_capture(&mut self, binding_id: BindingId) -> usize {
        if let Some(idx) = self
            .capture_indices
            .as_ref()
            .and_then(|m| m.get(&binding_id).copied())
        {
            return idx;
        }
        let idx = {
            let ci = self.capture_indices.get_or_insert_with(HashMap::new);
            let idx = ci.len();
            ci.insert(binding_id, idx);
            idx
        };
        self.transitive_captures_needed.push(binding_id);
        idx
    }

    /// Re-lower the `defer` block bodies registered at `[defer_depth..]` of
    /// `defer_stack`, in reverse declaration order (LIFO) — BEP-042.
    ///
    /// Each body is re-lowered INLINE (block-duplication) into a throwaway Void
    /// temp so it reads the live enclosing locals at THIS exit point, per the
    /// BEP's "final value" rule. Called at every scope exit. It does not truncate
    /// the stack; the owning `lower_scoped_block` truncates, while divergent callers leave it
    /// (a dead block follows). If a replayed body diverges (e.g. `throw`), the
    /// remaining defers are emitted on the resulting dead block and eliminated.
    fn replay_defers_to_depth(&mut self, defer_depth: usize) {
        let defers: Vec<AstExprId> = self.defer_stack[defer_depth..].to_vec();
        if defers.is_empty() {
            return;
        }
        // Inline replay (the non-throwing exits) runs the defers OUTSIDE their
        // own scope's unwind pads: a defer that throws here must not be routed
        // back into the pad that would replay it again (double-run / loop).
        // Clearing the catch context makes such a throw propagate outward
        // (replace-semantics; no cause chain in this pass). Restored after.
        let saved_catch = self.catch_context.take();
        for body in defers.into_iter().rev() {
            if self.builder.is_current_terminated() {
                break;
            }
            let tmp = self.builder.temp(RuntimeTy::Void {
                attr: TyAttr::default(),
            });
            self.lower_expr(body, Place::local(tmp));
        }
        self.catch_context = saved_catch;
    }

    fn restore_locals_after_scope(&mut self, saved_locals: HashMap<Name, Local>) {
        self.locals = saved_locals;
    }

    fn restore_active_locals(&mut self, saved_locals: HashMap<Name, Local>) {
        self.locals = saved_locals;
    }

    fn mark_captured_locals_in_scope_tree(&mut self, root_scope: FileScopeId) {
        let index = file_semantic_index(self.db, self.file);
        let root = &index.scopes[root_scope.index() as usize];
        let start = root_scope.index();
        let end = root.descendants.end.index();

        for raw_idx in start..end {
            let scope_id = FileScopeId::new(raw_idx);
            let Some(scope_bindings) = index.scope_bindings.get(scope_id.index() as usize) else {
                continue;
            };
            for binding_id in &scope_bindings.captured_bindings {
                if let Some(&local) = self.binding_locals.get(binding_id) {
                    self.builder.local_decl_mut(local).is_captured = true;
                }
            }
        }
    }

    /// Get the `baml_type::RuntimeTy` for an expression by looking up in the aggregated map
    /// and converting from TIR `Ty`. Uses `current_metadata_scope` as the arena namespace.
    fn expr_metadata_key(&self, expr_id: AstExprId) -> ExprMetadataKey {
        ExprMetadataKey::new(self.current_metadata_scope, expr_id)
    }

    fn pat_metadata_key(&self, pat_id: AstPatId) -> PatMetadataKey {
        (self.current_metadata_scope, pat_id)
    }

    // --- Inference views ---
    //
    // Point lookups into the one converted table store (MIR's own
    // consumption vocabulary, built at construction from whichever engine
    // backs this run). `MetadataScope::Body` reads a scope's body tables;
    // `MetadataScope::ParameterDefault` its default-parameter tables.

    fn tir_expr_type(&self, key: ExprMetadataKey) -> Option<&Tir2Ty> {
        self.tables.for_scope(key.scope).expr_type(key.expr)
    }

    fn tir_pat_type(&self, key: PatMetadataKey) -> Option<&Tir2Ty> {
        self.tables.for_scope(key.0).pat_type(key.1)
    }

    fn tir_resolution(
        &self,
        key: ExprMetadataKey,
    ) -> Option<&crate::inference_provider::MemberResolution<'db>> {
        self.tables.for_scope(key.scope).resolution(key.expr)
    }

    /// The recorded resolution for a virtual interface-field access: the realized
    /// declaring-interface view plus the field's index in it.
    ///
    /// Authoritative, and preferred over re-deriving a view from the receiver's type.
    /// It is what the type checker actually resolved through — which is the only
    /// thing that answers a *union* receiver, where the serving interface is the one
    /// every arm shares and is not recoverable from the receiver type alone.
    fn tir_virtual_field_view(&self, key: ExprMetadataKey) -> Option<(InterfaceTypeView, u32)> {
        use crate::inference_provider::MemberResolution;
        let (interface, field_index) = match self.tir_resolution(key)? {
            MemberResolution::InterfaceVirtualField {
                interface,
                field_index,
                ..
            }
            | MemberResolution::ExternalInterfaceVirtualField {
                interface,
                field_index,
                ..
            } => (interface, *field_index),
            _ => return None,
        };
        let Tir2Ty::Interface(tn, args, assoc, _) = interface else {
            return None;
        };
        Some(((tn.clone(), args.clone(), assoc.clone()), field_index))
    }

    fn tir_is_exhaustive_match(&self, key: ExprMetadataKey) -> bool {
        self.tables
            .for_scope(key.scope)
            .is_exhaustive_match(key.expr)
    }

    fn tir_path_root_type(&self, key: ExprMetadataKey) -> Option<&Tir2Ty> {
        self.tables.for_scope(key.scope).path_root_type(key.expr)
    }

    fn tir_path_segment_type(&self, key: (MetadataScope, AstExprId, usize)) -> Option<&Tir2Ty> {
        self.tables.for_scope(key.0).path_segment_type(key.1, key.2)
    }

    fn tir_path_member_resolutions(
        &self,
        key: ExprMetadataKey,
    ) -> Option<&[crate::inference_provider::MemberResolution<'db>]> {
        self.tables
            .for_scope(key.scope)
            .path_member_resolutions(key.expr)
    }

    fn tir_call_plan(&self, key: ExprMetadataKey) -> Option<&crate::inference_provider::CallPlan> {
        self.tables.for_scope(key.scope).call_plan(key.expr)
    }

    fn tir_type_binding(
        &self,
        stmt: AstStmtId,
    ) -> Option<&crate::inference_provider::ScopedTypeBinding> {
        self.tables
            .for_scope(self.current_metadata_scope)
            .type_binding(stmt)
    }

    fn tir_function_coercion(
        &self,
        key: ExprMetadataKey,
    ) -> Option<&crate::inference_provider::FunctionCoercion> {
        self.tables.for_scope(key.scope).function_coercion(key.expr)
    }

    fn convert_tir_ty_for_runtime(&self, ty: &Tir2Ty) -> RuntimeTy {
        // Resolve associated-type projections against the bounds the compiler
        // knows statically; anything still symbolic — a `TypeVar` or a
        // projection off one — is kept faithfully so the runtime can resolve it
        // from the receiver's actual type. We deliberately do *not* erase type
        // variables: `RuntimeTy` carries them, and erasing to `unknown` would
        // throw away the information needed to resolve the type at run time.
        let resolved = self.resolve_ty_projections(ty);
        let runtime_ready = Self::erase_compiler_only_ty(resolved);
        self.resolved_aliases.convert(&runtime_ready)
    }

    fn erase_compiler_only_ty(ty: Tir2Ty) -> Tir2Ty {
        match ty {
            Tir2Ty::Unknown { attr } | Tir2Ty::Error { attr } => Tir2Ty::BuiltinUnknown { attr },
            Tir2Ty::TypeVar(param, attr) if baml_type::is_synthetic_effect_param(param.name()) => {
                Tir2Ty::BuiltinUnknown { attr }
            }
            Tir2Ty::EvolvingList(inner, attr) => {
                Tir2Ty::List(Box::new(Self::erase_compiler_only_ty(*inner)), attr)
            }
            Tir2Ty::EvolvingMap(key, value, attr) => Tir2Ty::Map {
                key: Box::new(Self::erase_compiler_only_ty(*key)),
                value: Box::new(Self::erase_compiler_only_ty(*value)),
                attr,
            },
            Tir2Ty::Literal(lit, _freshness, attr) => {
                Tir2Ty::Literal(lit, baml_type::Freshness::Regular, attr)
            }
            Tir2Ty::Class(name, args, attr) => Tir2Ty::Class(
                name,
                args.into_iter().map(Self::erase_compiler_only_ty).collect(),
                attr,
            ),
            Tir2Ty::Interface(name, args, bindings, attr) => Tir2Ty::Interface(
                name,
                args.into_iter().map(Self::erase_compiler_only_ty).collect(),
                bindings
                    .into_iter()
                    .map(|(name, ty)| (name, Self::erase_compiler_only_ty(ty)))
                    .collect(),
                attr,
            ),
            Tir2Ty::List(inner, attr) => {
                Tir2Ty::List(Box::new(Self::erase_compiler_only_ty(*inner)), attr)
            }
            Tir2Ty::Map { key, value, attr } => Tir2Ty::Map {
                key: Box::new(Self::erase_compiler_only_ty(*key)),
                value: Box::new(Self::erase_compiler_only_ty(*value)),
                attr,
            },
            Tir2Ty::Union(types, attr) => Tir2Ty::Union(
                types
                    .into_iter()
                    .map(Self::erase_compiler_only_ty)
                    .collect(),
                attr,
            ),
            Tir2Ty::Function {
                params,
                ret,
                throws,
                attr,
            } => Tir2Ty::Function {
                params: params
                    .into_iter()
                    .map(|param| Tir2FunctionParamTy {
                        name: param.name,
                        ty: Self::erase_compiler_only_ty(param.ty),
                        mode: param.mode,
                    })
                    .collect(),
                ret: Box::new(Self::erase_compiler_only_ty(*ret)),
                throws: Box::new(Self::erase_compiler_only_ty(*throws)),
                attr,
            },
            Tir2Ty::Future(value, error, attr) => Tir2Ty::Future(
                Box::new(Self::erase_compiler_only_ty(*value)),
                Box::new(Self::erase_compiler_only_ty(*error)),
                attr,
            ),
            Tir2Ty::AssociatedTypeProjection {
                base,
                interface,
                member,
                attr,
            } => Tir2Ty::AssociatedTypeProjection {
                base: Box::new(Self::erase_compiler_only_ty(*base)),
                // The interface annotation carries component types (generics and
                // associated-type bindings); erase compiler-only types within them
                // too, matching the `Tir2Ty::Interface` arm above. (The field is an
                // `Interface` after the interface-object refactor, not a `Ty`.)
                interface: Box::new(
                    interface.map_tys(|ty| Self::erase_compiler_only_ty(ty.clone())),
                ),
                member,
                attr,
            },
            other => other,
        }
    }

    /// Lower a method-signature type expression (a parameter or return type) to
    /// a runtime type. In a method signature `Self` is the receiver type
    /// variable and `Self.Assoc` is an associated-type projection onto it. A bare
    /// `lower_type_expr_in_ns` has neither in scope and would erase both to
    /// `Ty::Unknown`, tripping the runtime lowering boundary — so bind `Self` to
    /// its rigid type variable through the `self_ty` channel, which roots both a
    /// bare `Self` and each `Self.Assoc` projection at it.
    fn lower_signature_runtime_ty(
        &self,
        te: &baml_compiler2_ast::TypeExpr,
        pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
        ns_context: &[baml_base::Name],
    ) -> RuntimeTy {
        let mut generic_params = self.enclosing_generic_params();
        let self_param = generic_params
            .iter()
            .find(|param| param.as_str() == "Self")
            .cloned()
            .unwrap_or_else(|| {
                let index = generic_params
                    .iter()
                    .map(ParamTy::index)
                    .max()
                    .map_or(0, |index| index + 1);
                let param = ParamTy::new(index, Name::new("Self"));
                generic_params.push(param.clone());
                param
            });
        let generic_param_bounds = self.enclosing_generic_param_bounds();
        let tir_ty = lower_expr_in_scope(
            self.db,
            te,
            pkg_items,
            ns_context,
            &generic_params,
            &generic_param_bounds,
            Some(Tir2Ty::TypeVar(self_param, TyAttr::default())),
        );
        self.convert_tir_ty_for_runtime(&tir_ty)
    }

    /// The interface *view* a receiver of this static type dispatches through, for
    /// the `member` being accessed — its own for an existential, its bound's for a
    /// type variable, its resolved bound's for a projection. `None` for concrete
    /// receivers: their providing interface is resolved by
    /// [`Self::dispatch_target_for_concrete`].
    ///
    /// `member` is not optional: a dispatch view exists only to key a member access,
    /// and which view is correct depends on the member. A bound list is a
    /// *conjunction*, so under `T extends A & B` a member may be declared by either
    /// conjunct while the emitted `virtual_call` names just one interface. Choosing
    /// without the member — as this did while a bound could only be a single
    /// interface — keys the call on an interface that need not declare it, which the
    /// VM then cannot resolve.
    fn interface_dispatch_target_for_member(
        &self,
        ty: &Tir2Ty,
        member: &Name,
    ) -> Option<InterfaceTypeView> {
        match ty {
            Tir2Ty::TypeAlias(qtn, _) if !self.resolved_aliases.recursive.contains(qtn) => self
                .resolved_aliases
                .aliases
                .get(qtn)
                .and_then(|target| self.interface_dispatch_target_for_member(target, member)),
            Tir2Ty::Union(members, _) => self.union_virtual_dispatch_view(members, member),
            Tir2Ty::Interface(qtn, type_args, associated_bindings, _) => {
                Some((qtn.clone(), type_args.clone(), associated_bindings.clone()))
            }
            // `T extends A & B` — the generic-parameter axis.
            Tir2Ty::TypeVar(name, _) => {
                let conjuncts: Vec<Tir2Ty> = self
                    .generic_param_bounds
                    .get(name)
                    .into_iter()
                    .flatten()
                    .map(baml_type::Interface::to_ty)
                    .collect();
                self.dispatch_view_over_conjunction(&conjuncts, member)
            }
            // `type Item extends A & B` — the associated-type axis, same rule.
            Tir2Ty::AssociatedTypeProjection { .. } => {
                let resolved = self.resolve_ty_projections(ty);
                if &resolved != ty {
                    return self.interface_dispatch_target_for_member(&resolved, member);
                }
                self.dispatch_view_over_conjunction(&self.resolve_projection_bounds(ty), member)
            }
            _ => None,
        }
    }

    /// Pick which conjunct of a bound list a `member` access dispatches through: the
    /// first whose `requires` closure declares it.
    ///
    /// Falls back to the first conjunct that yields a view at all when none declares
    /// `member` — an access TIR has already rejected, so the choice only shapes the
    /// code emitted for a program that will not run.
    ///
    /// Shared by both places a bound list is a conjunction — a generic parameter's
    /// `T extends A & B` and an associated type's `type Item extends A & B` — so the
    /// two cannot drift.
    fn dispatch_view_over_conjunction(
        &self,
        bounds: &[Tir2Ty],
        member: &Name,
    ) -> Option<InterfaceTypeView> {
        bounds
            .iter()
            .find_map(|bound| {
                let view = self.interface_dispatch_target_for_member(bound, member)?;
                self.interface_closure_declares_member(&view.0, member)
                    .then_some(view)
            })
            .or_else(|| {
                bounds
                    .iter()
                    .find_map(|bound| self.interface_dispatch_target_for_member(bound, member))
            })
    }

    /// Whether `iface_tn`'s `requires` closure declares `member`, as either a method
    /// or a field. Selects which conjunct of a bound list a member access dispatches
    /// through.
    fn interface_closure_declares_member(&self, iface_tn: &TypeName, member: &Name) -> bool {
        if self.mir_interface_declares_method(iface_tn, member) {
            return true;
        }
        self.interface_closure_type_name_views(iface_tn, &[], &[])
            .is_some_and(|views| {
                views
                    .iter()
                    .any(|(tn, _, _)| self.interface_field_index_directly(tn, member).is_some())
            })
    }

    /// The interface view an *expression* receiver dispatches through for `member`
    /// — see [`Self::interface_dispatch_target_for_member`].
    fn interface_dispatch_target_for_expr_member(
        &self,
        expr_id: AstExprId,
        member: &Name,
    ) -> Option<InterfaceTypeView> {
        self.source_param_interface_view_for_expr(expr_id, member)
            .or_else(|| {
                self.tir_expr_type(self.expr_metadata_key(expr_id))
                    .and_then(|ty| self.interface_dispatch_target_for_member(ty, member))
            })
            .or_else(|| {
                self.self_typevar_for_expr(expr_id)
                    .and_then(|ty| self.interface_dispatch_target_for_member(&ty, member))
            })
            .or_else(|| self.upcast_target_interface_view(expr_id, member))
    }

    /// The interface view used to lower a member-access expression. Optional
    /// member access has already guarded the null branch before the member is
    /// evaluated, so dispatch must use the non-null receiver type.
    fn dispatch_target_for_member_access(
        &self,
        access: AstExprId,
        base: AstExprId,
        member: &Name,
    ) -> Option<InterfaceTypeView> {
        let narrowed = if matches!(
            &self.body.exprs[access],
            AstExpr::OptionalMemberAccess { .. }
        ) {
            self.tir_expr_type(self.expr_metadata_key(base))
                .map(Tir2Ty::strip_null)
        } else {
            None
        };

        narrowed
            .as_ref()
            .and_then(|ty| {
                self.interface_dispatch_target_for_member(ty, member)
                    .or_else(|| self.dispatch_target_for_concrete(ty, member))
            })
            .or_else(|| self.interface_dispatch_target_for_expr_member(base, member))
            .or_else(|| {
                self.tir_expr_type(self.expr_metadata_key(base))
                    .and_then(|ty| self.dispatch_target_for_concrete(ty, member))
            })
    }

    fn source_param_interface_view_for_expr(
        &self,
        expr_id: AstExprId,
        member: &Name,
    ) -> Option<InterfaceTypeView> {
        let AstExpr::Path(segments) = &self.body.exprs[expr_id] else {
            return None;
        };
        if segments.len() != 1 {
            return None;
        }
        self.source_param_interface_view_for_name_at(expr_id, &segments[0], member)
    }

    fn source_param_interface_view_for_name_at(
        &self,
        expr_id: AstExprId,
        name: &Name,
        member: &Name,
    ) -> Option<InterfaceTypeView> {
        let binding_id = self.binding_id_for_path(expr_id, name)?;
        self.source_param_interface_view_for_binding(name, binding_id, member)
    }

    fn source_param_interface_view_for_binding(
        &self,
        name: &Name,
        binding_id: BindingId,
        member: &Name,
    ) -> Option<InterfaceTypeView> {
        let ty = self.source_param_tir_ty_for_binding(name, binding_id)?;
        self.interface_dispatch_target_for_member(&ty, member)
    }

    fn source_param_tir_ty_for_binding(
        &self,
        name: &Name,
        binding_id: BindingId,
    ) -> Option<Tir2Ty> {
        let func_loc = self.func_loc?;
        let param_scope = self.source_param_scope?;
        let sig = baml_compiler2_ppir::function_signature(self.db, func_loc);
        let (param_idx, param) = sig
            .params
            .iter()
            .enumerate()
            .find(|(_, param)| param.name == *name)?;
        if binding_id != BindingId::parameter(param_scope, param_idx) {
            return None;
        }

        let pkg_info = file_package(self.db, self.file);
        let pkg_id = PackageId::new(self.db, pkg_info.package);
        let pkg_items = package_items(self.db, pkg_id);
        let generic_params = self.enclosing_generic_params();
        let bindings: FxHashMap<ParamTy, Tir2Ty> = generic_params
            .iter()
            .map(|param| {
                (
                    param.clone(),
                    Tir2Ty::TypeVar(param.clone(), TyAttr::default()),
                )
            })
            .collect();
        let bounds = self.enclosing_generic_param_bounds();
        Some(lower_ty_with_bindings(
            self.db,
            &param.ty,
            pkg_items,
            &pkg_info.namespace_path,
            &bindings,
            &bounds,
        ))
    }

    fn self_typevar_for_expr(&self, expr_id: AstExprId) -> Option<Tir2Ty> {
        let AstExpr::Path(segments) = &self.body.exprs[expr_id] else {
            return None;
        };
        if segments.len() == 1
            && segments[0].as_str() == "self"
            && let Some(self_param) = self
                .enclosing_generic_params()
                .into_iter()
                .find(|param| param.as_str() == "Self")
            && self.generic_param_bounds.contains_key(&self_param)
        {
            Some(Tir2Ty::TypeVar(self_param, baml_type::TyAttr::default()))
        } else {
            None
        }
    }

    fn upcast_target_interface_view(
        &self,
        expr_id: AstExprId,
        member: &Name,
    ) -> Option<InterfaceTypeView> {
        let AstExpr::Upcast { target, .. } = &self.body.exprs[expr_id] else {
            return None;
        };
        let pkg_info = baml_compiler2_hir::file_package::file_package(self.db, self.file);
        let pkg_id = baml_compiler2_hir::package::PackageId::new(self.db, pkg_info.package.clone());
        let pkg_items = baml_compiler2_hir::package::package_items(self.db, pkg_id);
        let generic_params = self.enclosing_generic_params();
        let generic_param_bounds = self.enclosing_generic_param_bounds();
        let target_ty = lower_expr_in_scope(
            self.db,
            target,
            pkg_items,
            &pkg_info.namespace_path,
            &generic_params,
            &generic_param_bounds,
            None,
        );
        self.interface_dispatch_target_for_member(&target_ty, member)
    }

    fn class_dispatch_target_for_tir_ty(&self, ty: &Tir2Ty) -> Option<(TypeName, Vec<RuntimeTy>)> {
        match ty {
            Tir2Ty::Class(qtn, type_args, _) => Some((
                qtn.clone(),
                type_args
                    .iter()
                    .map(|arg| self.convert_tir_ty_for_runtime(arg))
                    .collect(),
            )),
            Tir2Ty::AssociatedTypeProjection { .. } => {
                let resolved = self.resolve_ty_projections(ty);
                if &resolved != ty {
                    return self.class_dispatch_target_for_tir_ty(&resolved);
                }
                // An unreduced projection's bounds are interface constraints, and
                // an interface is never a class, so only the reduction above can
                // name one.
                None
            }
            // A type variable has no class dispatch target: its bounds are
            // interface constraints, and an interface is never a class.
            Tir2Ty::TypeVar(..) => None,
            _ => None,
        }
    }

    /// BEP-044 wf3 #G7: for a *concrete* receiver whose
    /// method is provided by a blanket / out-of-body `implements … for …` rule
    /// (not an in-body block), find the single interface that provides `method`
    /// so a direct `recv.method()` dispatches through the normal interface
    /// switch. Returns the interface view. TIR has already rejected the
    /// ambiguous (>1 interface) case with E0121, so the first declaring match
    /// is unambiguous for a compiling program.
    /// Every impl block applying to `recv` — a concrete-headed receiver, possibly
    /// carrying the enclosing function's rigid type variables — enumerated through
    /// the canonical L1 substrate, with each impl's generic bounds discharged by
    /// the canonical algebra against this scope's bounds.
    fn l1_impl_views_for_recv(&self, recv: &Tir2Ty) -> Vec<InterfaceTypeView> {
        // hir_ty's substrate: alias transparency and impl-bound discharge
        // are internal to it (its own facts). Dispatch is by BASE type
        // (hir_ty's operand-dispatch discipline) - a literal-typed
        // receiver probes as its base primitive; realized view members
        // reduce through the oracle before becoming dispatch types (the
        // requires-closure rule).
        let recv = widen_literal_bases(recv);
        let Some(interned) = baml_compiler2_hir_ty::impls::try_interned_ty(&recv) else {
            return Vec::new();
        };
        baml_compiler2_hir_ty::impls::impls_for_type(self.db, &interned)
            .into_iter()
            .map(|resolved| {
                let realized = resolved.implemented_view(self.db, &interned);
                (
                    realized.name.clone(),
                    realized
                        .generics
                        .iter()
                        .map(|ty| self.resolve_ty_projections(&ty.to_plain()))
                        .collect(),
                    realized
                        .associated_types
                        .iter()
                        .map(|(name, ty)| {
                            (name.clone(), self.resolve_ty_projections(&ty.to_plain()))
                        })
                        .collect(),
                )
            })
            .collect()
    }

    /// The interface a *concrete* receiver provides `method` through — the realized
    /// view of the impl whose interface (or its `requires` closure) declares it.
    /// `None` for non-concrete receivers (interfaces/type-vars take their view from
    /// the type itself) and for methods no impl provides.
    fn dispatch_target_for_concrete(
        &self,
        recv_ty: &Tir2Ty,
        method: &Name,
    ) -> Option<InterfaceTypeView> {
        // Only concrete receivers — interfaces/type-vars dispatch via the
        // arms above. Containers are concrete too (`implements<T> I for T[]`).
        if !matches!(
            recv_ty,
            Tir2Ty::Class(..)
                | Tir2Ty::Int { .. }
                | Tir2Ty::Bigint { .. }
                | Tir2Ty::Float { .. }
                | Tir2Ty::String { .. }
                | Tir2Ty::Bool { .. }
                | Tir2Ty::Null { .. }
                | Tir2Ty::Uint8Array { .. }
                | Tir2Ty::Media(..)
                | Tir2Ty::List(..)
                | Tir2Ty::Map { .. }
                | Tir2Ty::Future(..)
        ) {
            return None;
        }
        // Fast pre-filter: a member name no in-scope interface declares can
        // never dispatch through an impl, so skip the per-call impl enumeration
        // entirely. `l1_impl_views_for_recv` probes every impl block's pattern
        // against the receiver (each probe running alias normalization / subtype
        // checks via the canonical algebra), so this hash lookup collapses the
        // dominant "plain method / field access" case — where the accessed
        // member is not an interface method — to O(1). Correctness: the set is
        // the union of every method declared by any reachable interface, so any
        // name that COULD dispatch is still present and falls through to the
        // full enumeration + `mir_interface_declares_method` check below.
        if !self.interface_method_names.contains(method) {
            return None;
        }
        for (name, generics, associated) in self.l1_impl_views_for_recv(recv_ty) {
            if self.mir_interface_declares_method(&name, method) {
                return Some((name, generics, associated));
            }
        }
        None
    }

    /// Whether `iface_qtn` or any interface in its `requires` closure declares a
    /// method named `method`. Mirrors the TIR-side check; used by
    /// `dispatch_target_for_concrete`.
    fn mir_interface_declares_method(&self, iface_qtn: &QualifiedTypeName, method: &Name) -> bool {
        let pkg_id =
            baml_compiler2_hir::package::PackageId::new(self.db, iface_qtn.package().clone());
        let pkg_items = baml_compiler2_hir::package::package_items(self.db, pkg_id);
        let Some(baml_compiler2_hir::contributions::Definition::Interface(root_loc)) =
            pkg_items.lookup_type(iface_qtn.namespace(), iface_qtn.name())
        else {
            if let Some(baml_compiler2_hir_ty::package_interface::ExportedType::Interface {
                required_methods,
                default_methods,
                requires,
                ..
            }) = baml_compiler2_hir_ty::package_interface::mounted_type_row(self.db, iface_qtn)
            {
                return required_methods.iter().any(|m| m.name == *method)
                    || default_methods.iter().any(|m| m.name == *method)
                    || requires
                        .iter()
                        .any(|req| self.mir_interface_declares_method(&req.name, method));
            }
            return false;
        };
        interface_requires_closure_locs(self.db, root_loc)
            .into_iter()
            .any(|iface_loc| {
                use baml_compiler2_ppir::item_data::{function_data, interface_data};
                let iface_data = interface_data(self.db, iface_loc);
                iface_data
                    .required_methods
                    .iter()
                    .any(|s| s.name == *method)
                    || iface_data
                        .default_methods
                        .iter()
                        .any(|&fn_loc| function_data(self.db, fn_loc).name == *method)
            })
    }

    /// Whether `iface_tn` declares `method` *directly* — in its own required or
    /// default methods, not via its `requires` closure. (Unlike
    /// [`Self::mir_interface_declares_method`], which walks the whole closure.)
    /// Whether `tn` names an interface declaration.
    ///
    /// A direct question about the name, not a lookup in a set of known
    /// implementors: an interface with no implementors in this compilation is still
    /// an interface, and one with implementors elsewhere is not more of one.
    fn is_interface_type_name(&self, tn: &TypeName) -> bool {
        let pkg_id = baml_compiler2_hir::package::PackageId::new(self.db, tn.package().clone());
        let pkg_items = baml_compiler2_hir::package::package_items(self.db, pkg_id);
        matches!(
            pkg_items.lookup_type(tn.namespace(), tn.name()),
            Some(baml_compiler2_hir::contributions::Definition::Interface(_))
        ) || matches!(
            baml_compiler2_hir_ty::package_interface::mounted_type_row(self.db, tn),
            Some(baml_compiler2_hir_ty::package_interface::ExportedType::Interface { .. })
        )
    }

    fn interface_declares_method_directly(&self, iface_tn: &TypeName, method: &Name) -> bool {
        use baml_compiler2_ppir::item_data::{function_data, interface_data};
        let pkg_id =
            baml_compiler2_hir::package::PackageId::new(self.db, iface_tn.package().clone());
        let pkg_items = baml_compiler2_hir::package::package_items(self.db, pkg_id);
        let Some(baml_compiler2_hir::contributions::Definition::Interface(loc)) =
            pkg_items.lookup_type(iface_tn.namespace(), iface_tn.name())
        else {
            if let Some(baml_compiler2_hir_ty::package_interface::ExportedType::Interface {
                required_methods,
                default_methods,
                ..
            }) = baml_compiler2_hir_ty::package_interface::mounted_type_row(self.db, iface_tn)
            {
                return required_methods.iter().any(|m| m.name == *method)
                    || default_methods.iter().any(|m| m.name == *method);
            }
            return false;
        };
        let iface_data = interface_data(self.db, loc);
        iface_data
            .required_methods
            .iter()
            .any(|s| s.name == *method)
            || iface_data
                .default_methods
                .iter()
                .any(|&fn_loc| function_data(self.db, fn_loc).name == *method)
    }

    /// Resolve the interface view that actually *declares* `method`, starting
    /// from `view`'s interface and walking its `requires` closure.
    ///
    /// A method may be declared by a super-interface: `interface B requires A {}`
    /// with `tag` declared in `A`. A `B` value must implement `A` (the `requires`
    /// rule), so calling `tag` on a `B` receiver dispatches `<Self as A>::tag` —
    /// the open-world virtual call must be keyed on the *declaring* interface
    /// `A`, not the receiver's static interface `B` (the impl registry has no
    /// `tag` under `(Self, B)`). Coherence makes the concrete `(Self, A)`
    /// implementation unique.
    ///
    /// Prefers `view`'s own interface when it declares the method directly
    /// (BEP-044 method disambiguation: the receiver's interface picks its own
    /// version), then the nearest required ancestor. Falls back to `view`
    /// unchanged when nothing in the closure declares it.
    /// `field`'s position in `iface_tn`'s own declared field list — the index space
    /// every implementation's `RuntimeImplRule::field_links` is baked against.
    /// `None` when this interface does not itself declare the field.
    ///
    /// Reads the same `interface_data` query, in the same order, that the bake and
    /// TIR's `MemberResolution::InterfaceVirtualField` read, so the three cannot
    /// disagree about what index a field has.
    fn interface_field_index_directly(&self, iface_tn: &TypeName, field: &Name) -> Option<u32> {
        use baml_compiler2_ppir::item_data::interface_data;
        let pkg_id =
            baml_compiler2_hir::package::PackageId::new(self.db, iface_tn.package().clone());
        let pkg_items = baml_compiler2_hir::package::package_items(self.db, pkg_id);
        let Some(def) = pkg_items.lookup_type(iface_tn.namespace(), iface_tn.name()) else {
            if let Some(baml_compiler2_hir_ty::package_interface::ExportedType::Interface {
                fields,
                ..
            }) = baml_compiler2_hir_ty::package_interface::mounted_type_row(self.db, iface_tn)
            {
                let index = fields.iter().position(|(name, _, _)| name == field)?;
                return Some(u32::try_from(index).expect("interface field count fits u32"));
            }
            return None;
        };
        let baml_compiler2_hir::contributions::Definition::Interface(loc) = def else {
            return None;
        };
        let index = interface_data(self.db, loc)
            .fields
            .iter()
            .position(|f| f.name == *field)?;
        Some(u32::try_from(index).expect("interface field count fits u32"))
    }

    /// Narrow an interface view to the interface that *declares* `field`, paired with
    /// that field's index there. `requires` is a bound rather than inheritance, so a
    /// parent-declared field is served by the implementor's separate impl of the
    /// parent, at the parent's own index — never the child's numbering.
    ///
    /// The field counterpart of [`Self::interface_view_declaring_method`].
    fn interface_view_declaring_field(
        &self,
        view: &InterfaceTypeView,
        field: &Name,
    ) -> Option<(InterfaceTypeView, u32)> {
        if let Some(index) = self.interface_field_index_directly(&view.0, field) {
            return Some((view.clone(), index));
        }
        self.interface_closure_type_name_views(&view.0, &view.1, &view.2)?
            .into_iter()
            .find_map(|v| {
                let index = self.interface_field_index_directly(&v.0, field)?;
                Some((v, index))
            })
    }

    fn interface_view_declaring_method(
        &self,
        view: &InterfaceTypeView,
        method: &Name,
    ) -> InterfaceTypeView {
        if self.interface_declares_method_directly(&view.0, method) {
            return view.clone();
        }
        self.interface_closure_type_name_views(&view.0, &view.1, &view.2)
            .and_then(|views| {
                views
                    .into_iter()
                    .find(|(tn, _, _)| self.interface_declares_method_directly(tn, method))
            })
            .unwrap_or_else(|| view.clone())
    }

    fn expr_ty(&self, expr_id: AstExprId) -> RuntimeTy {
        self.tir_expr_type(self.expr_metadata_key(expr_id))
            .map(|ty| self.convert_tir_ty_for_runtime(ty))
            .unwrap_or(RuntimeTy::Void {
                attr: TyAttr::default(),
            })
    }

    /// Compute the `TyTemplate` slice for the class-level type args of a class
    /// construction expression.
    ///
    /// Returns `vec![]` for non-generic (or unresolved) classes.
    fn class_type_arg_templates(&self, expr_id: AstExprId) -> Vec<TyTemplate> {
        let generic_params = self.enclosing_generic_params();
        match self.tir_expr_type(self.expr_metadata_key(expr_id)) {
            Some(Tir2Ty::Class(_, type_args, _)) if !type_args.is_empty() => type_args
                .iter()
                .map(|t| self.ty_to_template(t, &generic_params))
                .collect(),
            _ => vec![],
        }
    }

    /// The element-type template for an array-literal expression — the `T` of
    /// its `T[]` static type — for [`Rvalue::Array`]. A generic element maps to
    /// a `TypeArgRef` so it resolves against the frame's type args at runtime.
    /// Falls back to `unknown` when the recorded type is not a list (error
    /// recovery).
    fn array_element_template(&self, expr_id: AstExprId) -> TyTemplate {
        let generic_params = self.enclosing_generic_params();
        match self.tir_expr_type(self.expr_metadata_key(expr_id)) {
            Some(Tir2Ty::List(elem, _) | Tir2Ty::EvolvingList(elem, _)) => {
                self.ty_to_template(elem, &generic_params)
            }
            _ => TyTemplate::from(RealizedTy::unknown()),
        }
    }

    /// The key/value type templates for a map-literal expression — the `K`/`V`
    /// of its `map<K, V>` static type — for [`Rvalue::Map`]. Falls back to
    /// `map<string, unknown>` when the recorded type is not a map (error
    /// recovery); map keys are always strings.
    fn map_kv_templates(&self, expr_id: AstExprId) -> (TyTemplate, TyTemplate) {
        let generic_params = self.enclosing_generic_params();
        match self.tir_expr_type(self.expr_metadata_key(expr_id)) {
            Some(Tir2Ty::Map { key, value, .. } | Tir2Ty::EvolvingMap(key, value, _)) => (
                self.ty_to_template(key, &generic_params),
                self.ty_to_template(value, &generic_params),
            ),
            _ => (
                TyTemplate::from(RealizedTy::string()),
                TyTemplate::from(RealizedTy::unknown()),
            ),
        }
    }

    /// The `T`/`E` templates for a `spawn` expression — the arguments of its
    /// `Future<T, E>` static type — for [`Terminator::Spawn`]. TIR has already
    /// folded any `with` transformers into that type, so this is the future the
    /// spawn actually hands back. Falls back to `unknown` when the recorded type
    /// is not a future (error recovery).
    fn spawn_future_ty(&self, expr_id: AstExprId) -> Box<crate::ir::SpawnFutureTy> {
        let generic_params = self.enclosing_generic_params();
        let (returns, throws) = match self.tir_expr_type(self.expr_metadata_key(expr_id)) {
            Some(Tir2Ty::Future(value, error, _)) => (
                self.ty_to_template(value, &generic_params),
                self.ty_to_template(error, &generic_params),
            ),
            _ => (
                TyTemplate::from(RealizedTy::unknown()),
                TyTemplate::from(RealizedTy::unknown()),
            ),
        };
        Box::new(crate::ir::SpawnFutureTy { returns, throws })
    }

    fn object_class_type_arg_templates(
        &self,
        expr_id: AstExprId,
        explicit_type_args: &[AstTypeExpr],
    ) -> Vec<TyTemplate> {
        // Empty (`Box { … }`) or unlowerable (`Box<_> { … }` — a compile error
        // whose arg lowers to an error sentinel) type args: use the class type
        // TIR inferred/solved for this expression rather than re-lowering the
        // raw args, which cannot cross the runtime boundary.
        let generic_params = self.enclosing_generic_params();
        let has_hole = explicit_type_args
            .iter()
            .any(|arg| self.type_arg_is_infer_hole(arg, &generic_params));
        if explicit_type_args.is_empty() || has_hole {
            self.class_type_arg_templates(expr_id)
        } else {
            self.generic_apply_type_arg_templates(explicit_type_args)
        }
    }

    /// Whether a written type argument lowers to a type that cannot cross the
    /// runtime boundary — a `_` wildcard (a hard error lowering to `Ty::Error`)
    /// or any other error-recovery sentinel, at any depth. A bare frame
    /// type-arg reference (`T`) and any concrete type return `false`.
    fn type_arg_is_infer_hole(&self, type_arg: &AstTypeExpr, generic_params: &[ParamTy]) -> bool {
        if Self::direct_frame_type_arg_template(type_arg, generic_params).is_some() {
            return false;
        }
        let tir_ty = self.lower_type_arg_to_tir(type_arg, generic_params);
        baml_type::lower_to_runtime(&tir_ty, self.resolved_aliases).is_err()
    }

    /// Get the `baml_type::RuntimeTy` for a pattern binding
    fn pat_ty(&self, pat_id: AstPatId) -> RuntimeTy {
        self.tir_pat_type(self.pat_metadata_key(pat_id))
            .map(|ty| self.convert_tir_ty_for_runtime(ty))
            .unwrap_or(RuntimeTy::Void {
                attr: TyAttr::default(),
            })
    }

    fn is_pattern_type_recovery(ty: &RuntimeTy) -> bool {
        matches!(
            ty,
            RuntimeTy::Void { .. } | RuntimeTy::BuiltinUnknown { .. }
        )
    }

    /// Get the TIR-inferred root segment type for a multi-segment Path expression.
    /// Returns `None` if no root type was recorded (e.g. single-segment paths).
    fn path_root_ty(&self, expr_id: AstExprId) -> Option<RuntimeTy> {
        self.tir_path_root_type(self.expr_metadata_key(expr_id))
            .map(|ty| self.convert_tir_ty_for_runtime(ty))
    }

    /// Get the TIR-inferred type of `segments[..=seg_idx]` for a multi-segment
    /// local-rooted Path expression. Returns `None` if not recorded.
    #[allow(dead_code)]
    fn path_segment_ty(&self, expr_id: AstExprId, seg_idx: usize) -> Option<RuntimeTy> {
        self.tir_path_segment_type((self.current_metadata_scope, expr_id, seg_idx))
            .map(|ty| self.convert_tir_ty_for_runtime(ty))
    }

    /// Resolve a `TypeExpr` annotation directly to a `baml_type::RuntimeTy`.
    /// Used for `TypedBinding` patterns where TIR may not have populated the
    /// bindings map (e.g. catch arm and match arm patterns).
    fn resolve_type_annotation(&self, ty_expr: &baml_compiler2_ast::TypeExpr) -> RuntimeTy {
        // Lower with the enclosing function's generic params in scope so a type
        // variable in the annotation (`let item: T => …`) resolves faithfully
        // to a `TypeVar` rather than an unresolved `Unknown`. Erasing generics
        // here would make a `: T` pattern a constant-false test, violating the
        // type contract.
        self.resolved_aliases
            .convert(&self.lower_type_annotation_tir(ty_expr))
    }

    /// Lower a pattern's type annotation to TIR with the enclosing function's
    /// generic params in scope, so `TypeVar`s survive and a typed pattern test
    /// lowers to a `TypeArgRef` template (dynamic dispatch on the realized type
    /// argument) instead of a constant-false `Void` test.
    fn lower_type_annotation_tir(&self, ty_expr: &baml_compiler2_ast::TypeExpr) -> Tir2Ty {
        let generic_params = self.enclosing_generic_params();
        let generic_param_bounds = self.enclosing_generic_param_bounds();
        let pkg_info = file_package(self.db, self.file);
        let pkg_id = PackageId::new(self.db, pkg_info.package);
        let pkg_items = package_items(self.db, pkg_id);
        let ty_expr = &self.desugar_body_type_expr(ty_expr);
        lower_expr_in_scope(
            self.db,
            ty_expr,
            pkg_items,
            &pkg_info.namespace_path,
            &generic_params,
            &generic_param_bounds,
            self.body_self_tir_ty(),
        )
    }

    /// Desugar a type expression written in the current function's body for MIR's
    /// re-lowering sites: inside an interface's own default method, a `Self.Item`
    /// reference denotes the interface's associated type — exactly the assoc-name frame
    /// slot that [`Self::enclosing_generic_params`] registers and interface dispatch
    /// seeds. Rewrite it to that slot's bare name so it lowers to the `TypeVar` →
    /// `TypeArgRef` the runtime realizes, like the interface's generic params. A no-op
    /// (cheap clone) outside interface default methods.
    fn desugar_body_type_expr(
        &self,
        ty_expr: &baml_compiler2_ast::TypeExpr,
    ) -> baml_compiler2_ast::TypeExpr {
        match self.enclosing_interface_assoc_names() {
            Some(assoc_names) => Self::rewrite_self_assoc_annotations(ty_expr, &assoc_names),
            None => ty_expr.clone(),
        }
    }

    /// The associated-type names of the interface whose default method this body is —
    /// `None` when the current function is not an interface default method.
    fn enclosing_interface_assoc_names(&self) -> Option<Vec<baml_base::Name>> {
        use baml_compiler2_ppir::item_data::{MethodOwner, interface_data, method_owner};
        let fl = self.func_loc?;
        let MethodOwner::Interface(iface_loc) = method_owner(self.db, fl)? else {
            return None;
        };
        let iface_data = interface_data(self.db, iface_loc);
        Some(
            iface_data
                .associated_types
                .iter()
                .map(|assoc| assoc.name.clone())
                .collect(),
        )
    }

    /// Rewrite every `Self.X` path (where `X` is one of `assoc_names`) to the bare `X`
    /// path, recursing through the type constructors. See
    /// [`Self::lower_type_annotation_tir`] — both spellings denote the same associated
    /// type of the enclosing interface, and the bare name is the registered frame slot.
    fn rewrite_self_assoc_annotations(
        ty_expr: &baml_compiler2_ast::TypeExpr,
        assoc_names: &[baml_base::Name],
    ) -> baml_compiler2_ast::TypeExpr {
        use baml_compiler2_ast::TypeExprKind;
        let rewrite = |inner: &baml_compiler2_ast::TypeExpr| {
            Self::rewrite_self_assoc_annotations(inner, assoc_names)
        };
        let kind = match &ty_expr.kind {
            TypeExprKind::Path {
                segments,
                generic_args,
                associated_type_bindings,
                attrs,
            } => {
                let segments = if segments.len() == 2
                    && segments[0].as_str() == "Self"
                    && assoc_names.contains(&segments[1])
                {
                    vec![segments[1].clone()]
                } else {
                    segments.clone()
                };
                TypeExprKind::Path {
                    segments,
                    generic_args: generic_args.iter().map(rewrite).collect(),
                    associated_type_bindings: associated_type_bindings
                        .iter()
                        .map(|binding| baml_compiler2_ast::AssociatedTypeBinding {
                            name: binding.name.clone(),
                            ty: Box::new(rewrite(&binding.ty)),
                        })
                        .collect(),
                    attrs: attrs.clone(),
                }
            }
            TypeExprKind::List { inner, attrs } => TypeExprKind::List {
                inner: Box::new(rewrite(inner)),
                attrs: attrs.clone(),
            },
            TypeExprKind::Optional { inner, attrs } => TypeExprKind::Optional {
                inner: Box::new(rewrite(inner)),
                attrs: attrs.clone(),
            },
            TypeExprKind::Map {
                key, value, attrs, ..
            } => TypeExprKind::Map {
                key: Box::new(rewrite(key)),
                value: Box::new(rewrite(value)),
                attrs: attrs.clone(),
            },
            TypeExprKind::Union { variants, attrs } => TypeExprKind::Union {
                variants: variants.iter().map(rewrite).collect(),
                attrs: attrs.clone(),
            },
            TypeExprKind::Function {
                params,
                ret,
                throws,
                attrs,
            } => TypeExprKind::Function {
                params: params
                    .iter()
                    .map(|param| baml_compiler2_ast::FunctionTypeParam {
                        name: param.name.clone(),
                        optional: param.optional,
                        ty: rewrite(&param.ty),
                    })
                    .collect(),
                ret: Box::new(rewrite(ret)),
                throws: throws.as_ref().map(|throws| Box::new(rewrite(throws))),
                attrs: attrs.clone(),
            },
            // Leaves (primitives, literals, `Self`, `type`, error/unknown, …).
            other => other.clone(),
        };
        kind.at(ty_expr.span)
    }

    /// Build a `Span` from an expression's source range.
    /// Returns `None` if no source map is available (e.g. synthesized bodies).
    fn span_for_expr(&self, expr_id: AstExprId) -> Option<baml_base::Span> {
        let sm = self.source_map.as_ref()?;
        let range = sm.expr_span(expr_id);
        Some(baml_base::Span::new(self.file.file_id(self.db), range))
    }

    /// Build a `Span` from a statement's source range.
    fn span_for_stmt(&self, stmt_id: AstStmtId) -> Option<baml_base::Span> {
        let sm = self.source_map.as_ref()?;
        let range = sm.stmt_span(stmt_id);
        Some(baml_base::Span::new(self.file.file_id(self.db), range))
    }
}

// ─── 3.1: lower_function_body ────────────────────────────────────────────────

#[allow(clippy::elidable_lifetime_names)]
impl<'db> LoweringContext<'db> {
    fn lower_function_body(&mut self) -> MirFunction {
        let func_loc = self
            .func_loc
            .expect("lower_function_body called on non-function LoweringContext");
        let sig = baml_compiler2_ppir::function_signature(self.db, func_loc);

        // Return place _0
        let pkg_info = file_package(self.db, self.file);
        let pkg_id = PackageId::new(self.db, pkg_info.package);
        let pkg_items = package_items(self.db, pkg_id);

        let ret_ty = sig
            .return_type
            .as_ref()
            .map(|te| self.lower_signature_runtime_ty(te, pkg_items, &pkg_info.namespace_path))
            .unwrap_or(RuntimeTy::Null {
                attr: TyAttr::default(),
            });
        let ret = self
            .builder
            .declare_local(Some(Name::new("_0")), ret_ty, None);

        // Detect enclosing class for `self` parameter resolution
        let index = file_semantic_index(self.db, self.file);
        let func_data = baml_compiler2_ppir::item_data::function_data(self.db, func_loc);
        let func_span = baml_compiler2_ppir::item_data::function_source_map(self.db, func_loc).span;
        // Set the function-level span on the builder so MirFunction::span is populated.
        self.builder
            .set_span(baml_base::Span::new(self.file.file_id(self.db), func_span));
        let func_scope_id = baml_compiler2_ppir::item_data::function_scope(self.db, func_loc)
            .expect("every item-tree function has a recorded scope")
            .file_scope_id(self.db);
        let func_scope = &index.scopes[func_scope_id.index() as usize];
        let enclosing_class_name: Option<Name> = func_scope.parent.and_then(|parent_idx| {
            let parent = &index.scopes[parent_idx.index() as usize];
            if matches!(parent.kind, baml_compiler2_hir::scope::ScopeKind::Class) {
                parent.name.clone()
            } else {
                None
            }
        });
        let enclosing_impl = match baml_compiler2_ppir::item_data::method_owner(self.db, func_loc) {
            Some(baml_compiler2_ppir::item_data::MethodOwner::FreeImpl(impl_loc)) => Some(
                baml_compiler2_ppir::item_data::impl_block_data(self.db, impl_loc),
            ),
            _ => None,
        };

        // Parameter locals _1..=_n
        // For `self` with no annotation, use the active rule receiver pattern
        // for out-of-body implementations, otherwise the enclosing class type.
        for (param_idx, param) in sig.params.iter().enumerate() {
            let param_ty = if param.name.as_str() == "self"
                && matches!(
                    param.ty.kind,
                    baml_compiler2_ast::TypeExprKind::Unknown { .. }
                ) {
                if let Some(imp) = enclosing_impl
                    && let baml_compiler2_ppir::item_data::ImplSubjectData::Free {
                        for_target, ..
                    } = &imp.subject
                {
                    let generic_params = self.enclosing_generic_params();
                    let generic_param_bounds = self.enclosing_generic_param_bounds();
                    let tir_ty = lower_ref_in_scope(
                        self.db,
                        &imp.type_refs,
                        *for_target,
                        pkg_items,
                        &pkg_info.namespace_path,
                        &generic_params,
                        &generic_param_bounds,
                        None,
                    );
                    self.convert_tir_ty_for_runtime(&tir_ty)
                } else {
                    enclosing_class_name
                        .as_ref()
                        .and_then(|cn| {
                            pkg_items
                                .lookup_type(&pkg_info.namespace_path, cn)
                                .map(|def| {
                                    let tir_ty = Tir2Ty::Class(
                                        qualify_def(self.db, def, cn),
                                        vec![],
                                        baml_type::TyAttr::default(),
                                    );
                                    self.resolved_aliases.convert(&tir_ty)
                                })
                        })
                        .unwrap_or(RuntimeTy::Null {
                            attr: TyAttr::default(),
                        })
                }
            } else {
                self.lower_signature_runtime_ty(&param.ty, pkg_items, &pkg_info.namespace_path)
            };
            let local = self
                .builder
                .declare_local(Some(param.name.clone()), param_ty, None);
            self.locals.insert(param.name.clone(), local);
            self.binding_locals
                .insert(BindingId::parameter(self.current_scope, param_idx), local);
        }

        // Entry and exit blocks
        let entry = self.builder.create_block();
        let exit = self.builder.create_block();
        self.exit_block = exit;
        self.builder.set_current_block(entry);

        let parameter_defaults =
            baml_compiler2_ppir::function_parameter_defaults(self.db, func_loc);
        self.lower_default_parameter_prologue(func_data, &parameter_defaults);

        // Lower root expression into return place
        let root_expr = self.body.root_expr;
        if let Some(root) = root_expr {
            self.lower_expr(root, Place::local(ret));
        } else {
            self.builder.assign(
                Place::local(ret),
                Rvalue::Use(Operand::Constant(Constant::Null)),
            );
        }

        // Goto exit, emit Return terminator
        if !self.builder.is_current_terminated() {
            self.builder.goto(self.exit_block);
        }
        self.builder.set_current_block(self.exit_block);
        self.builder.return_();

        // Mark locals captured by nested lambdas. HIR stores this by binding
        // identity, including block-owned bindings.
        self.mark_captured_locals_in_scope_tree(self.current_scope);

        // Take the builder out of self to call `build()` which consumes it
        let dummy = MirBuilder::new(Name::new("_dummy"), 0);
        let builder = std::mem::replace(&mut self.builder, dummy);
        let mut mir = builder.build();
        optimize::optimize_function(&mut mir);

        // Drain any lambda functions lowered during this function's body into the
        // MirFunction's lambdas list.  The lambda_idx values in MakeClosure rvalues
        // index into this vec.
        mir.lambdas = std::mem::take(&mut self.pending_lambdas);

        mir
    }

    fn lower_default_parameter_prologue(
        &mut self,
        func_data: &baml_compiler2_ppir::item_data::FunctionData,
        parameter_defaults: &baml_compiler2_hir::signature::FunctionParameterDefaults,
    ) {
        for (index, param) in func_data.params.iter().enumerate() {
            let Some(default_ref) = parameter_defaults.param_default(index) else {
                continue;
            };

            let Some(&param_local) = self.locals.get(&param.name) else {
                continue;
            };

            let test_local = self.builder.temp(RuntimeTy::Bool {
                attr: TyAttr::default(),
            });
            self.builder.assign(
                Place::local(test_local),
                Rvalue::BinaryOp {
                    op: BinOp::Eq,
                    left: Operand::Copy(Place::local(param_local)),
                    right: Operand::Constant(Constant::OmittedArg),
                },
            );

            let default_block = self.builder.create_block();
            let next_block = self.builder.create_block();
            self.builder.branch(
                Operand::Copy(Place::local(test_local)),
                default_block,
                next_block,
            );

            self.builder.set_current_block(default_block);
            self.lower_default_expr(
                default_ref.expr.expr(),
                &parameter_defaults.defaults,
                Place::local(param_local),
            );
            if !self.builder.is_current_terminated() {
                self.builder.goto(next_block);
            }

            self.builder.set_current_block(next_block);
        }
    }

    fn lower_default_expr(
        &mut self,
        expr_id: AstExprId,
        defaults: &baml_compiler2_ast::FunctionDefaults,
        dest: Place,
    ) {
        let saved_body = std::mem::replace(&mut self.body, defaults.exprs.clone());
        let saved_source_map = self.source_map.replace(defaults.source_map.clone());
        let saved_metadata_scope = self.current_metadata_scope;
        self.current_metadata_scope = MetadataScope::ParameterDefault(self.current_scope);
        self.lower_expr(expr_id, dest);
        self.current_metadata_scope = saved_metadata_scope;
        self.source_map = saved_source_map;
        self.body = saved_body;
    }

    /// Lower a top-level let binding's initializer into a zero-arg `MirFunctionBody`.
    ///
    /// The resulting body has arity 0, a single `_0` return place (type unknown/null),
    /// and evaluates the initializer expression, leaving the result in `_0`.
    /// This is used by `compile_init_function` to compile let initializers into bytecode
    /// that can then be called and have their result stored via `StoreGlobal`.
    fn lower_let_body_inner(&mut self) -> MirFunctionBody {
        // Return place _0 (type unknown — let bodies don't have type annotations)
        let ret = self.builder.declare_local(
            Some(Name::new("_0")),
            RuntimeTy::Null {
                attr: TyAttr::default(),
            },
            None,
        );

        // Entry and exit blocks
        let entry = self.builder.create_block();
        let exit = self.builder.create_block();
        self.exit_block = exit;
        self.builder.set_current_block(entry);

        // Lower root expression into return place
        if let Some(root) = self.body.root_expr {
            self.lower_expr(root, Place::local(ret));
        } else {
            self.builder.assign(
                Place::local(ret),
                Rvalue::Use(Operand::Constant(Constant::Null)),
            );
        }

        // Goto exit, emit Return terminator
        if !self.builder.is_current_terminated() {
            self.builder.goto(self.exit_block);
        }
        self.builder.set_current_block(self.exit_block);
        self.builder.return_();

        // Take the builder out and build the MirFunctionBody
        let dummy = MirBuilder::new(Name::new("_dummy"), 0);
        let builder = std::mem::replace(&mut self.builder, dummy);
        let mut body = builder.build_body();
        optimize::optimize_function_body(&mut body);
        body
    }

    fn lower_optional_function_adapter(
        &mut self,
        expr_id: AstExprId,
        coercion: &crate::inference_provider::FunctionCoercion,
        dest: Place,
    ) {
        let original_ty = self.expr_ty(expr_id);
        let original_local = self.builder.temp(original_ty);
        self.lower_expr_without_function_coercion(expr_id, Place::Local(original_local));
        self.builder.local_decl_mut(original_local).is_captured = true;

        let parent_name = self.builder.name().to_string();
        let adapter_count = self
            .synthetic_name_counts
            .entry("__optional_adapter".to_string())
            .or_insert(0);
        let adapter_idx = *adapter_count;
        *adapter_count += 1;
        let adapter_name = format!("<optional-adapter({parent_name}, {adapter_idx})>");

        let mut adapter_builder =
            MirBuilder::new(Name::new(&adapter_name), coercion.target_params.len());

        let ret_ty = self.resolved_aliases.convert(&coercion.target_return);
        let ret = adapter_builder.declare_local(Some(Name::new("_0")), ret_ty, None);

        for param in &coercion.target_params {
            let param_ty = self.resolved_aliases.convert(&param.ty);
            adapter_builder.declare_local(param.name.clone(), param_ty, None);
        }

        let entry = adapter_builder.create_block();
        let after_call = adapter_builder.create_block();
        adapter_builder.set_current_block(entry);

        let mut next_required_target = 0usize;
        let mut source_args = Vec::with_capacity(coercion.source_params.len());
        for source_param in &coercion.source_params {
            match source_param.mode {
                FunctionParamMode::Required => {
                    let target_index = coercion
                        .target_params
                        .iter()
                        .enumerate()
                        .filter(|(_, param)| param.is_required())
                        .nth(next_required_target)
                        .map(|(idx, _)| idx)
                        .unwrap_or(next_required_target);
                    next_required_target += 1;
                    source_args.push(Operand::Copy(Place::Local(Local(target_index + 1))));
                }
                FunctionParamMode::Optional => {
                    let target_index = source_param.name.as_ref().and_then(|name| {
                        coercion.target_params.iter().position(|param| {
                            param.is_optional() && param.name.as_ref() == Some(name)
                        })
                    });
                    if let Some(target_index) = target_index {
                        source_args.push(Operand::Copy(Place::Local(Local(target_index + 1))));
                    } else {
                        source_args.push(Operand::Constant(Constant::OmittedArg));
                    }
                }
            }
        }

        adapter_builder.call(
            Operand::Copy(Place::Capture(0)),
            source_args,
            Place::Local(ret),
            after_call,
            None,
        );
        adapter_builder.set_current_block(after_call);
        adapter_builder.return_();

        let mut adapter_mir = adapter_builder.build();
        optimize::optimize_function(&mut adapter_mir);
        adapter_mir.item_ref = ItemRef::Free {
            package: Name::new(""),
            namespace: vec![],
            name: Name::new(&adapter_name),
        };

        let lambda_idx = self.pending_lambdas.len();
        self.pending_lambdas.push(adapter_mir);
        self.builder.assign(
            dest,
            Rvalue::MakeClosure {
                lambda_idx,
                captures: vec![Operand::Copy(Place::Local(original_local))],
                type_arg_templates: vec![],
            },
        );
    }

    /// Lower a lambda expression into a nested `MirFunction` and emit a
    /// `Rvalue::MakeClosure` assignment into `dest`.
    ///
    /// Saves all parent-body state, sets up a fresh builder for the lambda,
    /// lowers the lambda body, then restores the parent state.  The completed
    /// `MirFunction` is pushed into `self.pending_lambdas`; its index becomes
    /// the `lambda_idx` in the emitted `MakeClosure` rvalue.
    ///
    /// Captures are empty in Phase 3 (non-capturing lambdas only).
    #[allow(clippy::cast_possible_truncation)]
    fn lower_lambda(
        &mut self,
        func_def: &baml_compiler2_ast::LambdaDef,
        expr_id: AstExprId,
        dest: Place,
    ) {
        // Generate a unique synthetic name for this lambda.
        let parent_name = self.builder.name().to_string();
        let lambda_count = self
            .synthetic_name_counts
            .entry("__lambda".to_string())
            .or_insert(0);
        let lambda_idx_name = *lambda_count;
        *lambda_count += 1;
        let lambda_name = format!("<lambda({parent_name}, {lambda_idx_name})>");

        // Find the lambda's FileScopeId from the HIR index.
        // The HIR builder registered a ScopeKind::Lambda at the lambda expression's span.
        let lambda_scope_id: FileScopeId = if let Some(ref sm) = self.source_map {
            let lambda_span = sm.expr_span(expr_id);
            let index = file_semantic_index(self.db, self.file);
            // Two functions can carry a lambda at the *same* source span — an
            // LLM function and its synthesized companions all share the parent's
            // ranges (e.g. the `$spec` companion's prompt closure vs the
            // parent's prompt-tag closure). A bare range match would pick
            // whichever lambda scope appears first in the file, binding this
            // lambda to the *other* function's captures. Disambiguate by
            // preferring the lambda scope nested within the function currently
            // being lowered; fall back to the first range match (mirrors
            // `build_tagged_body_closure`).
            index
                .lambda_scope_for_within(self.current_scope, lambda_span)
                .or_else(|| index.lambda_scope_for(lambda_span))
                .unwrap_or(self.current_scope)
        } else {
            self.current_scope
        };

        // The body is an expression in the arena already installed — a lambda
        // owns no `ExprBody`, so there is nothing to swap in.
        let Some(lambda_root) = func_def.body else {
            // No body — emit a panic stub and return.
            self.emit_panic_call("lambda without body", expr_id);
            return;
        };

        // Read HIR captures for this lambda scope.
        // `captures` lists the exact binding identities that the lambda reads
        // from enclosing scopes. We build `capture_indices` so path/lvalue
        // lowering can emit `Place::Capture(idx)` without collapsing shadows by name.
        let hir_captures: Vec<(Name, BindingId)> = {
            let index = file_semantic_index(self.db, self.file);
            index
                .scope_bindings
                .get(lambda_scope_id.index() as usize)
                .map(|sb| sb.captures.clone())
                .unwrap_or_default()
        };
        let lambda_capture_indices: HashMap<BindingId, usize> = hir_captures
            .iter()
            .enumerate()
            .map(|(i, (_, binding_id))| (*binding_id, i))
            .collect();

        // Save parent state.
        let saved_builder = std::mem::replace(
            &mut self.builder,
            MirBuilder::new(Name::new(&lambda_name), 0),
        );
        let saved_locals = std::mem::take(&mut self.locals);
        let saved_binding_locals = std::mem::take(&mut self.binding_locals);
        let saved_exit_block = self.exit_block;
        let saved_loop_context = self.loop_context.take();
        let saved_catch_context = self.catch_context.take();
        // BEP-042: a lambda body is its own cleanup region — reset the defer
        // stack so it never replays the parent's defers, restore it after.
        let saved_defer_stack = std::mem::take(&mut self.defer_stack);
        let saved_current_scope = self.current_scope;
        let saved_metadata_scope = self.current_metadata_scope;
        // A lambda declares no generic parameters of its own, so its frame is
        // exactly the enclosing one and nothing is appended for the body. The
        // save/restore stays because the body may itself contain lambdas.
        let saved_lambda_generic_params = self.lambda_generic_params.clone();
        // NOTE: synthetic_name_counts is intentionally NOT saved — its counter
        // keeps incrementing across the whole function for uniqueness.
        //
        // pending_lambdas IS saved so each lambda collects only its own direct
        // children. The lambda body's nested lambdas are collected separately
        // and attached to the lambda as its `.lambdas` field.
        let saved_pending_lambdas = std::mem::take(&mut self.pending_lambdas);
        let saved_capture_indices = self.capture_indices.take();
        // Save transitive_captures_needed: after lowering this lambda's body,
        // newly discovered transitive captures will be in this field.  We save
        // the parent's list and restore it after collecting.
        let saved_transitive_captures = std::mem::take(&mut self.transitive_captures_needed);

        // Switch to the lambda scope and install capture map.
        // Always use Some(map) — even for empty HIR captures — so that
        // add_transitive_capture can extend it at runtime.
        self.current_scope = lambda_scope_id;
        self.current_metadata_scope = MetadataScope::Body(lambda_scope_id);
        self.capture_indices = Some(lambda_capture_indices);

        // Set up a fresh builder with the correct arity.
        let arity = func_def.params.len();
        self.builder = MirBuilder::new(Name::new(&lambda_name), arity);

        let pkg_info = file_package(self.db, self.file);
        let pkg_id = PackageId::new(self.db, pkg_info.package.clone());
        let pkg_items = package_items(self.db, pkg_id);

        // A lambda param annotation may reference the enclosing function's
        // generics or the lambda's own, so lower with both in scope; otherwise
        // a `(x: T) => …` would resolve `T` to an unresolved `Unknown`. Record
        // the lowered TIR type so interface dispatch on the parameter can
        // recover its (possibly bounded) static type — TIR does not surface it
        // via `path_segment_types` for lambda receivers. Restored after the
        // body (`saved_lambda_param_tir_types` below).
        let saved_lambda_param_tir_types = self.lambda_param_tir_types.clone();
        let lambda_param_generics = self.enclosing_generic_params();
        // Lower a lambda-scope type annotation (a param, the return, the
        // throws), with the enclosing and lambda generics in scope. Lowering
        // diagnostics are dropped: TIR reports the lambda's own type errors.
        let lower_sig_ty = |this: &mut Self, te: &baml_compiler2_ast::TypeExpr| {
            lower_expr_in_scope(
                this.db,
                te,
                pkg_items,
                &pkg_info.namespace_path,
                &lambda_param_generics,
                &this.enclosing_generic_param_bounds(),
                None,
            )
        };
        // BEP-062: collect the runtime signature alongside, so emit can stamp
        // it onto the compiled `Function` object. Top-level declarations get
        // theirs from TIR `func_data` during emit; lambdas have no `func_data`,
        // and without this a closure value carries no signature at all (which
        // `reflect.signature` / `reflect.call_any` consume).
        // A lambda's signature is templated over the *enclosing* frame's slots:
        // `MakeClosure` captures that frame's type args onto the closure, and
        // the body's own `TypeArgRef`s index the same list, so substituting a
        // closure value's captured args reconstructs its signature exactly.
        let sig_frame_params = self.enclosing_generic_params();
        let sig_template = |this: &Self, tir_ty: &Tir2Ty| {
            tir2_to_template(tir_ty, this.resolved_aliases, &sig_frame_params)
        };
        // TIR infers the lambda's whole function type: every parameter type,
        // the return type, and — for an unannotated clause — the throws
        // surface recovered from the body. The written annotations are only a
        // subset of that, so each unwritten position is read off the inference
        // rather than filled with a placeholder. A closure value's
        // reconstructed signature is what `is`/`match` and `reflect` see, and
        // `(x) => x + 1` is `(int) -> int throws never` — not the
        // `(null) -> unknown throws unknown` its syntax alone spells.
        //
        // The lambda *expression* is recorded in the body that contains it, so
        // this reads the enclosing metadata scope — `current_metadata_scope` has
        // already switched to the lambda's own body, whose table holds the
        // expressions *inside* the lambda and not the lambda itself.
        // Cloned out: the accessor's borrow is tied to `self` since the
        // dual provider, and the signature pieces outlive mutations below.
        let inferred_sig = match self
            .tir_expr_type(ExprMetadataKey::new(saved_metadata_scope, expr_id))
            .cloned()
        {
            Some(Tir2Ty::Function {
                params,
                ret,
                throws,
                ..
            }) => Some((params, *ret, *throws)),
            // No recorded type: the lambda failed to type-check and is already
            // diagnosed. Keep the syntactic shape rather than invent one.
            _ => None,
        };
        let inferred_template = |this: &Self, tir_ty: &Tir2Ty| {
            tir2_to_template_in_frame(tir_ty, this.resolved_aliases, &sig_frame_params)
        };
        // The return type, written or inferred. This types the return place as
        // well as the signature: `_0` holds the lambda's result, so declaring
        // it `null` would describe a slot the body never puts a null in.
        let (sig_return_type, sig_display_return_type, ret_local_ty) = match &func_def.return_type {
            Some(te) => {
                let tir_ty = lower_sig_ty(self, te);
                (
                    sig_template(self, &tir_ty),
                    tir_ty.render_user_facing(),
                    self.convert_tir_ty_for_runtime(&tir_ty),
                )
            }
            None => match inferred_sig
                .as_ref()
                .and_then(|(_, ret, _)| inferred_template(self, ret).map(|t| (ret, t)))
            {
                Some((tir_ty, template)) => (
                    template,
                    tir_ty.render_user_facing(),
                    self.convert_tir_ty_for_runtime(tir_ty),
                ),
                // Inference has no answer to give (an already-diagnosed
                // lambda): keep the placeholder rather than invent a type.
                None => (
                    baml_type::TyTemplate::BuiltinUnknown {
                        attr: baml_type::TyAttr::default(),
                    },
                    "unknown".to_string(),
                    baml_type::RuntimeTy::Null {
                        attr: baml_type::TyAttr::default(),
                    },
                ),
            },
        };

        // Declare return place _0, then parameter locals _1..=_n.
        let ret = self
            .builder
            .declare_local(Some(Name::new("_0")), ret_local_ty, None);

        let mut sig_param_types = Vec::with_capacity(func_def.params.len());
        let mut sig_display_param_types = Vec::with_capacity(func_def.params.len());
        for (param_idx, param) in func_def.params.iter().enumerate() {
            let (param_ty, param_template, param_display) = match &param.type_expr {
                Some(spanned_te) => {
                    let tir_ty = lower_sig_ty(self, spanned_te);
                    self.lambda_param_tir_types
                        .insert(param.name.clone(), tir_ty.clone());
                    (
                        self.convert_tir_ty_for_runtime(&tir_ty),
                        sig_template(self, &tir_ty),
                        tir_ty.render_user_facing(),
                    )
                }
                None => match inferred_sig
                    .as_ref()
                    .and_then(|(params, _, _)| params.get(param_idx))
                    .and_then(|p| inferred_template(self, &p.ty).map(|t| (&p.ty, t)))
                {
                    Some((tir_ty, template)) => (
                        self.convert_tir_ty_for_runtime(tir_ty),
                        template,
                        tir_ty.render_user_facing(),
                    ),
                    None => (
                        baml_type::RuntimeTy::Null {
                            attr: baml_type::TyAttr::default(),
                        },
                        baml_type::TyTemplate::Null {
                            attr: baml_type::TyAttr::default(),
                        },
                        "null".to_string(),
                    ),
                },
            };
            sig_param_types.push(param_template);
            sig_display_param_types.push(param_display);
            let local = self
                .builder
                .declare_local(Some(param.name.clone()), param_ty, None);
            self.locals.insert(param.name.clone(), local);
            self.binding_locals
                .insert(BindingId::parameter(self.current_scope, param_idx), local);
        }
        // The throws surface, written or inferred. An explicit `throws never`
        // stays `never` — the empty error set, spelled the same way a function
        // type spells it — and an omitted clause takes the set TIR recovered
        // from the body, which is the claim the lambda actually makes.
        // `unknown` survives only where inference has no answer to give.
        let sig_throws_type = match &func_def.throws {
            Some(te) => {
                let tir_ty = lower_sig_ty(self, te);
                sig_template(self, &tir_ty)
            }
            None => inferred_sig
                .as_ref()
                .and_then(|(_, _, throws)| inferred_template(self, throws))
                .unwrap_or_else(|| baml_type::TyTemplate::BuiltinUnknown {
                    attr: baml_type::TyAttr::default(),
                }),
        };

        // Create entry and exit blocks.
        let entry = self.builder.create_block();
        let exit_blk = self.builder.create_block();
        self.exit_block = exit_blk;
        self.builder.set_current_block(entry);

        // Lower the body expression into the return place.
        self.lower_expr(lambda_root, Place::local(ret));

        // Terminate: goto exit, then return.
        if !self.builder.is_current_terminated() {
            self.builder.goto(self.exit_block);
        }
        self.builder.set_current_block(self.exit_block);
        self.builder.return_();

        // Mark locals captured by nested lambdas. HIR stores this by binding
        // identity, including block-owned bindings.
        self.mark_captured_locals_in_scope_tree(lambda_scope_id);

        // Build the lambda MirFunction.
        // First, collect any nested lambdas that were encountered while lowering
        // this lambda's body (direct children only — saved_pending_lambdas holds
        // any lambdas from the parent scope that were already pending before
        // entering this lambda).
        let nested_lambdas = std::mem::take(&mut self.pending_lambdas);

        let dummy = MirBuilder::new(Name::new("_dummy"), 0);
        let lambda_builder = std::mem::replace(&mut self.builder, dummy);
        let mut lambda_mir = lambda_builder.build();
        optimize::optimize_function(&mut lambda_mir);
        // Override item_ref with the synthetic name.
        lambda_mir.item_ref = ItemRef::Free {
            package: Name::new(""),
            namespace: vec![],
            name: Name::new(&lambda_name),
        };
        // Attach nested lambdas as direct children.
        lambda_mir.lambdas = nested_lambdas;
        lambda_mir.signature = Some(crate::ir::RuntimeSignature {
            // A lambda has no source-level name; its `Function::name` is a
            // synthetic debug identity.
            name: None,
            // A lambda carries neither a docstring nor generic parameters.
            docstring: None,
            display_type_params: Vec::new(),
            generic_param_bounds: Vec::new(),
            display_param_types: sig_display_param_types,
            display_return_type: sig_display_return_type,
            param_names: func_def.params.iter().map(|p| p.name.to_string()).collect(),
            param_has_default: func_def
                .params
                .iter()
                .map(|p| p.default.is_some())
                .collect(),
            param_types: sig_param_types,
            return_type: sig_return_type,
            throws_type: sig_throws_type,
        });

        // Collect transitive captures that inner lambda bodies discovered were
        // needed (names that weren't in hir_captures but that inner lambdas
        // required via transitive capture).
        let newly_needed_transitive = std::mem::take(&mut self.transitive_captures_needed);

        // Restore parent state.
        self.lambda_param_tir_types = saved_lambda_param_tir_types;
        self.builder = saved_builder;
        self.locals = saved_locals;
        self.binding_locals = saved_binding_locals;
        self.exit_block = saved_exit_block;
        self.loop_context = saved_loop_context;
        self.catch_context = saved_catch_context;
        self.defer_stack = saved_defer_stack;
        self.current_scope = saved_current_scope;
        self.current_metadata_scope = saved_metadata_scope;
        self.lambda_generic_params = saved_lambda_generic_params;
        self.capture_indices = saved_capture_indices;
        // Restore parent's pending_lambdas (siblings of this lambda).
        self.pending_lambdas = saved_pending_lambdas;
        // Restore the parent's transitive captures (not ours).
        self.transitive_captures_needed = saved_transitive_captures;

        // Extend hir_captures with any transitively-needed names discovered
        // during body lowering (for inner lambdas that needed grandparent vars).
        // Do NOT propagate here — the capture operands building loop below will
        // handle propagation by pushing to `transitive_captures_needed` when a
        // name is not found in the current scope's locals or captures.
        let mut extended_hir_captures = hir_captures;
        for binding_id in newly_needed_transitive {
            if !extended_hir_captures
                .iter()
                .any(|(_, existing)| *existing == binding_id)
            {
                extended_hir_captures.push((Name::new("_capture"), binding_id));
            }
        }

        // Build capture operands from restored parent locals/captures.
        // Each captured name must be in the parent's locals map; we pass the cell
        // pointer (the slot itself, not the inner value) via Operand::Copy(Place::Local(local)).
        // The emit phase later replaces this with a LoadVar of the cell slot (not LoadDeref).
        //
        // If a name is not in the parent's locals AND not in the parent's
        // capture_indices, we add it as a transitive capture of the current
        // lambda — i.e. the current lambda (f) will need to capture it from ITS
        // parent, and g will receive it via f's capture slot.
        let mut capture_operands: Vec<Operand> = Vec::with_capacity(extended_hir_captures.len());
        for (_, binding_id) in &extended_hir_captures {
            if let Some(&local) = self.binding_locals.get(binding_id) {
                // Mark the local as captured at the capture site — this is the
                // definitive place where we know the exact Local being captured,
                // even in the presence of shadowing.
                self.builder.local_decl_mut(local).is_captured = true;
                capture_operands.push(Operand::Copy(Place::Local(local)));
            } else if let Some(cap_idx) = self
                .capture_indices
                .as_ref()
                .and_then(|m| m.get(binding_id))
                .copied()
            {
                // The variable is itself a capture in the current scope.
                capture_operands.push(Operand::Copy(Place::Capture(cap_idx)));
            } else {
                // Not in current scope's locals or captures.
                // Add as a new transitive capture of the current lambda so our
                // parent will pass it through to us, and we can forward it to
                // the inner lambda.
                let new_idx = {
                    let ci = self.capture_indices.get_or_insert_with(HashMap::new);
                    let idx = ci.len();
                    ci.insert(*binding_id, idx);
                    idx
                };
                // Signal to our parent lambda that it needs to capture this name.
                self.transitive_captures_needed.push(*binding_id);
                capture_operands.push(Operand::Copy(Place::Capture(new_idx)));
            }
        }

        // Push this lambda into the parent's pending_lambdas and emit MakeClosure.
        let lambda_pending_idx = self.pending_lambdas.len();
        self.pending_lambdas.push(lambda_mir);

        // Build TyTemplate entries for each enclosing generic type param so
        // the closure can materialise them at runtime.  These resolve in the
        // **outer** frame (TypeArgRef(N) → outer frame.type_args[N]).
        let type_arg_templates = self.enclosing_runtime_type_arg_templates();

        self.builder.assign(
            dest,
            Rvalue::MakeClosure {
                lambda_idx: lambda_pending_idx,
                captures: capture_operands,
                type_arg_templates,
            },
        );
    }
}

// ─── 3.1b: Tagged-template lowering (BEP-049 §10 / M4e.1) ─────────────────────

impl LoweringContext<'_> {
    /// Lower a tagged template (a `TAGGED_TEMPLATE_EXPR`) to a
    /// `tag(body = <closure>)` call, where the closure builds a
    /// `baml.TaggedString { parts, values }` from the template segments.
    ///
    /// The closure is hand-rolled (there is no AST `Expr::Lambda`): HIR
    /// registered a `ScopeKind::Lambda` spanning the tagged-template expr so
    /// captures are computed; we replicate `lower_lambda`'s skeleton but supply
    /// the body params (from the tag's `body: (...) -> baml.TaggedString`
    /// signature) and the array-builder body ourselves. The interpolation
    /// expressions live in the *current* `ExprBody`, so unlike `lower_lambda`
    /// we do NOT swap `self.body`/`self.source_map`.
    fn lower_tagged_template(
        &mut self,
        expr_id: AstExprId,
        tag: AstExprId,
        body: AstExprId,
        segments: &[baml_compiler2_ast::TemplateSegment],
        dest: Place,
    ) {
        // ── Resolve the tag function. TIR (M4d.3) already validated it is a
        //    //baml:tagged_string fn whose first param is
        //    `body: (...) -> baml.TaggedString`; resolve again for its ItemRef
        //    + signature (the body-lambda param names/types). ──
        let tag_span_start = self
            .source_map
            .as_ref()
            .map(|sm| sm.expr_span(tag).start())
            .unwrap_or_default();
        // Prefer the resolution TIR recorded for the tag expression. A qualified
        // tag like `ai.prompt` is a multi-segment path whose `func_loc`
        // lives in `resolutions` (`infer_multi_segment_path`); resolving only
        // the bare last segment (`prompt`) in the user's scope would miss it.
        // Fall back to bare-name resolution for unqualified, in-file tags.
        let tag_func_loc = self
            .tir_resolution(self.expr_metadata_key(tag))
            .and_then(|r| resolution_func_loc(r))
            .or_else(|| {
                let tag_name = match &self.body.exprs[tag] {
                    AstExpr::Path(segs) => segs.last().cloned(),
                    _ => None,
                };
                match tag_name.as_ref().map(|n| {
                    resolve_name_at_in_scope(
                        self.db,
                        self.file,
                        tag_span_start,
                        n,
                        self.scope_func_name.as_ref(),
                    )
                }) {
                    Some(
                        ResolvedName::Item(Definition::Function(fl))
                        | ResolvedName::Builtin(Definition::Function(fl)),
                    ) => Some(fl),
                    _ => None,
                }
            });
        let Some(tag_func_loc) = tag_func_loc else {
            // Unreachable in well-typed programs (TIR rejects non-function
            // tags); guard so codegen never proceeds on a malformed tag.
            self.emit_panic_call("tagged-template tag did not resolve to a function", expr_id);
            return;
        };
        let tag_item_ref = def_to_item_ref(self.db, Definition::Function(tag_func_loc));

        // ── Body-lambda params + closure type from the tag's `body` param. ──
        let tag_sig = baml_compiler2_ppir::function_signature(self.db, tag_func_loc);
        let tag_pkg_info = file_package(self.db, tag_func_loc.file(self.db));
        let tag_pkg_id = PackageId::new(self.db, tag_pkg_info.package.clone());
        let tag_pkg_items = package_items(self.db, tag_pkg_id);
        let mut body_params: Vec<(Name, RuntimeTy)> = Vec::new();
        let closure_ty = match tag_sig.params.first().map(|p| &p.ty) {
            Some(
                body_te @ baml_compiler2_ast::TypeExpr {
                    kind: baml_compiler2_ast::TypeExprKind::Function { params, .. },
                    ..
                },
            ) => {
                for (i, p) in params.iter().enumerate() {
                    let name = p
                        .name
                        .clone()
                        .unwrap_or_else(|| Name::new(format!("__arg{i}")));
                    let tir_ty = lower_expr_in_scope(
                        self.db,
                        &p.ty,
                        tag_pkg_items,
                        &tag_pkg_info.namespace_path,
                        &[],
                        &FxHashMap::default(),
                        None,
                    );
                    body_params.push((name, self.resolved_aliases.convert(&tir_ty)));
                }
                let tir_ty = lower_expr_in_scope(
                    self.db,
                    body_te,
                    tag_pkg_items,
                    &tag_pkg_info.namespace_path,
                    &[],
                    &FxHashMap::default(),
                    None,
                );
                self.resolved_aliases.convert(&tir_ty)
            }
            _ => RuntimeTy::Null {
                attr: TyAttr::default(),
            },
        };

        // ── Static segment layout (text + interp only) → fixed-array fast path
        //    (M4e.1a). `None` ⇒ a `${for}`/`${if}` block is present, so the
        //    closure body lowers the desugared `body` flatten block instead
        //    (M4e.1b). ──
        let static_layout = Self::collect_static_tagged_segments(segments);

        // ── Hand-roll the body closure → an Operand. ──
        let closure_op =
            self.build_tagged_body_closure(expr_id, body, &body_params, closure_ty, static_layout);

        // ── Emit `tag(closure)` → dest. The result is the template's value. ──
        let callee = Operand::Constant(Constant::Function(tag_item_ref));
        let unwind = self.catch_context.as_ref().map(|c| c.unwind_target);
        let target = self.builder.create_block();
        match &dest {
            Place::Local(_) => {
                self.builder
                    .call(callee, vec![closure_op], dest, target, unwind);
                self.builder.set_current_block(target);
            }
            _ => {
                let ty = self.expr_ty(expr_id);
                let tmp = self.builder.temp(ty);
                self.builder
                    .call(callee, vec![closure_op], Place::local(tmp), target, unwind);
                self.builder.set_current_block(target);
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Copy(Place::local(tmp))));
            }
        }
    }

    /// Flatten text/interpolation segments into `(parts, value_exprs)` honoring
    /// `parts.len() == value_exprs.len() + 1`. Returns `None` if any
    /// `${for}`/`${if}` block is present (M4e.1b handles those at runtime).
    fn collect_static_tagged_segments(
        segments: &[baml_compiler2_ast::TemplateSegment],
    ) -> Option<(Vec<String>, Vec<AstExprId>)> {
        use baml_compiler2_ast::TemplateSegment;
        let mut parts: Vec<String> = Vec::new();
        let mut values: Vec<AstExprId> = Vec::new();
        let mut cur = String::new();
        for seg in segments {
            match seg {
                TemplateSegment::Text(s) => cur.push_str(s),
                TemplateSegment::Interp(e) => {
                    // Close the current literal part, then record the value.
                    parts.push(std::mem::take(&mut cur));
                    values.push(*e);
                }
                TemplateSegment::For { .. }
                | TemplateSegment::CStyleFor { .. }
                | TemplateSegment::If { .. } => return None,
            }
        }
        parts.push(cur); // trailing part (possibly empty) → parts.len()==values.len()+1
        Some((parts, values))
    }

    /// Hand-roll the tagged-template body closure, returning the closure value.
    /// Replicates `lower_lambda`'s state-save / param-decl / build / capture /
    /// `MakeClosure` skeleton, replacing the AST-body lowering with the
    /// array-builder (`static_layout`). `self.body`/`self.source_map` are NOT
    /// swapped — the interpolation exprs live in the current `ExprBody`.
    #[allow(clippy::cast_possible_truncation)]
    fn build_tagged_body_closure(
        &mut self,
        expr_id: AstExprId,
        body: AstExprId,
        body_params: &[(Name, RuntimeTy)],
        closure_ty: RuntimeTy,
        static_layout: Option<(Vec<String>, Vec<AstExprId>)>,
    ) -> Operand {
        let parent_name = self.builder.name().to_string();
        let idx = {
            let c = self
                .synthetic_name_counts
                .entry("__tagged".to_string())
                .or_insert(0);
            let i = *c;
            *c += 1;
            i
        };
        let lambda_name = format!("<tagged({parent_name}, {idx})>");

        // Find the HIR Lambda scope registered for this tagged template (its
        // span == the tagged-template expr span; see HIR walk_tagged_template_body).
        let lambda_scope_id: FileScopeId = if let Some(ref sm) = self.source_map {
            let span = sm.expr_span(expr_id);
            let index = file_semantic_index(self.db, self.file);
            // Two functions can carry a tagged template at the *same* source
            // span — notably a new-mode LLM function and its `$stream`
            // companion, both synthesized from the one `prompt`…`` at
            // `llm_body_def.span`. A bare range match would pick whichever
            // lambda scope appears first in the file (the oneshot body's),
            // binding the companion's `${param}` interps to the *other*
            // function's captures. Disambiguate by preferring the lambda scope
            // nested within the function currently being lowered; fall back to
            // the first range match.
            let found = index
                .lambda_scope_for_within(self.current_scope, span)
                .or_else(|| index.lambda_scope_for(span));
            debug_assert!(
                found.is_some(),
                "no HIR Lambda scope for tagged template at {span:?}"
            );
            found.unwrap_or(self.current_scope)
        } else {
            self.current_scope
        };

        let hir_captures: Vec<(Name, BindingId)> = {
            let index = file_semantic_index(self.db, self.file);
            index
                .scope_bindings
                .get(lambda_scope_id.index() as usize)
                .map(|sb| sb.captures.clone())
                .unwrap_or_default()
        };
        let lambda_capture_indices: HashMap<BindingId, usize> = hir_captures
            .iter()
            .enumerate()
            .map(|(i, (_, binding_id))| (*binding_id, i))
            .collect();

        // Save parent state. NOTE: body/source_map are intentionally NOT saved
        // — the interpolation exprs live in the current (enclosing) ExprBody.
        let saved_builder = std::mem::replace(
            &mut self.builder,
            MirBuilder::new(Name::new(&lambda_name), 0),
        );
        let saved_locals = std::mem::take(&mut self.locals);
        let saved_binding_locals = std::mem::take(&mut self.binding_locals);
        let saved_exit_block = self.exit_block;
        let saved_loop_context = self.loop_context.take();
        let saved_catch_context = self.catch_context.take();
        let saved_current_scope = self.current_scope;
        let saved_metadata_scope = self.current_metadata_scope;
        let saved_pending_lambdas = std::mem::take(&mut self.pending_lambdas);
        let saved_capture_indices = self.capture_indices.take();
        let saved_transitive_captures = std::mem::take(&mut self.transitive_captures_needed);
        let saved_tagged_body_params = std::mem::take(&mut self.tagged_body_param_bindings);

        self.current_scope = lambda_scope_id;
        self.current_metadata_scope = MetadataScope::Body(lambda_scope_id);
        self.capture_indices = Some(lambda_capture_indices);
        // Body params resolve from `self.locals` (no HIR binding) — record each
        // name with the synthetic `BindingId::parameter` it is given below (same
        // `self.current_scope` and index order as the declare loop), so
        // `lower_path_expr` resolves `${param}` interps to the locals and a nested
        // lambda referencing one can capture it transitively by that BindingId.
        self.tagged_body_param_bindings = body_params
            .iter()
            .enumerate()
            .map(|(idx, (n, _))| (n.clone(), BindingId::parameter(self.current_scope, idx)))
            .collect();

        let arity = body_params.len();
        self.builder = MirBuilder::new(Name::new(&lambda_name), arity);

        // Return place _0.
        let ret = self.builder.declare_local(
            Some(Name::new("_0")),
            RuntimeTy::Null {
                attr: TyAttr::default(),
            },
            None,
        );

        // Body params _1..=_n (the tag supplies their values when it calls body).
        for (param_idx, (name, ty)) in body_params.iter().enumerate() {
            let local = self
                .builder
                .declare_local(Some(name.clone()), ty.clone(), None);
            self.locals.insert(name.clone(), local);
            self.binding_locals
                .insert(BindingId::parameter(self.current_scope, param_idx), local);
        }

        let entry = self.builder.create_block();
        let exit_blk = self.builder.create_block();
        self.exit_block = exit_blk;
        self.builder.set_current_block(entry);

        // ── Body: construct `baml.TaggedString { parts, values }`. ──
        match static_layout {
            Some((parts, value_exprs)) => {
                let parts_ops: Vec<Operand> = parts
                    .into_iter()
                    .map(|s| Operand::Constant(Constant::String(s)))
                    .collect();
                let parts_local = self.builder.declare_local(
                    Some(Name::new("__tt_parts")),
                    RuntimeTy::List(
                        Box::new(RuntimeTy::String {
                            attr: TyAttr::default(),
                        }),
                        TyAttr::default(),
                    ),
                    None,
                );
                self.builder.assign(
                    Place::local(parts_local),
                    // Tagged-template literal parts are always strings.
                    Rvalue::Array(TyTemplate::from(RealizedTy::string()), parts_ops),
                );

                // Interps lower in the closure scope (body-param refs →
                // Place::Local, enclosing-local refs → Place::Capture), but
                // their TIR types/resolutions were inferred INLINE in the
                // enclosing body — keyed under the enclosing `MetadataScope`,
                // not this synthetic lambda scope. Restore it so member/method
                // resolution lookups hit the recorded entries (otherwise a
                // method call `${ctx.m()}` misses its resolution and falls back
                // to a map-element access → runtime `expected Map, got Instance`).
                // Mirrors the `None` (dynamic-layout) arm below.
                let prev_metadata_scope = self.current_metadata_scope;
                self.current_metadata_scope = saved_metadata_scope;
                let value_ops: Vec<Operand> = value_exprs
                    .iter()
                    .map(|&e| self.lower_to_operand(e))
                    .collect();
                self.current_metadata_scope = prev_metadata_scope;
                let values_local = self.builder.declare_local(
                    Some(Name::new("__tt_values")),
                    RuntimeTy::List(
                        Box::new(RuntimeTy::BuiltinUnknown {
                            attr: TyAttr::default(),
                        }),
                        TyAttr::default(),
                    ),
                    None,
                );
                self.builder.assign(
                    Place::local(values_local),
                    // Tagged-template interpolated values are heterogeneous.
                    Rvalue::Array(TyTemplate::from(RealizedTy::unknown()), value_ops),
                );

                self.builder.assign(
                    Place::local(ret),
                    Rvalue::Aggregate {
                        kind: AggregateKind::Class {
                            name: "baml.TaggedString".to_string(),
                            type_arg_templates: vec![],
                        },
                        fields: vec![
                            Operand::Copy(Place::local(parts_local)),
                            Operand::Copy(Place::local(values_local)),
                        ],
                    },
                );
            }
            None => {
                // M4e.1b: a `${for}`/`${if}` block is present, so lengths are
                // data-dependent. Lower the desugared `body` flatten block
                // (built at AST lowering, type-checked by TIR): it builds
                // `baml.TaggedString { parts, values }` via empty lists + `push`
                // in real loops/branches. Body-param and capture references
                // inside resolve through the closure scope / capture indices
                // set up above (those don't use the metadata scope).
                //
                // The flatten block's exprs were inferred INLINE in the
                // enclosing function (the tag isn't a real `Expr::Lambda`), so
                // their TIR types/resolutions are keyed under the enclosing
                // body's `MetadataScope` — not this synthetic lambda scope.
                // Temporarily restore it so `expr_ty`/resolution lookups (e.g.
                // resolving `parts.push(...)` to `Array.push` rather than a
                // map-element access) hit the recorded entries.
                let prev_metadata_scope = self.current_metadata_scope;
                self.current_metadata_scope = saved_metadata_scope;
                self.lower_expr(body, Place::local(ret));
                self.current_metadata_scope = prev_metadata_scope;
            }
        }

        if !self.builder.is_current_terminated() {
            self.builder.goto(self.exit_block);
        }
        self.builder.set_current_block(self.exit_block);
        self.builder.return_();

        self.mark_captured_locals_in_scope_tree(lambda_scope_id);

        let nested_lambdas = std::mem::take(&mut self.pending_lambdas);
        let dummy = MirBuilder::new(Name::new("_dummy"), 0);
        let lambda_builder = std::mem::replace(&mut self.builder, dummy);
        let mut lambda_mir = lambda_builder.build();
        optimize::optimize_function(&mut lambda_mir);
        lambda_mir.item_ref = ItemRef::Free {
            package: Name::new(""),
            namespace: vec![],
            name: Name::new(&lambda_name),
        };
        lambda_mir.lambdas = nested_lambdas;

        let newly_needed_transitive = std::mem::take(&mut self.transitive_captures_needed);

        // Restore parent state.
        self.builder = saved_builder;
        self.locals = saved_locals;
        self.binding_locals = saved_binding_locals;
        self.exit_block = saved_exit_block;
        self.loop_context = saved_loop_context;
        self.catch_context = saved_catch_context;
        self.current_scope = saved_current_scope;
        self.current_metadata_scope = saved_metadata_scope;
        self.capture_indices = saved_capture_indices;
        self.pending_lambdas = saved_pending_lambdas;
        self.transitive_captures_needed = saved_transitive_captures;
        self.tagged_body_param_bindings = saved_tagged_body_params;

        let mut extended_hir_captures = hir_captures;
        for binding_id in newly_needed_transitive {
            if !extended_hir_captures
                .iter()
                .any(|(_, existing)| *existing == binding_id)
            {
                extended_hir_captures.push((Name::new("_capture"), binding_id));
            }
        }

        let mut capture_operands: Vec<Operand> = Vec::with_capacity(extended_hir_captures.len());
        for (_, binding_id) in &extended_hir_captures {
            if let Some(&local) = self.binding_locals.get(binding_id) {
                self.builder.local_decl_mut(local).is_captured = true;
                capture_operands.push(Operand::Copy(Place::Local(local)));
            } else if let Some(cap_idx) = self
                .capture_indices
                .as_ref()
                .and_then(|m| m.get(binding_id))
                .copied()
            {
                capture_operands.push(Operand::Copy(Place::Capture(cap_idx)));
            } else {
                let new_idx = {
                    let ci = self.capture_indices.get_or_insert_with(HashMap::new);
                    let idx = ci.len();
                    ci.insert(*binding_id, idx);
                    idx
                };
                self.transitive_captures_needed.push(*binding_id);
                capture_operands.push(Operand::Copy(Place::Capture(new_idx)));
            }
        }

        let lambda_pending_idx = self.pending_lambdas.len();
        self.pending_lambdas.push(lambda_mir);

        let type_arg_templates = self.enclosing_runtime_type_arg_templates();

        let closure_local = self.builder.temp(closure_ty);
        self.builder.assign(
            Place::local(closure_local),
            Rvalue::MakeClosure {
                lambda_idx: lambda_pending_idx,
                captures: capture_operands,
                type_arg_templates,
            },
        );
        Operand::Copy(Place::Local(closure_local))
    }
}

// ─── 3.2: Core lower_expr dispatch ───────────────────────────────────────────

impl LoweringContext<'_> {
    fn lower_scoped_block(
        &mut self,
        stmts: &[AstStmtId],
        tail_expr: Option<AstExprId>,
        dest: Place,
    ) {
        let saved_locals = self.locals.clone();
        let type_binding_scope_start = self.runtime_type_binding_params.len();
        let defer_depth = self.defer_stack.len();

        // BEP-042 Stage 2: a defer must also run when an exception propagates
        // out of a *call* inside the block. Each defer splits the block and
        // opens a catch-all unwind region whose landing pad replays that defer
        // then cascades to the next-outer pad / enclosing handler. The
        // exception table routes a throw to the innermost region reached so far
        // (see `try_unwind_exception`), so only the defers armed before the
        // throw run. (Non-throwing exits — normal fall-through, return,
        // break/continue — run defers via the inline `replay_defers_to_depth`
        // path instead.)
        let block_incoming_catch = self.catch_context;
        // (landing-pad block, defer body, context to cascade to after replay,
        // catch-region index — to fill in the pad's handler_body once its body
        // is lowered below)
        let mut defer_pads: Vec<(BlockId, AstExprId, Option<CatchContext>, usize)> = Vec::new();
        let mut shared_error: Option<Local> = None;
        // BEP-042 cause chain: a throw inside a defer pad — a sibling defer that
        // throws while the scope is already unwinding — is "during handling of"
        // the in-flight error. All pads in this block share one ErrorContext
        // slot; the throw funnel materializes the in-flight error into it when
        // an error reaches a pad, and the next sibling defer's throw chains onto
        // it. Lazily declared alongside `shared_error`.
        let mut shared_ctx: Option<Local> = None;

        for &stmt_id in stmts {
            let defer_body = match &self.body.stmts[stmt_id] {
                AstStmt::Defer { body } => Some(*body),
                _ => None,
            };
            match defer_body {
                Some(body) => {
                    // Register for inline replay on the non-throwing exits.
                    self.defer_stack.push(body);
                    // Open the unwind region protecting the rest of the block.
                    let error_local = *shared_error.get_or_insert_with(|| {
                        self.builder.declare_local(
                            None,
                            RuntimeTy::BuiltinUnknown {
                                attr: TyAttr::default(),
                            },
                            None,
                        )
                    });
                    let ctx_local = *shared_ctx.get_or_insert_with(|| {
                        self.builder.declare_local(
                            None,
                            RuntimeTy::BuiltinUnknown {
                                attr: TyAttr::default(),
                            },
                            None,
                        )
                    });
                    let pad = self.builder.create_block();
                    // Split into a fresh block so the region covers only the
                    // code AFTER this defer (a throw before it must not run it).
                    let region_start = self.builder.create_block();
                    if !self.builder.is_current_terminated() {
                        self.builder.goto(region_start);
                    }
                    self.builder.set_current_block(region_start);
                    let region_idx = self.builder.catch_regions.len();
                    self.builder.catch_regions.push(CatchRegion {
                        body_entry: region_start,
                        handler: pad,
                        // handler_body is filled in once the pad body is lowered
                        // (below). `stack_trace_local` holds the in-flight
                        // error's ErrorContext so a sibling defer that throws
                        // while unwinding chains onto it (BEP-042 cause chain).
                        handler_body: Vec::new(),
                        error_local,
                        stack_trace_local: Some(ctx_local),
                    });
                    let route_ctx = self.catch_context;
                    defer_pads.push((pad, body, route_ctx, region_idx));
                    self.catch_context = Some(CatchContext {
                        unwind_target: pad,
                        error_local,
                    });
                }
                None => {
                    self.lower_stmt(stmt_id);
                    if self.builder.is_current_terminated() {
                        break;
                    }
                }
            }
        }

        // Tail expr is still inside the innermost defer region, so a throw here
        // runs the block's defers via the pad path.
        if !self.builder.is_current_terminated() {
            match tail_expr {
                Some(tail) => self.lower_expr(tail, dest),
                None => {
                    self.builder
                        .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
                }
            }
        }

        // Normal (non-throwing) fall-through: replay defers inline.
        if !self.builder.is_current_terminated() {
            self.replay_defers_to_depth(defer_depth);
        }

        // Emit the landing pads out of line (reached via the exception table).
        // Reverse order so the innermost (last-declared) pad is laid out first.
        if !defer_pads.is_empty() {
            let continuation = self.builder.current_block();
            for &(pad, body, route_ctx, region_idx) in defer_pads.iter().rev() {
                self.builder.set_current_block(pad);
                // Lower the defer body under the ENCLOSING context, not this
                // pad's `route_ctx`. A throw/call inside the body is routed to
                // the next-outer pad by the exception table (its region covers
                // the body). Using `route_ctx` here would instead give the
                // body's calls an unwind edge to the sibling pad, pulling that
                // pad early in RPO so its region no longer covers the body's
                // (later-laid-out) throw block — and the throw would escape,
                // skipping the remaining defers. The explicit cascade below
                // handles a defer body that completes normally.
                self.catch_context = block_incoming_catch;
                let tmp = self.builder.temp(RuntimeTy::Void {
                    attr: TyAttr::default(),
                });
                // The pad body IS this defer's handler body: a throw inside it
                // is "during handling of" the in-flight error. Capture every
                // block the body lowers into (the pad plus any it creates) so
                // the cause pre-walk covers them all.
                let pad_body_lo = self.builder.num_blocks();
                self.lower_expr(body, Place::local(tmp));
                if !self.builder.is_current_terminated() {
                    let error =
                        shared_error.expect("a defer pad implies a shared error local exists");
                    match route_ctx {
                        Some(outer) => {
                            if outer.error_local != error {
                                self.builder.assign(
                                    Place::local(outer.error_local),
                                    Rvalue::Use(Operand::Copy(Place::Local(error))),
                                );
                            }
                            self.builder.goto(outer.unwind_target);
                        }
                        None => {
                            // Re-raise the in-flight error unchanged: a rethrow,
                            // not a fresh throw, so the cause pre-walk does not
                            // chain it onto its own context (a self-link).
                            self.builder.rethrow(Operand::Copy(Place::Local(error)));
                        }
                    }
                }
                self.builder.catch_regions[region_idx].handler_body = std::iter::once(pad)
                    .chain((pad_body_lo..self.builder.num_blocks()).map(BlockId))
                    .collect();
            }
            self.builder.set_current_block(continuation);
        }

        self.catch_context = block_incoming_catch;
        self.defer_stack.truncate(defer_depth);
        self.runtime_type_binding_params
            .truncate(type_binding_scope_start);
        self.restore_locals_after_scope(saved_locals);
    }

    fn lower_expr(&mut self, expr_id: AstExprId, dest: Place) {
        if let Some(coercion) = self
            .tir_function_coercion(self.expr_metadata_key(expr_id))
            .cloned()
        {
            self.lower_optional_function_adapter(expr_id, &coercion, dest);
        } else {
            self.lower_expr_without_function_coercion(expr_id, dest);
        }
    }

    fn planned_call_args(
        &self,
        expr_id: AstExprId,
        args: &[CallArg],
    ) -> (Vec<AstExprId>, Option<AstExprId>) {
        let runtime_id = self
            .tir_call_plan(self.expr_metadata_key(expr_id))
            .and_then(|plan| plan.side_channels.runtime_id);
        let ordinary_args = args
            .iter()
            .filter_map(|arg| (Some(arg.expr) != runtime_id).then_some(arg.expr))
            .collect();
        (ordinary_args, runtime_id)
    }

    fn lower_runtime_id_operand(&mut self, runtime_id: Option<AstExprId>) -> Option<Operand> {
        runtime_id.map(|expr_id| {
            let operand = self.lower_to_operand(expr_id);
            let ty = self.expr_ty(expr_id);
            Operand::Copy(Place::Local(self.operand_to_local(operand, ty)))
        })
    }

    fn lower_expr_without_function_coercion(&mut self, expr_id: AstExprId, dest: Place) {
        let prev_span = self.builder.current_source_span;
        if let Some(span) = self.span_for_expr(expr_id) {
            self.builder.current_source_span = Some(span);
        }

        // Clone expr to avoid borrow issues
        let expr = self.body.exprs[expr_id].clone();
        match expr {
            AstExpr::Literal(lit) => {
                let constant = Self::lower_literal(&lit);
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(constant)));
            }

            AstExpr::ByteStringLiteral(bytes) => {
                self.builder.assign(dest, Rvalue::Uint8Array(bytes));
            }

            AstExpr::Null => {
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
            }

            AstExpr::Path(segments) => {
                self.lower_path_expr(expr_id, &segments, dest);
            }

            AstExpr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.lower_if(expr_id, condition, then_branch, else_branch, dest);
            }

            AstExpr::IfLet {
                pattern,
                scrutinee,
                then_branch,
                else_branch,
            } => {
                self.lower_if_let(expr_id, pattern, scrutinee, then_branch, else_branch, dest);
            }

            AstExpr::Binary { op, lhs, rhs } => {
                self.lower_binary(expr_id, op, lhs, rhs, dest);
            }

            AstExpr::Unary { op, expr } => {
                self.lower_unary(expr_id, op, expr, dest);
            }

            AstExpr::Call { callee, args, .. } => {
                let (arg_exprs, runtime_id) = self.planned_call_args(expr_id, &args);
                self.lower_call(expr_id, callee, &arg_exprs, runtime_id, dest);
            }

            AstExpr::Array { elements } => {
                let operands: Vec<Operand> =
                    elements.iter().map(|&e| self.lower_to_operand(e)).collect();
                let element_ty = self.array_element_template(expr_id);
                self.builder
                    .assign(dest, Rvalue::Array(element_ty, operands));
            }

            AstExpr::Map { entries } => {
                let pairs: Vec<(Operand, Operand)> = entries
                    .iter()
                    .map(|entry| {
                        (
                            self.lower_to_operand(entry.key),
                            self.lower_to_operand(entry.value),
                        )
                    })
                    .collect();
                let (key_ty, value_ty) = self.map_kv_templates(expr_id);
                self.builder
                    .assign(dest, Rvalue::Map(key_ty, value_ty, pairs));
            }

            AstExpr::Object {
                type_name,
                type_args,
                fields,
                spreads,
                ..
            } => {
                self.lower_object(expr_id, &type_name, &type_args, &fields, &spreads, dest);
            }

            AstExpr::MemberAccess { base, member } => {
                self.lower_member_access(expr_id, base, &member, dest);
            }

            AstExpr::Upcast { base, .. } => {
                // `.as<I>` is a static type projection. Runtime representation
                // is the original value.
                self.lower_expr(base, dest);
            }

            AstExpr::GenericApply { base, type_args } => {
                self.lower_generic_apply(base, &type_args, dest);
            }

            AstExpr::OptionalMemberAccess { base, member } => {
                self.lower_optional_member_access(expr_id, base, &member, dest);
            }

            AstExpr::OptionalIndex { base, index } => {
                self.lower_optional_index(base, index, dest);
            }

            AstExpr::OptionalCall { callee, args } => {
                let (arg_exprs, runtime_id) = self.planned_call_args(expr_id, &args);
                self.lower_optional_call(expr_id, callee, &arg_exprs, runtime_id, dest);
            }

            AstExpr::Index { base, index } => {
                self.lower_index(base, index, dest);
            }

            AstExpr::Block { stmts, tail_expr } => {
                self.lower_scoped_block(&stmts, tail_expr, dest);
            }

            AstExpr::Match {
                scrutinee, arms, ..
            } => {
                let arms_owned = arms;
                self.lower_match(expr_id, scrutinee, &arms_owned, dest);
            }

            AstExpr::Is { scrutinee, pattern } => {
                // `<scrutinee> is <pattern>` — runtime pattern test that
                // yields `true` if the pattern matches, `false` otherwise.
                // We reuse `lower_pattern_test`, the same engine match-arm
                // dispatch uses, with two terminal blocks that write the
                // boolean constant into `dest` and jump to a join.
                let scrutinee_local = self.try_resolve_to_local(scrutinee).unwrap_or_else(|| {
                    let op = self.lower_to_operand(scrutinee);
                    let ty = self.expr_ty(scrutinee);
                    self.operand_to_local(op, ty)
                });

                let bb_true = self.builder.create_block();
                let bb_false = self.builder.create_block();
                let bb_join = self.builder.create_block();

                self.lower_pattern_test(scrutinee_local, pattern, bb_true, bb_false);

                self.builder.set_current_block(bb_true);
                self.builder.assign(
                    dest.clone(),
                    Rvalue::Use(Operand::Constant(Constant::Bool(true))),
                );
                self.builder.goto(bb_join);

                self.builder.set_current_block(bb_false);
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(Constant::Bool(false))));
                self.builder.goto(bb_join);

                self.builder.set_current_block(bb_join);
            }

            AstExpr::Catch { base, clauses } => {
                let clauses_owned = clauses;
                self.lower_catch(expr_id, base, &clauses_owned, &dest);
            }

            AstExpr::Throw { value } => {
                let val_op = self.lower_throw_operand(value);
                // Route every throw through the exception funnel (like
                // `AstStmt::Throw`) rather than a static jump to
                // `catch_context.unwind_target`. The funnel computes the
                // BEP-042 cause chain (`find_cause_context`) and materializes
                // the destination handler's `ErrorContext`; a static goto
                // bypasses both, so a `throw` in expression position inside a
                // `defer` region (or a `catch` arm/base) would drop its cause
                // and leave a bound `ctx` unmaterialized (B-611). The exception
                // table routes the throw to the same innermost handler the
                // static jump targeted — its region covers this PC — so control
                // flow is unchanged.
                if self.operand_is_marked_rethrow(&val_op) {
                    self.builder.rethrow(val_op);
                } else {
                    self.builder.throw(val_op);
                }
                // Start a dead block for any code after this (unreachable)
                let dead = self.builder.create_block();
                self.builder.set_current_block(dead);
            }

            AstExpr::Return { value } => {
                // A `return` expression (e.g. a braceless `catch`/`match` arm
                // value, `_ => return 0`) transfers control to the enclosing
                // function's exit. Unlike `throw`, it is NOT routed through
                // `catch_context` — it returns from the function rather than
                // being handled by the surrounding `catch`. This mirrors
                // `AstStmt::Return`; `dest` is never written because we diverge.
                let ret = Local(0); // _0 is always the return place
                if let Some(e) = value {
                    self.lower_expr(e, Place::local(ret));
                }
                // Run pending defers (LIFO) before jumping to the exit.
                self.replay_defers_to_depth(0);
                self.builder.goto(self.exit_block);
                // Subsequent code is unreachable; lower it into a dead block.
                let dead = self.builder.create_block();
                self.builder.set_current_block(dead);
            }

            AstExpr::Lambda(func_def) => {
                self.lower_lambda(&func_def, expr_id, dest);
            }

            AstExpr::OptionalChain { expr } => {
                self.lower_optional_chain(expr_id, expr, dest);
            }

            AstExpr::Missing => {
                self.emit_panic_call("parse error", expr_id);
            }

            AstExpr::Template { tag, segments } => match tag {
                baml_compiler2_ast::TemplateTag::Custom { tag, body } => {
                    self.lower_tagged_template(expr_id, tag, body, &segments, dest);
                }
                // Untagged (BEP §11): the value is the desugared `elaborated`
                // concat (built at AST lowering and type-checked by TIR). Lower
                // it directly — the structured `segments` were diagnostics-only.
                baml_compiler2_ast::TemplateTag::Default { elaborated } => {
                    self.lower_expr(elaborated, dest);
                }
            },

            AstExpr::Spawn {
                name,
                with_exprs,
                body,
            } => {
                self.lower_spawn(expr_id, name, &with_exprs, body, dest);
            }

            AstExpr::Await { future } => {
                self.lower_await(expr_id, future, dest);
            }
        }

        self.builder.current_source_span = prev_span;
    }

    fn operand_is_marked_rethrow(&self, operand: &Operand) -> bool {
        match operand {
            Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => {
                self.catch_rethrow_locals.contains(local)
            }
            Operand::Copy(_) | Operand::Move(_) | Operand::Constant(_) => false,
        }
    }

    /// Lower `spawn name? with? { body }` into:
    ///   1. A `MakeClosure` for the body wrapped as a 0-arg lambda.
    ///   2. A name temp (string operand or null constant).
    ///   3. An optional config operand from the `with baml.spawn.options(...)`
    ///      clause (BEP-034 spawn options).
    ///   4. A `Terminator::Spawn` writing the resulting Future handle.
    fn lower_spawn(
        &mut self,
        expr_id: AstExprId,
        name: Option<AstExprId>,
        with_exprs: &[AstExprId],
        body: AstExprId,
        dest: Place,
    ) {
        // The AST-lower step has already wrapped the spawn body in a
        // synthetic 0-arg `Expr::Lambda`. Lowering it through the
        // standard expression path emits a `MakeClosure` rvalue, which
        // is exactly what we want as the closure operand to `Spawn`.
        let closure_local = self.builder.temp(RuntimeTy::Null {
            attr: TyAttr::default(),
        });
        let closure_place = Place::Local(closure_local);
        self.lower_expr(body, closure_place.clone());
        let closure_op = Operand::Copy(closure_place);

        // Lower the optional name into an operand.
        let name_op = match name {
            Some(name_id) => self.lower_to_operand(name_id),
            None => Operand::Constant(Constant::Null),
        };

        // BEP-034 middleware: with transformers present, package the body
        // closure + name into a `baml.spawn.SpawnParams` instance, apply each
        // `with` expression to it left-to-right (each is a function
        // `(SpawnParams<T, E>) -> SpawnParams<U, F>`), and hand the FINAL
        // params to the spawn as the config operand. The engine reads
        // body/name/group/cancel/detach from its fields — a transformer may
        // have replaced any of them, including the body. Fields are built in
        // declaration order (the engine reads them BY INDEX; see
        // ns_spawn/spawn.baml).
        let config_op = if with_exprs.is_empty() {
            None
        } else {
            let params_local = self.builder.temp(RuntimeTy::Null {
                attr: TyAttr::default(),
            });
            self.builder.assign(
                Place::Local(params_local),
                Rvalue::Aggregate {
                    kind: AggregateKind::Class {
                        name: "baml.spawn.SpawnParams".to_string(),
                        type_arg_templates: Vec::new(),
                    },
                    fields: vec![
                        closure_op.clone(),
                        name_op.clone(),
                        Operand::Constant(Constant::Null),
                        Operand::Constant(Constant::Null),
                        Operand::Constant(Constant::Bool(false)),
                    ],
                },
            );
            let unwind = self.catch_context.as_ref().map(|c| c.unwind_target);
            let mut cur = params_local;
            for &with_id in with_exprs {
                let transformer_op = self.lower_to_operand(with_id);
                let next = self.builder.temp(RuntimeTy::Null {
                    attr: TyAttr::default(),
                });
                let resume = self.builder.create_block();
                self.builder.call(
                    transformer_op,
                    vec![Operand::Copy(Place::Local(cur))],
                    Place::Local(next),
                    resume,
                    unwind,
                );
                self.builder.set_current_block(resume);
                cur = next;
            }
            Some(Box::new(Operand::Copy(Place::Local(cur))))
        };

        // Allocate the future temp, typed as the `Future<T, E>` TIR inferred.
        let future_local = self.builder.temp(self.expr_ty(expr_id));
        let future_place = Place::Local(future_local);

        // The same `Future<T, E>`, as templates: the runtime resolves them
        // against the spawning frame's type args and stores the pair on the
        // heap `Future` for reflection and `is`/`match`.
        let future_ty = self.spawn_future_ty(expr_id);

        let resume = self.builder.create_block();
        self.builder.spawn(
            closure_op,
            name_op,
            config_op,
            future_ty,
            future_place.clone(),
            resume,
        );
        self.builder.set_current_block(resume);
        // The result of `spawn` is the Future handle.
        self.builder
            .assign(dest, Rvalue::Use(Operand::Copy(future_place)));
    }

    /// Lower `await expr` into a `Terminator::Await` whose destination is
    /// the awaited value.
    fn lower_await(&mut self, _expr_id: AstExprId, future: AstExprId, dest: Place) {
        let future_local = self.builder.temp(RuntimeTy::Null {
            attr: TyAttr::default(),
        });
        let future_place = Place::Local(future_local);
        self.lower_expr(future, future_place.clone());

        // `Terminator::Await` requires its destination to be `Place::Local`.
        // If the caller handed us a projection (field/index), await into a
        // temp local and then assign through to the projection — mirrors
        // how `lower_call` normalizes its destination.
        let (await_dest, projection_dest) = match dest {
            Place::Local(_) => (dest, None),
            projection => {
                let tmp = self.builder.temp(RuntimeTy::Null {
                    attr: TyAttr::default(),
                });
                (Place::Local(tmp), Some(projection))
            }
        };

        let resume = self.builder.create_block();
        let unwind = self.catch_context.as_ref().map(|c| c.unwind_target);
        self.builder
            .await_(future_place, await_dest.clone(), resume, unwind);
        self.builder.set_current_block(resume);

        if let Some(projection) = projection_dest {
            self.builder
                .assign(projection, Rvalue::Use(Operand::Copy(await_dest)));
        }
    }
}

// ─── Literal helper ───────────────────────────────────────────────────────────

impl LoweringContext<'_> {
    /// Whether `segments` is rooted at the BEP-044 `default` receiver keyword
    /// and that keyword is not shadowed by a local of the same name. See
    /// [`baml_compiler2_ast::DEFAULT_RECEIVER_KEYWORD`].
    fn is_default_receiver_root(&self, expr_id: AstExprId, segments: &[Name]) -> bool {
        segments
            .first()
            .is_some_and(|s| s.as_str() == baml_compiler2_ast::DEFAULT_RECEIVER_KEYWORD)
            && self.binding_id_for_path(expr_id, &segments[0]).is_none()
    }

    fn lower_literal(lit: &AstLiteral) -> Constant {
        use baml_base::Literal;
        match lit {
            Literal::Int(v) => Constant::Int(*v),
            Literal::Bigint(v) => Constant::Bigint(v.clone()),
            Literal::Float(s) => {
                // Literal::Float stores a string representation — parse to f64
                let v: f64 = s.parse().unwrap_or(0.0);
                Constant::Float(v)
            }
            Literal::String(v) => Constant::String(v.clone()),
            Literal::Bool(v) => Constant::Bool(*v),
        }
    }
}

// ─── 3.3: Path expression lowering ───────────────────────────────────────────

#[allow(clippy::elidable_lifetime_names)]
impl<'db> LoweringContext<'db> {
    fn lower_path_expr(&mut self, expr_id: AstExprId, segments: &[Name], dest: Place) {
        // Multi-segment paths (e.g. baml.http.fetch, self.field, obj.method) — check TIR resolution first
        if segments.len() > 1 {
            // Check path_member_resolutions first (set by infer_local_rooted_path for local-rooted paths).
            // This takes priority over the flat resolutions map since infer_local_rooted_path
            // moves resolutions from the flat map into path_member_resolutions.
            if let Some(member_resolutions) = self
                .tir_path_member_resolutions(self.expr_metadata_key(expr_id))
                .map(<[_]>::to_vec)
            {
                use crate::inference_provider::MemberResolution;
                // The last resolution corresponds to the final segment of the path.
                // - If the last resolution is a BoundMethod/UnboundMethod/Free, this path is a
                //   callee reference; emit a function constant. The receiver will be prepended
                //   by lower_call.
                // - If the last resolution is a Field, this is a pure field-chain access.
                // Note: for paths like `user.profile.items.slice`, the member_resolutions
                // are [Field{profile}, Field{items}, BoundMethod{slice}], so we check last().
                match member_resolutions.last() {
                    Some(MemberResolution::BoundMethod { .. }) => {
                        // Bound method reference: lower receiver and emit MakeBoundMethod.
                        let resolution = member_resolutions.into_iter().last().unwrap();
                        if let Some(item) = resolution_to_item_ref(self.db, &resolution) {
                            let receiver_segments = &segments[..segments.len() - 1];
                            let receiver_op = if receiver_segments.len() == 1 {
                                self.place_for_path(expr_id, &segments[0]).map_or_else(
                                    || Operand::Constant(Constant::Null),
                                    Operand::Copy,
                                )
                            } else {
                                // Multi-segment receiver (e.g. `cfg.encoder`): lower as field chain.
                                let recv_ty = self.expr_ty(expr_id);
                                let recv_local = self.builder.temp(recv_ty);
                                self.lower_multi_segment_path_as_field_chain(
                                    expr_id,
                                    receiver_segments,
                                    Place::local(recv_local),
                                );
                                Operand::Copy(Place::local(recv_local))
                            };
                            self.builder.assign(
                                dest,
                                Rvalue::MakeBoundMethod {
                                    item_ref: item,
                                    receiver: receiver_op,
                                },
                            );
                            return;
                        }
                    }
                    // A *value-rooted* interface-method reference (`let f = x.eq`)
                    // must capture the receiver and bind its impl at runtime — the
                    // virtual-bound path below handles it; a bare function constant
                    // would name an interface-keyed global that (for a required
                    // method) does not exist.
                    Some(
                        MemberResolution::InterfaceVirtualMethod { .. }
                        | MemberResolution::InterfaceConcreteMethod { .. },
                    ) if self.binding_id_for_path(expr_id, &segments[0]).is_some() => {}
                    Some(MemberResolution::External(external))
                        if matches!(
                            external.target,
                            baml_compiler2_hir_ty::callable::ExternalCallTarget::Interface { .. }
                        ) && self.binding_id_for_path(expr_id, &segments[0]).is_some() => {}
                    Some(MemberResolution::External(external))
                        if external.takes_self
                            && matches!(
                                external.target,
                                baml_compiler2_hir_ty::callable::ExternalCallTarget::Method { .. }
                            )
                            && self.binding_id_for_path(expr_id, &segments[0]).is_some() =>
                    {
                        let resolution = member_resolutions.into_iter().last().unwrap();
                        if let Some(item) = resolution_to_item_ref(self.db, &resolution) {
                            let receiver_segments = &segments[..segments.len() - 1];
                            let receiver_op = if receiver_segments.len() == 1 {
                                self.place_for_path(expr_id, &segments[0]).map_or_else(
                                    || Operand::Constant(Constant::Null),
                                    Operand::Copy,
                                )
                            } else {
                                let recv_ty = self.expr_ty(expr_id);
                                let recv_local = self.builder.temp(recv_ty);
                                self.lower_multi_segment_path_as_field_chain(
                                    expr_id,
                                    receiver_segments,
                                    Place::local(recv_local),
                                );
                                Operand::Copy(Place::local(recv_local))
                            };
                            self.builder.assign(
                                dest,
                                Rvalue::MakeBoundMethod {
                                    item_ref: item,
                                    receiver: receiver_op,
                                },
                            );
                            return;
                        }
                    }
                    Some(
                        MemberResolution::UnboundMethod { .. }
                        | MemberResolution::Free { .. }
                        | MemberResolution::InterfaceVirtualMethod { .. }
                        | MemberResolution::InterfaceConcreteMethod { .. }
                        | MemberResolution::External(_),
                    ) => {
                        // Unbound method or free function reference — emit a plain function constant.
                        let resolution = member_resolutions.into_iter().last().unwrap();
                        if let Some(item) = resolution_to_item_ref(self.db, &resolution) {
                            self.builder.assign(
                                dest,
                                Rvalue::Use(Operand::Constant(Constant::Function(item))),
                            );
                            return;
                        }
                    }
                    Some(
                        MemberResolution::Field { .. } | MemberResolution::ExternalField { .. },
                    ) => {
                        // Local-rooted field access — chain field projections.
                        self.lower_multi_segment_path_as_field_chain(expr_id, segments, dest);
                        return;
                    }
                    Some(
                        MemberResolution::Variant { .. }
                        | MemberResolution::InterfaceVirtualField { .. }
                        | MemberResolution::ExternalVariant { .. }
                        | MemberResolution::ExternalInterfaceVirtualField { .. },
                    ) => {
                        // Handled by expr_types check below (a virtual field read on an
                        // existential falls through to the general member-access lowering).
                    }
                    None => {}
                }
            }

            // Check flat resolutions (set by infer_multi_segment_path for package-rooted paths
            // like baml.fs.open, baml.env.get, etc.).
            if let Some(resolution) = self
                .tir_resolution(self.expr_metadata_key(expr_id))
                .cloned()
            {
                use crate::inference_provider::MemberResolution;
                match &resolution {
                    MemberResolution::BoundMethod { .. } => {
                        // Bound method reference via flat resolutions: emit MakeBoundMethod.
                        if let Some(item) = resolution_to_item_ref(self.db, &resolution) {
                            let receiver_segments = &segments[..segments.len() - 1];
                            let receiver_op = if receiver_segments.len() == 1 {
                                self.place_for_path(expr_id, &segments[0]).map_or_else(
                                    || Operand::Constant(Constant::Null),
                                    Operand::Copy,
                                )
                            } else {
                                let recv_ty = self.expr_ty(expr_id);
                                let recv_local = self.builder.temp(recv_ty);
                                self.lower_multi_segment_path_as_field_chain(
                                    expr_id,
                                    receiver_segments,
                                    Place::local(recv_local),
                                );
                                Operand::Copy(Place::local(recv_local))
                            };
                            self.builder.assign(
                                dest,
                                Rvalue::MakeBoundMethod {
                                    item_ref: item,
                                    receiver: receiver_op,
                                },
                            );
                            return;
                        }
                    }
                    // Value-rooted interface-method reference: see the
                    // `member_resolutions` match above — the virtual-bound path
                    // below captures the receiver and binds its impl at runtime.
                    MemberResolution::InterfaceVirtualMethod { .. }
                    | MemberResolution::InterfaceConcreteMethod { .. }
                        if self.binding_id_for_path(expr_id, &segments[0]).is_some() => {}
                    MemberResolution::External(external)
                        if matches!(
                            external.target,
                            baml_compiler2_hir_ty::callable::ExternalCallTarget::Interface { .. }
                        ) && self.binding_id_for_path(expr_id, &segments[0]).is_some() => {}
                    MemberResolution::External(external)
                        if external.takes_self
                            && matches!(
                                external.target,
                                baml_compiler2_hir_ty::callable::ExternalCallTarget::Method { .. }
                            )
                            && self.binding_id_for_path(expr_id, &segments[0]).is_some() =>
                    {
                        if let Some(item) = resolution_to_item_ref(self.db, &resolution) {
                            let receiver_segments = &segments[..segments.len() - 1];
                            let receiver_op = if receiver_segments.len() == 1 {
                                self.place_for_path(expr_id, &segments[0]).map_or_else(
                                    || Operand::Constant(Constant::Null),
                                    Operand::Copy,
                                )
                            } else {
                                let recv_ty = self.expr_ty(expr_id);
                                let recv_local = self.builder.temp(recv_ty);
                                self.lower_multi_segment_path_as_field_chain(
                                    expr_id,
                                    receiver_segments,
                                    Place::local(recv_local),
                                );
                                Operand::Copy(Place::local(recv_local))
                            };
                            self.builder.assign(
                                dest,
                                Rvalue::MakeBoundMethod {
                                    item_ref: item,
                                    receiver: receiver_op,
                                },
                            );
                            return;
                        }
                    }
                    MemberResolution::UnboundMethod { .. }
                    | MemberResolution::Free { .. }
                    | MemberResolution::InterfaceVirtualMethod { .. }
                    | MemberResolution::InterfaceConcreteMethod { .. }
                    | MemberResolution::External(_) => {
                        if let Some(item) = resolution_to_item_ref(self.db, &resolution) {
                            self.builder.assign(
                                dest,
                                Rvalue::Use(Operand::Constant(Constant::Function(item))),
                            );
                            return;
                        }
                    }
                    MemberResolution::Variant { .. }
                    | MemberResolution::InterfaceVirtualField { .. }
                    | MemberResolution::ExternalVariant { .. }
                    | MemberResolution::ExternalInterfaceVirtualField { .. } => {
                        // Handled by expr_types check below (a virtual field read on an
                        // existential falls through to the general lowering).
                    }
                    MemberResolution::Field { .. } | MemberResolution::ExternalField { .. } => {
                        // Local-rooted field access — chain field projections.
                        // The root segment is a local; chain through class fields.
                        self.lower_multi_segment_path_as_field_chain(expr_id, segments, dest);
                        return;
                    }
                }
            }
            // An interface method referenced as a *value* on a generic- or
            // interface-typed receiver (`let f = x.eq`): no single concrete method
            // exists statically, so bind the receiver's impl method by its runtime
            // type at bind time — the value analogue of `lower_call`'s virtual
            // dispatch. The declaring interface is resolved *before* lowering the
            // receiver so a field access (no such method) falls through without
            // lowering the prefix twice.
            //
            // Unlike the direct-call path this does not strip a trailing
            // type-qualifier segment (`x.Iface.method`): a qualified method
            // *reference* resolves through TIR's `member_resolutions` / flat
            // `resolutions` above and returns before reaching here, so the
            // receiver is always `segments[..len-1]`.
            if segments.len() >= 2
                && let Some(recv_root_local) = self.local_for_path(expr_id, &segments[0])
            {
                let method_name = segments.last().unwrap().clone();
                let recv_seg_idx = if segments.len() == 2 {
                    0
                } else {
                    segments.len() - 2
                };
                let recv_tir_ty = self
                    .tir_path_segment_type((self.current_metadata_scope, expr_id, recv_seg_idx))
                    .cloned();
                if let Some(view) = recv_tir_ty
                    .as_ref()
                    .and_then(|ty| self.interface_dispatch_target_for_member(ty, &method_name))
                    .or_else(|| {
                        recv_tir_ty
                            .as_ref()
                            .and_then(|ty| self.dispatch_target_for_concrete(ty, &method_name))
                    })
                    && self.mir_interface_declares_method(&view.0, &method_name)
                {
                    let receiver_segments = &segments[..segments.len() - 1];
                    let recv_local = self.lower_path_receiver_to_local(
                        expr_id,
                        receiver_segments,
                        recv_root_local,
                    );
                    self.emit_virtual_bound_method(recv_local, &view, &method_name, &dest);
                    return;
                }
            }
            if self
                .binding_id_for_path(expr_id, &segments[0])
                .is_some()
                // BEP-044 wf3 #4: `default.<field>` as a value — the field-chain
                // lowerer maps the `default` root to `self`-viewed-as-interface.
                // (The `default.method(...)` call form is intercepted earlier in
                // `lower_call`, so this only catches the value/field form.)
                || self.is_default_receiver_root(expr_id, segments)
            {
                self.lower_multi_segment_path_as_field_chain(expr_id, segments, dest);
                return;
            }
            // Check for enum variant (e.g. Status.Active lowered to Path(["Status","Active"]))
            if let Some(Tir2Ty::EnumVariant(qtn, variant, _)) = self
                .tir_expr_type(self.expr_metadata_key(expr_id))
                .cloned()
                .as_ref()
            {
                let enum_ref = ItemRef::EnumType {
                    package: qtn.package().clone(),
                    namespace: qtn.namespace().clone(),
                    name: qtn.name().clone(),
                };
                self.builder.assign(
                    dest,
                    Rvalue::Use(Operand::Constant(Constant::EnumVariant {
                        enum_ref,
                        variant: variant.clone(),
                    })),
                );
                return;
            }
            // Namespace intermediate or unresolved — emit null placeholder.
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
            return;
        }

        let name = &segments[0];
        if name.as_str() == "$id" {
            self.lower_current_runtime_id(dest);
            return;
        }

        if let Some(place) = self.place_for_path(expr_id, name) {
            self.builder.assign(dest, Rvalue::Use(Operand::Copy(place)));
            return;
        }
        if self.binding_id_for_path(expr_id, name).is_some() {
            self.emit_panic_call(&format!("unresolved local: {name}"), expr_id);
            return;
        }

        let span_start = self
            .source_map
            .as_ref()
            .map(|sm| sm.expr_span(expr_id).start())
            .unwrap_or_default();

        let resolved = resolve_name_at_in_scope(
            self.db,
            self.file,
            span_start,
            name,
            self.scope_func_name.as_ref(),
        );
        match resolved {
            ResolvedName::Item(def) => {
                self.lower_item_ref(expr_id, def, dest);
            }
            ResolvedName::Builtin(def) => {
                let item = def_to_item_ref(self.db, def);
                self.builder.assign(
                    dest,
                    Rvalue::Use(Operand::Constant(Constant::Function(item))),
                );
            }
            ResolvedName::Local { .. } | ResolvedName::Unknown => {
                if self
                    .tir_expr_type(self.expr_metadata_key(expr_id))
                    .is_some()
                {
                    // If TIR recorded a type for this expr, it was handled as a
                    // package path intermediate (e.g. `baml` in
                    // `baml.HttpMethod.Get`). Emit a null placeholder — the outer
                    // FieldAccess will produce the real value.
                    self.builder
                        .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
                } else {
                    let msg = format!("unresolved name: {name}");
                    self.emit_panic_call(&msg, expr_id);
                }
            }
        }
    }

    /// Lower a multi-segment `Path` expression (`a.b.c`) as chained field projections.
    ///
    /// The first segment is resolved as a local variable; subsequent segments are
    /// projected as struct fields (using `class_fields`) or map keys (fallback).
    fn lower_multi_segment_path_as_field_chain(
        &mut self,
        expr_id: AstExprId,
        segments: &[Name],
        dest: Place,
    ) {
        let root_place = self.place_for_path(expr_id, &segments[0]);
        let (mut current_place, mut current_ty) = if let Some(place) = root_place {
            let ty = match place {
                Place::Local(root_local) => {
                    if let Some(tir_root) = self.path_root_ty(expr_id) {
                        // If TIR inferred a more specific type for the root local,
                        // update the MIR local's declared type so the emitter can
                        // resolve field names for display (e.g. `load_field .index`).
                        if matches!(
                            self.builder.local_ty(root_local),
                            RuntimeTy::BuiltinUnknown { .. }
                        ) && !matches!(
                            tir_root,
                            RuntimeTy::BuiltinUnknown { .. } | RuntimeTy::Void { .. }
                        ) {
                            self.builder.local_decl_mut(root_local).ty = tir_root.clone();
                        }
                        tir_root
                    } else {
                        self.builder.local_ty(root_local)
                    }
                }
                Place::Capture(_) => {
                    self.path_root_ty(expr_id)
                        .unwrap_or_else(|| RuntimeTy::BuiltinUnknown {
                            attr: TyAttr::default(),
                        })
                }
                _ => unreachable!("path roots are locals or captures"),
            };
            (place, ty)
        } else if let Some(root_ty) = self.path_root_ty(expr_id)
            && let Some(definition) = {
                let span_start = self
                    .source_map
                    .as_ref()
                    .map(|source_map| source_map.expr_span(expr_id).start())
                    .unwrap_or_default();
                match resolve_name_at_in_scope(
                    self.db,
                    self.file,
                    span_start,
                    &segments[0],
                    self.scope_func_name.as_ref(),
                ) {
                    ResolvedName::Item(definition @ Definition::Let(_)) => Some(definition),
                    _ => None,
                }
            }
        {
            // Persistent Session bindings are initialized globals, not lexical
            // locals. Load the root once into a temp so the normal field-chain
            // lowering can project its class/interface members.
            let root_local = self.builder.temp(root_ty.clone());
            self.lower_item_ref(expr_id, definition, Place::local(root_local));
            (Place::Local(root_local), root_ty)
        } else if self.is_default_receiver_root(expr_id, segments)
            && let Some(&self_local) = self.locals.get(&Name::new("self"))
        {
            // BEP-044 wf3 #4: `default.<field>` denotes the enclosing `self`
            // viewed at the declaring interface. TIR typed the root as
            // `RuntimeTy::Interface`, so reuse that and let the interface-prefix
            // routing below resolve the field view (same path as
            // `self.as<I>.field`). Without this the `default` root is not a
            // local -> null -> `string + null` VM crash.
            let place = Place::Local(self_local);
            let ty = self
                .path_root_ty(expr_id)
                .unwrap_or_else(|| self.builder.local_ty(self_local));
            (place, ty)
        } else {
            // Root not found as a local or capture; emit null.
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
            return;
        };

        let mut skip_next_segment = false;
        for (offset, seg) in segments[1..].iter().enumerate() {
            if skip_next_segment {
                skip_next_segment = false;
                continue;
            }
            let seg_idx = offset + 1;
            let is_last = seg_idx + 1 == segments.len();
            let interface_prefix =
                self.interface_receiver_for_path_prefix(expr_id, seg_idx - 1, seg, &current_ty);
            if let Some((tn, class_type_args)) =
                self.class_receiver_for_path_prefix(expr_id, seg_idx - 1, &current_ty)
            {
                if let Some(fields) = self.class_fields.get(&tn) {
                    if let Some(&idx) = fields.get(seg.as_str()) {
                        // Substitute the receiver's class type-args into the
                        // declared field type so chained access through generic
                        // positions (`b.value.name` where `b: Box<User>`)
                        // produces `RuntimeTy::Class(User, ...)` rather than the
                        // erased runtime metadata. Without this, the next iteration
                        // falls through to the dynamic map-key path below and the
                        // VM hits `expected Map, got Instance`.
                        let next_ty = self.class_field_ty(&tn, seg, &class_type_args);
                        current_place = Place::Field {
                            base: Box::new(current_place),
                            field: idx,
                        };
                        current_ty = next_ty;
                        continue;
                    }
                    if !is_last {
                        let qualified = Name::new(format!("{}.{}", seg, segments[seg_idx + 1]));
                        if let Some(&idx) = fields.get(qualified.as_str()) {
                            let next_ty = self.class_field_ty(&tn, &qualified, &class_type_args);
                            current_place = Place::Field {
                                base: Box::new(current_place),
                                field: idx,
                            };
                            current_ty = next_ty;
                            skip_next_segment = true;
                            continue;
                        }
                    }
                }
            }

            let target_ty = self.path_segment_ty(expr_id, seg_idx).unwrap_or_else(|| {
                RuntimeTy::BuiltinUnknown {
                    attr: TyAttr::default(),
                }
            });
            let target_place = if is_last {
                dest.clone()
            } else {
                Place::local(self.builder.temp(target_ty.clone()))
            };
            let base_local = match current_place.clone() {
                Place::Local(local) => local,
                place => {
                    let local = self.builder.temp(current_ty.clone());
                    self.builder
                        .assign(Place::local(local), Rvalue::Use(Operand::Copy(place)));
                    local
                }
            };
            if let Some((iface_tn, iface_type_args, iface_assoc)) = interface_prefix
                && self.try_lower_interface_field_access(
                    base_local,
                    &iface_tn,
                    &iface_type_args,
                    &iface_assoc,
                    seg,
                    &target_place,
                )
            {
                if is_last {
                    return;
                }
                current_place = target_place;
                current_ty = target_ty;
                continue;
            }
            if self.lower_union_class_field_access(
                expr_id,
                base_local,
                &current_ty,
                seg,
                &target_place,
            ) {
                if is_last {
                    return;
                }
                current_place = target_place;
                current_ty = target_ty;
                continue;
            }

            // Dynamic map key fallback
            let key_local = self.builder.temp(RuntimeTy::String {
                attr: TyAttr::default(),
            });
            self.builder.assign(
                Place::local(key_local),
                Rvalue::Use(Operand::Constant(Constant::String(seg.to_string()))),
            );
            current_place = Place::Index {
                base: Box::new(current_place),
                index: key_local,
                kind: IndexKind::Map,
            };
            break;
        }

        self.builder
            .assign(dest, Rvalue::Use(Operand::Copy(current_place)));
    }

    /// Look up the MIR type of a named field on a class, for chained field access.
    ///
    /// `class_type_args` are the type-args carried on the receiver's
    /// `RuntimeTy::Class(tn, class_type_args, _)` (e.g. `[User]` for `Box<User>`).
    /// They are substituted into the declared field type so a generic-typed
    /// position (`item: T` in `Container<T>`) resolves to the concrete
    /// receiver-side binding rather than `RuntimeTy::Void`.
    ///
    /// Returns `RuntimeTy::Null` if the field is not found or the type cannot be
    /// resolved.  Called by `lower_multi_segment_path_as_field_chain` to
    /// track the type through a chain of field projections (`a.b.c` needs
    /// the type of `b` to find `c`).
    fn class_field_ty(
        &self,
        class_tn: &TypeName,
        field_name: &Name,
        class_type_args: &[RuntimeTy],
    ) -> RuntimeTy {
        use baml_compiler2_hir::{contributions::Definition, package::package_items};
        let db = self.db;

        let pkg_name = class_tn.package();
        let pkg_id = baml_compiler2_hir::package::PackageId::new(db, pkg_name.clone());
        let pkg_items_ref = package_items(db, pkg_id);

        let namespace: Vec<Name> = class_tn.namespace().clone();

        let Some(Definition::Class(class_loc)) =
            pkg_items_ref.lookup_type(&namespace, class_tn.name())
        else {
            return RuntimeTy::Null {
                attr: TyAttr::default(),
            };
        };

        let class_data = baml_compiler2_ppir::item_data::class_data(db, class_loc);
        let class_generic_params = baml_compiler2_hir_ty::lower::class_generic_frame(db, class_loc);

        let field = class_data.fields.iter().find(|f| &f.name == field_name);
        let Some(field) = field else {
            return RuntimeTy::Null {
                attr: TyAttr::default(),
            };
        };
        let type_ref = field.type_ref;

        let pkg_ns =
            baml_compiler2_hir::file_package::file_package(db, class_loc.file(db)).namespace_path;
        let tir_ty = lower_ref_in_scope(
            db,
            &class_data.type_refs,
            type_ref,
            pkg_items_ref,
            &pkg_ns,
            &class_generic_params,
            &baml_compiler2_hir_ty::lower::class_generic_bounds(db, class_loc),
            None,
        );
        // Build a TyTemplate with `TypeArgRef(N)` for each class-level
        // generic param, then substitute `class_type_args` so a field
        // declared as `T` resolves to the concrete receiver-side binding.
        let template = tir2_to_template(&tir_ty, self.resolved_aliases, &class_generic_params);
        template.substitute_symbolic(class_type_args)
    }

    fn lower_item_ref(&mut self, expr_id: AstExprId, def: Definition<'db>, dest: Place) {
        let item = def_to_item_ref(self.db, def);
        // Check if this expression's type is EnumVariant
        if let Some(Tir2Ty::EnumVariant(_qtn, variant, _)) = self
            .tir_expr_type(self.expr_metadata_key(expr_id))
            .cloned()
            .as_ref()
        {
            let variant_name = variant.clone();
            // Convert the Free item ref to an EnumType variant
            let enum_ref = match item {
                ItemRef::Free {
                    package,
                    namespace,
                    name,
                } => ItemRef::EnumType {
                    package,
                    namespace,
                    name,
                },
                other => other,
            };
            self.builder.assign(
                dest,
                Rvalue::Use(Operand::Constant(Constant::EnumVariant {
                    enum_ref,
                    variant: variant_name,
                })),
            );
            return;
        }
        // A function reference becomes a pooled function-value wrapper at
        // emit; any other item (a client, a top-level `let`, a template
        // string, ...) is a plain read of the global slot `$init` filled.
        let constant = match def {
            Definition::Function(_) => Constant::Function(item),
            _ => Constant::GlobalItem(item),
        };
        self.builder
            .assign(dest, Rvalue::Use(Operand::Constant(constant)));
    }
}

// ─── 3.4: Operator mapping and binary/unary lowering ─────────────────────────

impl LoweringContext<'_> {
    fn convert_binop(op: AstBinaryOp) -> Option<BinOp> {
        match op {
            AstBinaryOp::Add => Some(BinOp::Add),
            AstBinaryOp::Sub => Some(BinOp::Sub),
            AstBinaryOp::Mul => Some(BinOp::Mul),
            AstBinaryOp::Div => Some(BinOp::Div),
            AstBinaryOp::Mod => Some(BinOp::Mod),
            AstBinaryOp::Eq => Some(BinOp::Eq),
            AstBinaryOp::Ne => Some(BinOp::Ne),
            AstBinaryOp::Lt => Some(BinOp::Lt),
            AstBinaryOp::Le => Some(BinOp::Le),
            AstBinaryOp::Gt => Some(BinOp::Gt),
            AstBinaryOp::Ge => Some(BinOp::Ge),
            AstBinaryOp::BitAnd => Some(BinOp::BitAnd),
            AstBinaryOp::BitOr => Some(BinOp::BitOr),
            AstBinaryOp::BitXor => Some(BinOp::BitXor),
            AstBinaryOp::Shl => Some(BinOp::Shl),
            AstBinaryOp::Shr => Some(BinOp::Shr),
            // Short-circuit operators handled separately
            AstBinaryOp::And | AstBinaryOp::Or => None,
            // Null coalescing desugars to control flow, not a binary op
            AstBinaryOp::NullCoalesce => None,
        }
    }

    fn lower_binary(
        &mut self,
        expr_id: AstExprId,
        op: AstBinaryOp,
        lhs: AstExprId,
        rhs: AstExprId,
        dest: Place,
    ) {
        match op {
            AstBinaryOp::And => {
                return self.lower_short_circuit(expr_id, lhs, rhs, dest, true);
            }
            AstBinaryOp::Or => {
                return self.lower_short_circuit(expr_id, lhs, rhs, dest, false);
            }
            AstBinaryOp::NullCoalesce => {
                return self.lower_null_coalesce(expr_id, lhs, rhs, dest);
            }
            _ => {}
        }

        // Check if TIR already folded this expression to a literal constant
        if self.opt >= crate::OptLevel::Two {
            if let RuntimeTy::Literal(ref lit, _, _) = self.expr_ty(expr_id) {
                let constant = Self::lower_literal(lit);
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(constant)));
                return;
            }
        }

        // `==`/`!=`: the `baml.ops.equals_equals` driver is the always-correct
        // general case — it compares the operands' concrete runtime types and
        // dispatches a custom `Equals` when present. The specialized comparison
        // opcode is only an equivalent optimization when both operands are the
        // *same* primitive (value comparison == the native `Equals`), so keep it
        // there and route everything else through the driver.
        if matches!(op, AstBinaryOp::Eq | AstBinaryOp::Ne)
            && !self.equality_uses_primitive_opcode(lhs, rhs)
        {
            self.lower_equality_via_driver(op, lhs, rhs, dest);
            return;
        }

        // Arithmetic over non-primitive operands (a user type, or a union /
        // interface-existential / type variable involving one) dispatches through
        // the matching `baml.ops` interface, resolved at runtime from the
        // operands' concrete types by the `__union_*` driver. Primitive operands
        // keep the specialized/`exec_binop` fast path below.
        if matches!(
            op,
            AstBinaryOp::Add
                | AstBinaryOp::Sub
                | AstBinaryOp::Mul
                | AstBinaryOp::Div
                | AstBinaryOp::Mod
        ) && !self.arithmetic_uses_primitive_opcode(lhs, rhs)
        {
            self.lower_arithmetic_via_driver(expr_id, op, lhs, rhs, dest);
            return;
        }

        // Ordering over operands the comparison opcodes cannot order (`bool`, an
        // enum or class implementing `Compare`, or a type variable / `Self` /
        // projection that realizes to one) dispatches through `baml.ops.Compare`,
        // resolved at runtime from the receiver's concrete type. Unlike the
        // arithmetic operators this needs no `__union_*` driver: `Compare` is
        // *single* dispatch (`other: Self`), so the receiver alone picks the impl.
        if let Some(method) = Self::ordering_method(op)
            && !self.ordering_uses_primitive_opcode(lhs, rhs)
        {
            self.lower_ordering_via_virtual_call(method, lhs, rhs, dest);
            return;
        }

        // Mixed `int OP bigint` (or `bigint OP int`) operators resolve the
        // `int` operand to a small local `BigInt` in the VM (the specialized
        // `*Bigint`/`CmpBigint` opcodes accept a lone `int` operand), without
        // allocating a heap bigint. `int` is not a subtype of `bigint` and
        // there is no implicit move coercion — only these operators and the FFI
        // boundary convert — so lower both operands naturally.
        let left = self.lower_to_operand(lhs);
        let right = self.lower_to_operand(rhs);
        if let Some(mir_op) = Self::convert_binop(op) {
            self.builder.assign(
                dest,
                Rvalue::BinaryOp {
                    op: mir_op,
                    left,
                    right,
                },
            );
        } else {
            // Fallback — shouldn't happen for well-typed code
            self.emit_panic_call("unsupported binary op", expr_id);
        }
    }

    /// Whether both operands of an `==`/`!=` are the *same* primitive type, so
    /// the specialized comparison opcode is equivalent to the `equals_equals`
    /// driver (value comparison matches the unoverridable native `Equals`).
    /// Literals widen to their base primitive; everything else (mixed primitives,
    /// `uint8array`, enums, classes, containers, unions, interfaces, `unknown`)
    /// goes through the driver.
    fn equality_uses_primitive_opcode(&self, lhs: AstExprId, rhs: AstExprId) -> bool {
        fn prim_class(ty: &RuntimeTy) -> Option<u8> {
            use baml_base::Literal;
            Some(match ty {
                RuntimeTy::Int { .. } | RuntimeTy::Literal(Literal::Int(_), _, _) => 0,
                RuntimeTy::Bigint { .. } | RuntimeTy::Literal(Literal::Bigint(_), _, _) => 1,
                RuntimeTy::Float { .. } | RuntimeTy::Literal(Literal::Float(_), _, _) => 2,
                RuntimeTy::String { .. } | RuntimeTy::Literal(Literal::String(_), _, _) => 3,
                RuntimeTy::Bool { .. } | RuntimeTy::Literal(Literal::Bool(_), _, _) => 4,
                RuntimeTy::Null { .. } => 5,
                _ => return None,
            })
        }
        let l = prim_class(&self.expr_ty(lhs));
        l.is_some() && l == prim_class(&self.expr_ty(rhs))
    }

    /// Lower `==`/`!=` through the `baml.ops.equals_equals` driver — the general
    /// case (concrete-type comparison + custom `Equals` dispatch). The driver may
    /// yield (it can call a user `eq`), so the call splits the block. `!=` negates
    /// the `==` result.
    //
    // BUG: `!=` never dispatches `Equals.neq`, so a type that overrides `neq`
    // inconsistently with `eq` sees the override ignored by the operator (it is
    // only reachable as `a.neq(b)`). Unlike ordering — which is gated on a single
    // concrete `Compare` type and so can dispatch the interface method directly —
    // `==`/`!=` accept arbitrary operand pairs, which have no shared `Equals` to
    // dispatch through. Fixing it therefore means deciding what `!=` should mean
    // across type boundaries, not just changing the lowering.
    fn lower_equality_via_driver(
        &mut self,
        op: AstBinaryOp,
        lhs: AstExprId,
        rhs: AstExprId,
        dest: Place,
    ) {
        let lhs_op = self.lower_to_operand(lhs);
        let rhs_op = self.lower_to_operand(rhs);
        let bool_ty = RuntimeTy::Bool {
            attr: TyAttr::default(),
        };
        if matches!(op, AstBinaryOp::Eq) {
            self.lower_via_ops_driver("equals_equals", vec![lhs_op, rhs_op], bool_ty, dest);
            return;
        }
        // `!=`: call into a bool temp, then negate into `dest` (`assign` handles
        // projection destinations, so this covers both local and projection cases).
        let eq_dest = Place::local(self.builder.temp(bool_ty.clone()));
        self.lower_via_ops_driver(
            "equals_equals",
            vec![lhs_op, rhs_op],
            bool_ty,
            eq_dest.clone(),
        );
        self.builder.assign(
            dest,
            Rvalue::UnaryOp {
                op: crate::UnaryOp::Not,
                operand: Operand::Copy(eq_dest),
            },
        );
    }

    /// Emit `dest = baml.ops.<driver>(args)` — the shared shape of the operator
    /// dispatch drivers (`equals_equals`, `__union_add`, …, `__union_neg`). A
    /// driver may yield (it can call user bytecode), so the call splits the block
    /// and lowering resumes in a fresh one. The call terminator's destination
    /// must be a `Place::Local` (the emitter stores its result with
    /// `emit_store_place`, which only handles locals), so a projection/capture
    /// `dest` is routed through a `result_ty`-typed temp and copied through.
    fn lower_via_ops_driver(
        &mut self,
        driver: &str,
        args: Vec<Operand>,
        result_ty: RuntimeTy,
        dest: Place,
    ) {
        let callee = Operand::Constant(Constant::Function(ItemRef::Free {
            package: Name::new("baml"),
            namespace: vec![Name::new("ops")],
            name: Name::new(driver),
        }));
        let unwind = self.catch_context.as_ref().map(|c| c.unwind_target);
        let needs_temp = !matches!(dest, Place::Local(_));
        let call_dest = if needs_temp {
            Place::local(self.builder.temp(result_ty))
        } else {
            dest.clone()
        };
        let resume = self.builder.create_block();
        self.builder
            .call(callee, args, call_dest.clone(), resume, unwind);
        self.builder.set_current_block(resume);
        if needs_temp {
            self.builder
                .assign(dest, Rvalue::Use(Operand::Copy(call_dest)));
        }
    }

    /// Whether `ty` is a primitive the specialized opcodes handle directly —
    /// int/bigint/float, plus `string` when `include_string` (binary `+`
    /// concatenates; unary `-` has no string form).
    /// A literal counts as its base; a union counts only when every member is
    /// the SAME primitive kind (`int | 3`): a mixed-kind union (`int | float`)
    /// would let emit pick a single-kind opcode for a value of the other kind —
    /// UB in the specialized handlers — so it takes the interface route instead,
    /// as does anything else (a user type, or a union / existential / type
    /// variable involving one). TIR has already validated the operation through
    /// the interface registry; this only chooses the lowering route.
    ///
    /// Shared by three operator families, which reach different interface routes
    /// when it says no: arithmetic and unary negation go to the `baml.ops`
    /// `__union_*` drivers ([`Self::arithmetic_uses_primitive_opcode`],
    /// [`Self::negate_uses_primitive_opcode`]), while ordering dispatches
    /// `baml.ops.Compare` directly ([`Self::ordering_uses_primitive_opcode`],
    /// which additionally rejects `null`). `exec_binop` and `exec_cmpop` are the
    /// runtime counterparts; ordering is the narrower of the two, so a change to
    /// either handler's supported set has to be reflected here.
    fn arith_primitive(ty: &RuntimeTy, include_string: bool) -> bool {
        /// The primitive kind of a non-union member, literal widened to base.
        /// The builtin wrapper classes (`baml.Float`, etc.) count as their
        /// primitive — `self` inside their method bodies is class-typed but
        /// primitive-valued.
        fn kind(ty: &RuntimeTy, include_string: bool) -> Option<PrimitiveType> {
            let primitive = match ty {
                RuntimeTy::Int { .. } => PrimitiveType::Int,
                RuntimeTy::Bigint { .. } => PrimitiveType::Bigint,
                RuntimeTy::Float { .. } => PrimitiveType::Float,
                RuntimeTy::String { .. } => PrimitiveType::String,
                RuntimeTy::Literal(literal, _, _) => PrimitiveType::from_literal(literal),
                RuntimeTy::Class(name, args, _) if args.is_empty() => name.builtin_primitive()?,
                _ => return None,
            };
            match primitive {
                PrimitiveType::Int | PrimitiveType::Bigint | PrimitiveType::Float => {
                    Some(primitive)
                }
                PrimitiveType::String if include_string => Some(primitive),
                _ => None,
            }
        }
        match ty {
            RuntimeTy::Union(members, _) => {
                // `null` members are transparent: only a chain-narrowed
                // compound-assign target still carries `| null` here (its
                // null case never reaches the op — the `?.` guard skips it),
                // and binary operands with a possible `null` are rejected by
                // TIR before lowering.
                let mut first = None;
                members
                    .iter()
                    .filter(|m| !matches!(m, RuntimeTy::Null { .. }))
                    .all(|m| match kind(m, include_string) {
                        Some(kind) => first.get_or_insert(kind) == &kind,
                        None => false,
                    })
                    && first.is_some()
            }
            _ => kind(ty, include_string).is_some(),
        }
    }

    /// Whether both arithmetic operands are [`Self::arith_primitive`] (string
    /// included: `+` concatenates, and TIR rejects strings under the other ops
    /// before lowering).
    fn arithmetic_uses_primitive_opcode(&self, lhs: AstExprId, rhs: AstExprId) -> bool {
        Self::arith_primitive(&self.expr_ty(lhs), true)
            && Self::arith_primitive(&self.expr_ty(rhs), true)
    }

    /// Lower `a OP b` through the `baml.ops.__union_<op>` driver — the general
    /// case for operands whose static types don't pin a single impl. The driver
    /// resolves `<typeof a as Op<typeof b>>` at runtime and tail-calls it; its
    /// `unknown` result is the operator's value, re-typed by `dest`.
    fn lower_arithmetic_via_driver(
        &mut self,
        expr_id: AstExprId,
        op: AstBinaryOp,
        lhs: AstExprId,
        rhs: AstExprId,
        dest: Place,
    ) {
        let driver = match op {
            AstBinaryOp::Add => "__union_add",
            AstBinaryOp::Sub => "__union_sub",
            AstBinaryOp::Mul => "__union_mul",
            AstBinaryOp::Div => "__union_div",
            AstBinaryOp::Mod => "__union_rem",
            _ => unreachable!("lower_arithmetic_via_driver: non-arithmetic op {op:?}"),
        };
        let lhs_op = self.lower_to_operand(lhs);
        let rhs_op = self.lower_to_operand(rhs);
        let result_ty = self.expr_ty(expr_id);
        self.lower_via_ops_driver(driver, vec![lhs_op, rhs_op], result_ty, dest);
    }

    /// Whether both ordering operands are primitives the comparison opcodes can
    /// *order*: int, bigint, float, string. That reduces to
    /// [`Self::arith_primitive`] with `include_string` — which also admits the
    /// spellings of those four (a literal, a same-kind union like `int | 3`, and
    /// the builtin companion classes) — so the predicate is shared rather than
    /// duplicated. `exec_cmpop` orders exactly those four and treats every other
    /// pair (`bool`, `uint8array`, enum variants, class instances, …) as
    /// equality-only. `bool` falls out on its own — `PrimitiveType::Bool` is not
    /// one of the arithmetic kinds — which is what routes it to the `Compare`
    /// impl the stdlib declares for it.
    ///
    /// Preconditions, both owed by TIR's ordering check and *not* re-derived
    /// here: the two operands have the same type, and that type implements
    /// `baml.ops.Compare`. The second is what makes the interface route correct
    /// for everything this predicate rejects. Note the predicate tests each
    /// operand independently, so it leans on the first precondition — a mixed
    /// pair such as `int < string` would take the opcode path and fault, but TIR
    /// rejects it before lowering.
    ///
    /// The `null` guard is defense in depth rather than a fix: `null` has no
    /// `Compare` impl, so neither route can order it and TIR rejects it outright.
    /// It is here because [`Self::arith_primitive`] deliberately treats a `null`
    /// union member as *transparent* — a carve-out for chain-narrowed
    /// compound-assign targets, which ordering has no form of — and inheriting
    /// that silently would make `int | null` look opcode-orderable.
    ///
    /// Reads `expr_ty` (TIR types), where a `Self`-annotated parameter in a
    /// concrete `implements` block has already been resolved to the block's
    /// subject — the *MIR local* type keeps the unresolved `Self` (see
    /// `lower_signature_runtime_ty`), and reading that instead would deoptimize
    /// `baml.Comparable$for$int.compare` and friends off the opcode path.
    fn ordering_uses_primitive_opcode(&self, lhs: AstExprId, rhs: AstExprId) -> bool {
        /// `arith_primitive`, minus the `null`-transparency carve-out.
        fn orderable(ty: &RuntimeTy) -> bool {
            let has_null = match ty {
                RuntimeTy::Union(members, _) => {
                    members.iter().any(|m| matches!(m, RuntimeTy::Null { .. }))
                }
                RuntimeTy::Null { .. } => true,
                _ => false,
            };
            !has_null && LoweringContext::arith_primitive(ty, true)
        }
        orderable(&self.expr_ty(lhs)) && orderable(&self.expr_ty(rhs))
    }

    /// The `baml.ops.Compare` method an ordering operator dispatches, or `None`
    /// for any other operator. Single source for both the route test in
    /// [`Self::lower_binary`] and the dispatched method name, so the two cannot
    /// disagree about which operators are orderings.
    fn ordering_method(op: AstBinaryOp) -> Option<&'static str> {
        match op {
            AstBinaryOp::Lt => Some("lt"),
            AstBinaryOp::Le => Some("le"),
            AstBinaryOp::Gt => Some("gt"),
            AstBinaryOp::Ge => Some("ge"),
            _ => None,
        }
    }

    /// Lower `a OP b` for `<`/`<=`/`>`/`>=` through `baml.ops.Compare`, resolving
    /// the impl at runtime from the receiver's concrete type. Mirrors
    /// [`Self::lower_arithmetic_via_driver`], but dispatches directly instead of
    /// through a `baml.ops` driver function.
    ///
    /// A driver earns its keep when the compiler cannot *name* the interface to
    /// dispatch on: `equals_equals` because `==` spans operand pairs that share
    /// no interface at all, and `__union_add` and friends because `Add<Rhs>` is
    /// generic in an `Rhs` that may be statically erased. `Compare` is neither —
    /// it is single dispatch (`other: Self`), non-generic, and its methods return
    /// plain `bool` rather than an associated type — so the interface, the
    /// method, and the result type are all statically known and the receiver
    /// alone picks the impl. (Single dispatch is necessary but not sufficient:
    /// `Negate` is single dispatch too, yet returns `Self.Output`, which is why
    /// it still goes through `__union_neg`.)
    ///
    /// Each operator dispatches its *own* method rather than deriving the other
    /// three from `lt`. `implement Compare for float` overrides all four
    /// natively so that NaN is unordered in every direction, which `ge = !lt`
    /// would break; and rewriting `a > b` as `b.lt(a)` would ignore a user's
    /// `gt` override. The interface's defaults still supply whichever methods an
    /// impl leaves out — they are merged into the impl's method table when the
    /// program is baked.
    fn lower_ordering_via_virtual_call(
        &mut self,
        method: &str,
        lhs: AstExprId,
        rhs: AstExprId,
        dest: Place,
    ) {
        let lhs_op = self.lower_to_operand(lhs);
        let rhs_op = self.lower_to_operand(rhs);
        // `Compare` is non-generic and declares no associated types, so the
        // template carries neither; the receiver supplies `Self` at runtime.
        // With no args or associated types to map onto frame slots, the
        // enclosing generic params would never be consulted — pass none.
        let iface = tir2_interface_to_template(
            &Self::baml_ops_qtn("Compare"),
            &[],
            &[],
            self.resolved_aliases,
            &[],
        );
        // Ordering always produces `bool`: TIR's ordering arm types it that way,
        // and the literal pairs `try_fold_binary` would fold instead are all
        // opcode-orderable, so they never reach here. Name it directly rather
        // than reading it back out of `expr_ty`.
        let bool_ty = RuntimeTy::Bool {
            attr: TyAttr::default(),
        };
        let unwind = self.catch_context.as_ref().map(|c| c.unwind_target);
        self.emit_virtual_call_with_operands(
            iface,
            method,
            vec![lhs_op, rhs_op],
            /* ntypeargs */ 0,
            /* runtime_type_check */ false,
            /* runtime_id */ None,
            bool_ty,
            unwind,
            dest,
        );
    }

    /// Whether the negation operand is [`Self::arith_primitive`] (the `Neg`
    /// opcode has no string form).
    fn negate_uses_primitive_opcode(&self, operand: AstExprId) -> bool {
        Self::arith_primitive(&self.expr_ty(operand), false)
    }

    /// Lower `-a` through the `baml.ops.__union_neg` driver (single dispatch on
    /// `a`'s runtime type). Mirrors [`Self::lower_arithmetic_via_driver`].
    fn lower_negate_via_driver(&mut self, expr_id: AstExprId, operand: AstExprId, dest: Place) {
        let operand_op = self.lower_to_operand(operand);
        let result_ty = self.expr_ty(expr_id);
        self.lower_via_ops_driver("__union_neg", vec![operand_op], result_ty, dest);
    }

    fn lower_short_circuit(
        &mut self,
        _expr_id: AstExprId,
        lhs: AstExprId,
        rhs: AstExprId,
        dest: Place,
        is_and: bool,
    ) {
        // `ShortCircuit`'s destination must be a `Place::Local`: the emitter
        // materializes it with `emit_store_place` on the short-circuit edge,
        // which does not handle Field/Index projections. Normalize through a
        // temp and assign through at the join — mirrors `lower_await`.
        let (sc_dest, projection_dest) = match dest {
            Place::Local(_) => (dest, None),
            projection => {
                let tmp = self.builder.temp(RuntimeTy::Null {
                    attr: TyAttr::default(),
                });
                (Place::Local(tmp), Some(projection))
            }
        };

        let lhs_op = self.lower_to_operand(lhs);

        let bb_rhs = self.builder.create_block();
        let bb_join = self.builder.create_block();

        // ShortCircuit terminator: JumpIfFalse (peek) keeps lhs on TOS when
        // short-circuiting. The rhs block evaluates and leaves its result on
        // TOS. At join, dest is on TOS when the destination local is
        // stack-carried (PhiLike); otherwise the emitter stores to its slot
        // on both edges and the join reads the slot.
        self.builder
            .short_circuit(lhs_op, is_and, sc_dest.clone(), bb_rhs, bb_join);

        self.builder.set_current_block(bb_rhs);
        self.lower_expr(rhs, sc_dest.clone());
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        self.builder.set_current_block(bb_join);
        if let Some(projection) = projection_dest {
            self.builder
                .assign(projection, Rvalue::Use(Operand::Copy(sc_dest)));
        }
    }

    /// Lower `a ?? b` — evaluate `a`, if null then evaluate `b`, otherwise use `a`.
    fn lower_null_coalesce(
        &mut self,
        expr_id: AstExprId,
        lhs: AstExprId,
        rhs: AstExprId,
        dest: Place,
    ) {
        let result = self.builder.temp(self.expr_ty(expr_id));
        let result_place = Place::local(result);

        let lhs_op = self.lower_to_operand(lhs);
        self.builder
            .assign(result_place.clone(), Rvalue::Use(lhs_op));

        // Test: lhs == null
        let is_null = Rvalue::BinaryOp {
            op: BinOp::Eq,
            left: Operand::Copy(result_place.clone()),
            right: Operand::Constant(Constant::Null),
        };
        let test_local = self.builder.temp(RuntimeTy::Bool {
            attr: TyAttr::default(),
        });
        self.builder.assign(Place::local(test_local), is_null);

        let bb_rhs = self.builder.create_block();
        let bb_join = self.builder.create_block();

        // If null → evaluate RHS, otherwise keep LHS
        self.builder
            .branch(Operand::Copy(Place::Local(test_local)), bb_rhs, bb_join);

        self.builder.set_current_block(bb_rhs);
        self.lower_expr(rhs, result_place.clone());
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        self.builder.set_current_block(bb_join);
        self.builder
            .assign(dest, Rvalue::Use(Operand::Copy(result_place)));
    }

    /// Lower `OptionalChain { expr }` — set up shared null exit for the entire chain.
    fn lower_optional_chain(&mut self, _expr_id: AstExprId, inner: AstExprId, dest: Place) {
        let bb_null = self.builder.create_block();
        let bb_join = self.builder.create_block();

        // Push shared null exit
        self.chain_null_exits.push(bb_null);

        // Lower inner expression — Optional* nodes will jump to bb_null on null
        self.lower_expr(inner, dest.clone());

        self.chain_null_exits.pop();

        // Non-null path: goto join
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        // Null path: assign null, goto join
        self.builder.set_current_block(bb_null);
        self.builder
            .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
        self.builder.goto(bb_join);

        self.builder.set_current_block(bb_join);
    }

    /// Lower an assignment whose target is wrapped in `OptionalChain`.
    /// Sets up null guards, then emits the assignment only on the non-null path.
    fn lower_assign_optional_chain(&mut self, inner_target: AstExprId, value: AstExprId) {
        let bb_null = self.builder.create_block();
        let bb_join = self.builder.create_block();

        // Push shared null exit — Optional* nodes inside will jump here on null
        self.chain_null_exits.push(bb_null);

        // Lower target as lvalue (this will trigger null checks at each ?. node)
        let place = self.lower_lvalue(inner_target);

        // Lower value and assign.
        self.lower_expr(value, place);

        self.chain_null_exits.pop();

        // Non-null path: goto join
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        // Null path: skip assignment, goto join
        self.builder.set_current_block(bb_null);
        self.builder.goto(bb_join);

        self.builder.set_current_block(bb_join);
    }

    /// Lower a compound assignment (+=, etc.) whose target is wrapped in `OptionalChain`.
    fn lower_assign_op_optional_chain(
        &mut self,
        inner_target: AstExprId,
        op: AstAssignOp,
        value: AstExprId,
    ) {
        let bb_null = self.builder.create_block();
        let bb_join = self.builder.create_block();

        self.chain_null_exits.push(bb_null);

        let place = self.lower_lvalue(inner_target);
        self.emit_assign_op(place, inner_target, op, value);

        self.chain_null_exits.pop();

        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        self.builder.set_current_block(bb_null);
        self.builder.goto(bb_join);

        self.builder.set_current_block(bb_join);
    }

    /// Emit `place = place OP value` — the shared body of both `AssignOp`
    /// lowerings (plain and `?.`-chain targets). An arithmetic op whose
    /// operands aren't primitive routes through the `__union_*` driver, exactly
    /// like the binary-expression form (`v += w` and `v = v + w` must agree);
    /// everything else emits the raw opcode. Mixed `bigint OP= int` does NOT
    /// widen the int rhs on the opcode path: the specialized `*Bigint` opcodes
    /// accept a lone `int` operand and resolve it in the VM without allocating
    /// a heap bigint.
    fn emit_assign_op(
        &mut self,
        place: Place,
        target: AstExprId,
        op: AstAssignOp,
        value: AstExprId,
    ) {
        let mir_op = Self::convert_assign_op(op);
        let driver = match mir_op {
            BinOp::Add => Some("__union_add"),
            BinOp::Sub => Some("__union_sub"),
            BinOp::Mul => Some("__union_mul"),
            BinOp::Div => Some("__union_div"),
            BinOp::Mod => Some("__union_rem"),
            _ => None,
        };
        if let Some(driver) = driver
            && !self.arithmetic_uses_primitive_opcode(target, value)
        {
            let current = Operand::Copy(place.clone());
            let rhs = self.lower_to_operand(value);
            let result_ty = self.expr_ty(target);
            self.lower_via_ops_driver(driver, vec![current, rhs], result_ty, place);
            return;
        }
        let current = Operand::Copy(place.clone());
        let rhs = self.lower_to_operand(value);
        self.builder.assign(
            place,
            Rvalue::BinaryOp {
                op: mir_op,
                left: current,
                right: rhs,
            },
        );
    }

    /// Lower `obj?.member` — null-check obj, then access member or produce null.
    fn lower_optional_member_access(
        &mut self,
        expr_id: AstExprId,
        base: AstExprId,
        field: &Name,
        dest: Place,
    ) {
        let base_op = self.lower_to_operand(base);

        // Test: base == null
        let is_null = Rvalue::BinaryOp {
            op: BinOp::Eq,
            left: base_op,
            right: Operand::Constant(Constant::Null),
        };
        let test_local = self.builder.temp(RuntimeTy::Bool {
            attr: TyAttr::default(),
        });
        self.builder.assign(Place::local(test_local), is_null);

        let bb_access = self.builder.create_block();

        if let Some(&bb_null) = self.chain_null_exits.last() {
            // Inside an OptionalChain — jump to shared null exit
            self.builder
                .branch(Operand::Copy(Place::Local(test_local)), bb_null, bb_access);

            self.builder.set_current_block(bb_access);
            self.lower_member_access(expr_id, base, field, dest);
            // Don't create our own join — the OptionalChain handler does that
        } else {
            // Standalone (no wrapping OptionalChain) — fall back to own null/join blocks
            let bb_null = self.builder.create_block();
            let bb_join = self.builder.create_block();

            self.builder
                .branch(Operand::Copy(Place::Local(test_local)), bb_null, bb_access);

            self.builder.set_current_block(bb_access);
            self.lower_member_access(expr_id, base, field, dest.clone());
            if !self.builder.is_current_terminated() {
                self.builder.goto(bb_join);
            }

            self.builder.set_current_block(bb_null);
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
            self.builder.goto(bb_join);

            self.builder.set_current_block(bb_join);
        }
    }

    /// Lower `obj?.[index]` — null-check obj, then index or produce null.
    fn lower_optional_index(&mut self, base: AstExprId, index: AstExprId, dest: Place) {
        let base_op = self.lower_to_operand(base);

        let is_null = Rvalue::BinaryOp {
            op: BinOp::Eq,
            left: base_op,
            right: Operand::Constant(Constant::Null),
        };
        let test_local = self.builder.temp(RuntimeTy::Bool {
            attr: TyAttr::default(),
        });
        self.builder.assign(Place::local(test_local), is_null);

        let bb_access = self.builder.create_block();

        if let Some(&bb_null) = self.chain_null_exits.last() {
            self.builder
                .branch(Operand::Copy(Place::Local(test_local)), bb_null, bb_access);

            self.builder.set_current_block(bb_access);
            self.lower_optional_index_access(base, index, dest, bb_null);
        } else {
            let bb_null = self.builder.create_block();
            let bb_join = self.builder.create_block();

            self.builder
                .branch(Operand::Copy(Place::Local(test_local)), bb_null, bb_access);

            self.builder.set_current_block(bb_access);
            self.lower_optional_index_access(base, index, dest.clone(), bb_null);
            if !self.builder.is_current_terminated() {
                self.builder.goto(bb_join);
            }

            self.builder.set_current_block(bb_null);
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
            self.builder.goto(bb_join);

            self.builder.set_current_block(bb_join);
        }
    }

    /// Lower the access half of `base?.[index]`, with the base already known
    /// non-null. `?.[]` is the null-safe index operator, so a null *subscript*
    /// must short-circuit the whole expression to null (via `bb_null`) rather
    /// than abort the VM — mirroring the base guard. Only a nullable-typed index
    /// needs the extra check; a non-null index lowers straight to the access.
    fn lower_optional_index_access(
        &mut self,
        base: AstExprId,
        index: AstExprId,
        dest: Place,
        bb_null: BlockId,
    ) {
        let base_ty = self.expr_ty(base);
        let base_op = self.lower_to_operand(base);
        let index_ty = self.expr_ty(index);
        // Lower the index once and reuse it for both the null check and the
        // access, so a side-effectful subscript isn't evaluated twice.
        let index_op = self.lower_to_operand(index);
        if index_ty != index_ty.strip_null() {
            let is_null = Rvalue::BinaryOp {
                op: BinOp::Eq,
                left: index_op.clone(),
                right: Operand::Constant(Constant::Null),
            };
            let test_local = self.builder.temp(RuntimeTy::Bool {
                attr: TyAttr::default(),
            });
            self.builder.assign(Place::local(test_local), is_null);
            let bb_real = self.builder.create_block();
            self.builder
                .branch(Operand::Copy(Place::Local(test_local)), bb_null, bb_real);
            self.builder.set_current_block(bb_real);
        }
        self.emit_index_access(base_op, &base_ty, index_op, index_ty, dest);
    }

    /// Lower `func?.(args)` — null-check callee, then call or produce null.
    fn lower_optional_call(
        &mut self,
        expr_id: AstExprId,
        callee: AstExprId,
        args: &[AstExprId],
        runtime_id: Option<AstExprId>,
        dest: Place,
    ) {
        let callee_op = self.lower_to_operand(callee);

        let is_null = Rvalue::BinaryOp {
            op: BinOp::Eq,
            left: callee_op,
            right: Operand::Constant(Constant::Null),
        };
        let test_local = self.builder.temp(RuntimeTy::Bool {
            attr: TyAttr::default(),
        });
        self.builder.assign(Place::local(test_local), is_null);

        let bb_call = self.builder.create_block();

        if let Some(&bb_null) = self.chain_null_exits.last() {
            self.builder
                .branch(Operand::Copy(Place::Local(test_local)), bb_null, bb_call);

            self.builder.set_current_block(bb_call);
            self.lower_call(expr_id, callee, args, runtime_id, dest);
        } else {
            let bb_null = self.builder.create_block();
            let bb_join = self.builder.create_block();

            self.builder
                .branch(Operand::Copy(Place::Local(test_local)), bb_null, bb_call);

            self.builder.set_current_block(bb_call);
            self.lower_call(expr_id, callee, args, runtime_id, dest.clone());
            if !self.builder.is_current_terminated() {
                self.builder.goto(bb_join);
            }

            self.builder.set_current_block(bb_null);
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
            self.builder.goto(bb_join);

            self.builder.set_current_block(bb_join);
        }
    }

    fn lower_unary(&mut self, expr_id: AstExprId, op: AstUnaryOp, expr: AstExprId, dest: Place) {
        // Check if TIR already folded this expression to a literal constant
        if self.opt >= crate::OptLevel::Two {
            if let RuntimeTy::Literal(ref lit, _, _) = self.expr_ty(expr_id) {
                let constant = Self::lower_literal(lit);
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(constant)));
                return;
            }
        }
        // Negation of a non-primitive operand (a user type, or a union /
        // existential / type variable involving one) dispatches through
        // `baml.ops.Negate`, resolved at runtime from the operand's concrete type.
        if matches!(op, AstUnaryOp::Neg) && !self.negate_uses_primitive_opcode(expr) {
            self.lower_negate_via_driver(expr_id, expr, dest);
            return;
        }
        let operand = self.lower_to_operand(expr);
        let mir_op = match op {
            AstUnaryOp::Not => crate::UnaryOp::Not,
            AstUnaryOp::Neg => crate::UnaryOp::Neg,
        };
        self.builder.assign(
            dest,
            Rvalue::UnaryOp {
                op: mir_op,
                operand,
            },
        );
    }
}

// ─── 3.5: Call lowering with builtin detection ────────────────────────────────

impl<'db> LoweringContext<'db> {
    fn lower_call_arg_operands(&mut self, expr_id: AstExprId, args: &[AstExprId]) -> Vec<Operand> {
        let Some(plan) = self.tir_call_plan(self.expr_metadata_key(expr_id)).cloned() else {
            // No call plan: lower each arg in order (the type checker would
            // have already flagged any mismatch).
            return args.iter().map(|&a| self.lower_to_operand(a)).collect();
        };

        // If this call targets a sys_op (`$rust_io_function`), an omitted
        // defaulted param must be materialized to its declared default HERE:
        // sys_ops have no bytecode body, so they never run the default-parameter
        // prologue that a regular callee would. Leaving `OmittedArg` for a
        // sys_op would reach the engine and panic in `vm_arg_to_bex_value`.
        let callee_expr = match &self.body.exprs[expr_id] {
            AstExpr::Call { callee, .. } => Some(*callee),
            _ => None,
        };
        let sysop_callee = callee_expr.and_then(|callee| self.sys_op_callee(callee));
        // A method-convention sys-op call (e.g. `ctx.output_format_with(...)`) has
        // a receiver-relative `param_index` — TIR strips `self` via
        // `skip_self_param` when building the call plan — but the callee's default
        // arena (`function_parameter_defaults`) is indexed self-inclusive. Shift
        // omitted-default indices by one to skip `self`; free-function sys-ops have
        // no `self`, so no shift.
        let sysop_self_offset = match (sysop_callee, callee_expr) {
            (Some(_), Some(callee)) if self.callee_uses_method_convention(callee) => 1,
            _ => 0,
        };

        // Pre-lower each provided arg in source order (the order `args` appear
        // in the call expression). This preserves the original evaluation
        // order, which matters for side effects.
        let provided_args: Vec<_> = plan.provided_args().collect();
        let mut lowered_args: FxHashMap<AstExprId, Operand> = FxHashMap::default();
        for &arg in args {
            if provided_args.contains(&arg) {
                lowered_args.insert(arg, self.lower_to_operand(arg));
            }
        }

        plan.bindings
            .into_iter()
            .map(|binding| match binding {
                crate::inference_provider::ParamBinding::Provided { arg, .. } => lowered_args
                    .remove(&arg)
                    .expect("call plan referenced an argument outside the call expression"),
                crate::inference_provider::ParamBinding::OmittedDefault { param_index, .. } => {
                    match sysop_callee {
                        Some(callee_loc) => {
                            self.sysop_default_operand(callee_loc, param_index + sysop_self_offset)
                        }
                        None => Operand::Constant(Constant::OmittedArg),
                    }
                }
            })
            .collect()
    }

    /// Materialize a sys-op parameter's omitted default as a constant operand.
    /// `$rust_io_function` callees have no bytecode body — and thus no
    /// default-parameter prologue — so their omitted defaults must be folded at
    /// the call site. The default is read from the CALLEE's own defaults arena
    /// (correct cross-file/cross-package, where the caller's TIR tables don't
    /// cover the callee). Sys-op defaults are constant literals today; a
    /// non-constant default falls back to `OmittedArg` rather than mis-evaluate.
    fn sysop_default_operand(&self, callee_loc: FunctionLoc<'db>, param_index: usize) -> Operand {
        let defaults = baml_compiler2_ppir::function_parameter_defaults(self.db, callee_loc);
        let constant = defaults
            .param_default(param_index)
            .map(|d| d.expr.expr())
            .map(|id| match &defaults.defaults.exprs.exprs[id] {
                AstExpr::Null => Constant::Null,
                AstExpr::Literal(lit) => Self::lower_literal(lit),
                _ => Constant::OmittedArg,
            })
            .unwrap_or(Constant::OmittedArg);
        Operand::Constant(constant)
    }

    /// Operator-style `recv.to_string()` -> `string.from(recv)` desugar, the
    /// inverse direction of `==` -> `baml.ops.equals_equals` (`lower_equality_via_driver`).
    /// Fires only for a 0-arg `to_string` call with NO resolved method: the only
    /// source of a real `to_string` is `implements baml.ToString` (a bare one is
    /// banned), which resolves to a method and is handled by the dispatch/resolution
    /// paths in `lower_call`. `string.from` is total (`throws never`) and honors any
    /// `baml.ToString` override via its runtime shim, so it matches a real call.
    /// Returns `true` (and emits the call) when it handled the expression.
    fn try_lower_to_string_fallback(
        &mut self,
        expr_id: AstExprId,
        callee: AstExprId,
        args: &[AstExprId],
        dest: &Place,
    ) -> bool {
        if !args.is_empty() {
            return false;
        }
        let callee_expr = self.body.exprs[callee].clone();
        // Trigger shape (shared with TIR type inference + throws analysis): a
        // `to_string` member/path call.
        if !is_sugar_callee(&callee_expr, "to_string") {
            return false;
        }
        // Fires only when TIR left the callee *untyped* (`Unknown`/`Error`) — no
        // real `to_string` method resolved. A real implementor (any `baml.ToString`
        // / interface impl) types the callee as a method and is dispatched by the
        // normal paths. Key on the callee's TIR type, not on resolution presence: a
        // generic typevar receiver records a placeholder resolution yet still has an
        // untyped callee, and must take the fallback rather than ICE on it.
        // A nullable receiver types the missing member as `Unknown | null`, so test
        // the non-null part (matches the TIR fallback gate).
        let callee_untyped = self
            .tir_expr_type(self.expr_metadata_key(callee))
            .is_none_or(|t| {
                matches!(
                    t.remove_null(),
                    Tir2Ty::Unknown { .. } | Tir2Ty::Error { .. }
                )
            });
        if !callee_untyped {
            return false;
        }
        let (recv_op, recv_tir_ty): (Operand, Option<Tir2Ty>) = match &callee_expr {
            AstExpr::MemberAccess { base, .. } => {
                let base_id = *base;
                let ty = self.tir_expr_type(self.expr_metadata_key(base_id)).cloned();
                (self.lower_to_operand(base_id), ty)
            }
            AstExpr::Path(segments) => {
                let receiver_segments = &segments[..segments.len() - 1];
                // Lower the receiver, mirroring normal path-method receiver
                // handling: a single-segment root may be a local OR a closure
                // capture; a multi-segment receiver is a field chain off either.
                // (Can't reuse `lower_path_receiver_to_local`: it assumes a local
                // root and `expr_ty(callee)` would ICE on the Unknown callee.)
                let recv_op = if receiver_segments.len() == 1 {
                    let Some(place) = self.place_for_path(callee, &receiver_segments[0]) else {
                        return false;
                    };
                    Operand::Copy(place)
                } else {
                    let recv_ty = self
                        .tir_path_segment_type((
                            self.current_metadata_scope,
                            callee,
                            receiver_segments.len() - 1,
                        ))
                        .cloned()
                        .map(|t| self.convert_tir_ty_for_runtime(&t))
                        .unwrap_or_else(|| RuntimeTy::BuiltinUnknown {
                            attr: TyAttr::default(),
                        });
                    let recv_local = self.builder.temp(recv_ty);
                    self.lower_multi_segment_path_as_field_chain(
                        callee,
                        receiver_segments,
                        Place::local(recv_local),
                    );
                    Operand::Copy(Place::local(recv_local))
                };
                let prefix_idx = segments.len() - 2;
                let ty = self
                    .tir_path_segment_type((self.current_metadata_scope, callee, prefix_idx))
                    .cloned();
                (recv_op, ty)
            }
            _ => return false,
        };

        // `string.from` is the static `from<T>` on `class String` (baml root
        // package, no namespace). Pass the receiver's static type as the leading
        // type arg so `T` binds under monomorphization (a generic receiver `t: T`
        // would otherwise leave `T` undetermined and ICE). The shim ignores `T` at
        // runtime, so an out-of-scope typevar or unknown receiver type safely
        // drops to ntypeargs=0 — matching how `string.from(x)` is normally emitted.
        let caller_generic_params = self.enclosing_generic_params();
        let type_arg_ops: Vec<Operand> = match &recv_tir_ty {
            Some(t)
                if !matches!(t, Tir2Ty::Unknown { .. })
                    && !baml_type_runtime::contains_typevar_where(t, &|name| {
                        !caller_generic_params.iter().any(|p| p == name)
                    }) =>
            {
                self.emit_frame_type_arg_ops(std::slice::from_ref(t))
            }
            _ => Vec::new(),
        };
        let ntypeargs = type_arg_ops.len();
        let mut all_args = type_arg_ops;
        all_args.push(recv_op);

        let callee_op = Operand::Constant(Constant::Function(ItemRef::Method {
            package: Name::new("baml"),
            namespace: vec![],
            class: Name::new("String"),
            name: Name::new("from"),
        }));
        // `string.from` is `throws never`; the unwind target is harmless/unused.
        let unwind = self.catch_context.as_ref().map(|c| c.unwind_target);
        let target = self.builder.create_block();
        // The call destination must be a `Place::Local`; route projection/capture
        // dests through a temp + assign-through (mirrors the normal call path).
        if let Place::Local(_) = dest {
            self.builder.call_with_type_args(
                callee_op,
                all_args,
                ntypeargs,
                dest.clone(),
                target,
                unwind,
            );
            self.builder.set_current_block(target);
        } else {
            let call_ty = self.expr_ty(expr_id);
            let tmp = self.builder.temp(call_ty);
            self.builder.call_with_type_args(
                callee_op,
                all_args,
                ntypeargs,
                Place::local(tmp),
                target,
                unwind,
            );
            self.builder.set_current_block(target);
            self.builder
                .assign(dest.clone(), Rvalue::Use(Operand::Copy(Place::local(tmp))));
        }
        true
    }

    /// Operator-style `recv.to_json()` -> `baml.json.from(recv)` desugar, the json
    /// analog of [`try_lower_to_string_fallback`]. Fires only for a 0-arg `to_json`
    /// call with NO resolved method: the only source of a real `to_json` is
    /// `implements baml.ToJson` (a bare one is banned), handled by the dispatch
    /// paths in `lower_call`. `baml.json.from` honors any `baml.ToJson` override via
    /// its runtime shim, so it matches a real call. Unlike `string.from` it throws
    /// `JsonSerializationError`, so the call's unwind target carries the throw.
    /// Returns `true` (and emits the call) when it handled the expression.
    fn try_lower_to_json_fallback(
        &mut self,
        expr_id: AstExprId,
        callee: AstExprId,
        args: &[AstExprId],
        dest: &Place,
    ) -> bool {
        if !args.is_empty() {
            return false;
        }
        let callee_expr = self.body.exprs[callee].clone();
        if !is_sugar_callee(&callee_expr, "to_json") {
            return false;
        }
        // Fires only when TIR left the callee untyped (no real `to_json` method).
        let callee_untyped = self
            .tir_expr_type(self.expr_metadata_key(callee))
            .is_none_or(|t| {
                matches!(
                    t.remove_null(),
                    Tir2Ty::Unknown { .. } | Tir2Ty::Error { .. }
                )
            });
        if !callee_untyped {
            return false;
        }
        let (recv_op, recv_tir_ty): (Operand, Option<Tir2Ty>) = match &callee_expr {
            AstExpr::MemberAccess { base, .. } => {
                let base_id = *base;
                let ty = self.tir_expr_type(self.expr_metadata_key(base_id)).cloned();
                (self.lower_to_operand(base_id), ty)
            }
            AstExpr::Path(segments) => {
                let receiver_segments = &segments[..segments.len() - 1];
                let recv_op = if receiver_segments.len() == 1 {
                    let Some(place) = self.place_for_path(callee, &receiver_segments[0]) else {
                        return false;
                    };
                    Operand::Copy(place)
                } else {
                    let recv_ty = self
                        .tir_path_segment_type((
                            self.current_metadata_scope,
                            callee,
                            receiver_segments.len() - 1,
                        ))
                        .cloned()
                        .map(|t| self.convert_tir_ty_for_runtime(&t))
                        .unwrap_or_else(|| RuntimeTy::BuiltinUnknown {
                            attr: TyAttr::default(),
                        });
                    let recv_local = self.builder.temp(recv_ty);
                    self.lower_multi_segment_path_as_field_chain(
                        callee,
                        receiver_segments,
                        Place::local(recv_local),
                    );
                    Operand::Copy(Place::local(recv_local))
                };
                let prefix_idx = segments.len() - 2;
                let ty = self
                    .tir_path_segment_type((self.current_metadata_scope, callee, prefix_idx))
                    .cloned();
                (recv_op, ty)
            }
            _ => return false,
        };

        // `baml.json.from` is the namespace function `from<T>(value: T) -> json`.
        // Pass the receiver's static type as the leading type arg so `T` binds
        // under monomorphization (the shim ignores `T` at runtime, so an
        // out-of-scope typevar / unknown receiver safely drops to ntypeargs=0).
        let caller_generic_params = self.enclosing_generic_params();
        let type_arg_ops: Vec<Operand> = match &recv_tir_ty {
            Some(t)
                if !matches!(t, Tir2Ty::Unknown { .. })
                    && !baml_type_runtime::contains_typevar_where(t, &|name| {
                        !caller_generic_params.iter().any(|p| p == name)
                    }) =>
            {
                self.emit_frame_type_arg_ops(std::slice::from_ref(t))
            }
            _ => Vec::new(),
        };
        let ntypeargs = type_arg_ops.len();
        let mut all_args = type_arg_ops;
        all_args.push(recv_op);

        let callee_op = Operand::Constant(Constant::Function(ItemRef::Free {
            package: Name::new("baml"),
            namespace: vec![Name::new("json")],
            name: Name::new("from"),
        }));
        let unwind = self.catch_context.as_ref().map(|c| c.unwind_target);
        let target = self.builder.create_block();
        if let Place::Local(_) = dest {
            self.builder.call_with_type_args(
                callee_op,
                all_args,
                ntypeargs,
                dest.clone(),
                target,
                unwind,
            );
            self.builder.set_current_block(target);
        } else {
            let call_ty = self.expr_ty(expr_id);
            let tmp = self.builder.temp(call_ty);
            self.builder.call_with_type_args(
                callee_op,
                all_args,
                ntypeargs,
                Place::local(tmp),
                target,
                unwind,
            );
            self.builder.set_current_block(target);
            self.builder
                .assign(dest.clone(), Rvalue::Use(Operand::Copy(Place::local(tmp))));
        }
        true
    }

    /// Static-constructor sugar: `Type.from_json(j)` -> `baml.json.to<Type>(j)`.
    /// The deserialize analog of `try_lower_to_json_fallback`. The call's RESULT
    /// type is the receiver type `Type`, so it threads in as the leading type arg
    /// (concretely — `Box<int>` decodes to `Box<int>`). Fires only when TIR left
    /// the callee untyped (no real `from_json` method / `baml.FromJson` override).
    fn try_lower_from_json_static_fallback(
        &mut self,
        expr_id: AstExprId,
        callee: AstExprId,
        args: &[AstExprId],
        dest: &Place,
    ) -> bool {
        if args.len() != 1 {
            return false;
        }
        let callee_expr = self.body.exprs[callee].clone();
        if !is_sugar_callee(&callee_expr, "from_json") {
            return false;
        }
        // Fire only for a type-name receiver (`Type.from_json`), never a value
        // call (`x.from_json`) — rewriting the latter would silently drop `x`.
        // Mirrors the guard in the TIR sugar that types this call.
        let static_receiver = match &callee_expr {
            AstExpr::MemberAccess { base, .. } => match &self.body.exprs[*base] {
                AstExpr::Path(segs) if !segs.is_empty() => {
                    self.binding_id_for_path(*base, &segs[0]).is_none()
                }
                _ => false,
            },
            AstExpr::Path(segs) if segs.len() >= 2 => {
                self.binding_id_for_path(callee, &segs[0]).is_none()
            }
            _ => false,
        };
        if !static_receiver {
            return false;
        }
        let callee_untyped = self
            .tir_expr_type(self.expr_metadata_key(callee))
            .is_none_or(|t| {
                matches!(
                    t.remove_null(),
                    Tir2Ty::Unknown { .. } | Tir2Ty::Error { .. }
                )
            });
        if !callee_untyped {
            return false;
        }

        // The receiver type is the call's result type. Pass it as the leading
        // type arg so `baml.json.to<T>` binds `T` under monomorphization; an
        // out-of-scope typevar / unknown safely drops to ntypeargs=0 (the shim
        // resolves on the runtime value when no static type is supplied).
        let recv_tir_ty: Option<Tir2Ty> =
            self.tir_expr_type(self.expr_metadata_key(expr_id)).cloned();
        let caller_generic_params = self.enclosing_generic_params();
        let type_arg_ops: Vec<Operand> = match &recv_tir_ty {
            Some(t)
                if !matches!(t, Tir2Ty::Unknown { .. })
                    && !baml_type_runtime::contains_typevar_where(t, &|name| {
                        !caller_generic_params.iter().any(|p| p == name)
                    }) =>
            {
                self.emit_frame_type_arg_ops(std::slice::from_ref(t))
            }
            _ => Vec::new(),
        };
        let ntypeargs = type_arg_ops.len();
        let arg_op = self.lower_to_operand(args[0]);
        let mut all_args = type_arg_ops;
        all_args.push(arg_op);

        let callee_op = Operand::Constant(Constant::Function(ItemRef::Free {
            package: Name::new("baml"),
            namespace: vec![Name::new("json")],
            name: Name::new("to"),
        }));
        let unwind = self.catch_context.as_ref().map(|c| c.unwind_target);
        let target = self.builder.create_block();
        if let Place::Local(_) = dest {
            self.builder.call_with_type_args(
                callee_op,
                all_args,
                ntypeargs,
                dest.clone(),
                target,
                unwind,
            );
            self.builder.set_current_block(target);
        } else {
            let call_ty = self.expr_ty(expr_id);
            let tmp = self.builder.temp(call_ty);
            self.builder.call_with_type_args(
                callee_op,
                all_args,
                ntypeargs,
                Place::local(tmp),
                target,
                unwind,
            );
            self.builder.set_current_block(target);
            self.builder
                .assign(dest.clone(), Rvalue::Use(Operand::Copy(Place::local(tmp))));
        }
        true
    }

    fn lower_call(
        &mut self,
        expr_id: AstExprId,
        callee: AstExprId,
        args: &[AstExprId],
        runtime_id: Option<AstExprId>,
        dest: Place,
    ) {
        use baml_compiler2_hir_ty::callable::ExternalCallTarget;

        use crate::inference_provider::MemberResolution;

        let callee_expr = self.body.exprs[callee].clone();
        if let AstExpr::MemberAccess { base, member } = &callee_expr {
            let member_name = member.clone();
            let base_id = *base;
            // BEP-044: interface-typed receiver — dispatch by type tag over
            // the registered implementor set. Each arm emits a static call
            // to that implementor's method.
            if self.try_lower_interface_dispatch(
                expr_id,
                base_id,
                &member_name,
                args,
                runtime_id,
                &dest,
            ) {
                return;
            }
            // Receiver may be a union of concrete classes sharing the method
            // (e.g. `(if c { Dog {} } else { Cat {} }).speak()`).
            if self.try_lower_union_dispatch(
                expr_id,
                base_id,
                &member_name,
                args,
                runtime_id,
                &dest,
            ) {
                return;
            }
            // Receiver may be a union containing an interface member
            // (e.g. `Animal | Vehicle`), where every member declares the
            // method — dispatch on the runtime class across all implementors.
            if self.try_lower_union_iface_dispatch(
                expr_id,
                base_id,
                &member_name,
                args,
                runtime_id,
                &dest,
            ) {
                return;
            }
        }
        // A mounted interface UFCS call names an interface slot but supplies
        // its receiver as the first explicit argument. Route it through the
        // same open-world virtual dispatcher as `value.method()`.
        if let AstExpr::Path(segments) = &callee_expr
            // A value-rooted path such as `a.merge(b)` has `b` as its first
            // source argument; treating that as UFCS would silently replace
            // `a` with `b` and drop the real argument.  Only a type-/package-
            // rooted path spells UFCS, and therefore supplies `self` explicitly.
            && self.binding_id_for_path(callee, &segments[0]).is_none()
            && let Some(MemberResolution::External(external)) =
                self.tir_resolution(self.expr_metadata_key(callee)).cloned()
            && let ExternalCallTarget::Interface { method, .. } = &external.target
            && external.takes_self
            && let Some(&receiver) = args.first()
            && self.try_lower_interface_ufcs_dispatch(
                expr_id, receiver, method, args, runtime_id, &dest,
            )
        {
            return;
        }
        // BEP-044: `default.<method>(...)` inside an `implements I { ... }`
        // block emits a static call to `I`'s default function, with the
        // class's `self` forwarded as the receiver. No type-tag switch —
        // the override is being deliberately bypassed.
        if let AstExpr::Path(segments) = &callee_expr
            && segments.len() == 2
            && self.is_default_receiver_root(callee, segments)
            && let Some(target) = self.implements_block_iface_target()
            && matches!(
                target.type_refs[target.target].kind,
                baml_compiler2_hir::type_ref::TypeRefKind::Path { .. }
            )
        {
            let current_pkg = baml_compiler2_hir::file_package::file_package(self.db, self.file);
            let pkg_id = PackageId::new(self.db, current_pkg.package.clone());
            let pkg_items = package_items(self.db, pkg_id);
            if let Some(iface_loc) = resolve_ref_to_interface_loc(
                self.db,
                &target.type_refs,
                target.target,
                pkg_items,
                &current_pkg.namespace_path,
            ) {
                let iface_pkg = baml_compiler2_hir::file_package::file_package(
                    self.db,
                    iface_loc.file(self.db),
                );
                let iface_name = baml_compiler2_ppir::item_data::interface_data(self.db, iface_loc)
                    .name
                    .clone();
                let method_name = segments[1].clone();
                let item_ref = ItemRef::Method {
                    package: iface_pkg.package.clone(),
                    namespace: iface_pkg.namespace_path,
                    class: iface_name,
                    name: method_name,
                };
                let callee_op = Operand::Constant(Constant::Function(item_ref));
                let Some(&self_local) = self.locals.get(&Name::new("self")) else {
                    return;
                };
                // Seed the default method's frame exactly as the runtime seeds
                // an inherited default: `[Self ++ interface generics ++
                // associated types]` (see `interface_frame` in
                // `baml_compiler2_emit` and `enclosing_generic_params`),
                // expressed over the enclosing impl's generic params. `Self`
                // is the enclosing implements-block's subject, statically
                // known at this call. An associated type the block leaves
                // unpinned is completed from its declared default — realized
                // at this impl (`Self` := the subject, the interface's params
                // := the target's arguments), mirroring emit's rule
                // completion. The default body lowers `Self`, the interface's
                // params, and its assoc names to `TypeArgRef` slots, so a
                // short or shifted frame would resolve them wrongly at
                // runtime.
                let frame_tys: Vec<Tir2Ty> = {
                    let generic_params = self.enclosing_generic_params();
                    let generic_param_bounds = self.enclosing_generic_param_bounds();
                    let self_subject = self.implements_subject_tir_ty().unwrap_or_else(|| {
                        // `implements_block_iface_target` gated entry to this
                        // branch, and a recorded target pairs with a Class /
                        // FreeImpl owner by construction (both are written by
                        // the same HIR builder call).
                        unreachable!(
                            "`default.<method>()` bypass outside an implements-block method"
                        )
                    });
                    // The target lowered whole (as the constraint head it is —
                    // written pins only): its generic args plus any inline
                    // `<Item = int>` bindings. Block-level `type Item = …;`
                    // bindings are appended after (a name is bound at most
                    // once, so first-match lookup is exact).
                    let (iface_args, mut assoc_bindings) = match lower_ref_in_scope_at(
                        self.db,
                        &target.type_refs,
                        target.target,
                        pkg_items,
                        &current_pkg.namespace_path,
                        &generic_params,
                        &generic_param_bounds,
                        None,
                        baml_compiler2_hir_ty::lower::TypePosition::ConstraintHead,
                    ) {
                        Tir2Ty::Interface(_, args, assoc, _) => (args, assoc),
                        // A non-interface resolution was already diagnosed
                        // upstream; seed the dimensions it cannot supply
                        // as absent.
                        _ => (Vec::new(), Vec::new()),
                    };
                    for binding in &target.associated_type_bindings {
                        let Some(id) = binding.type_ref else { continue };
                        assoc_bindings.push((
                            binding.name.clone(),
                            lower_ref_in_scope(
                                self.db,
                                &target.type_refs,
                                id,
                                pkg_items,
                                &current_pkg.namespace_path,
                                &generic_params,
                                &generic_param_bounds,
                                None,
                            ),
                        ));
                    }
                    let iface_data =
                        baml_compiler2_ppir::item_data::interface_data(self.db, iface_loc);
                    let iface_env =
                        baml_compiler2_hir_ty::lower::interface_frame(self.db, iface_loc);
                    let self_param = iface_env
                        .first()
                        .expect("interface frame starts with Self")
                        .clone();
                    let iface_params =
                        baml_compiler2_hir_ty::lower::interface_declared_params(self.db, iface_loc);
                    let mut default_bindings: FxHashMap<ParamTy, Tir2Ty> = FxHashMap::default();
                    default_bindings.insert(self_param, self_subject.clone());
                    for (param, arg) in iface_params.iter().zip(&iface_args) {
                        default_bindings.insert(param.clone(), arg.clone());
                    }
                    let assoc_tys: Vec<Tir2Ty> = iface_data
                        .associated_types
                        .iter()
                        .map(|assoc| {
                            assoc_bindings
                                .iter()
                                .find(|(n, _)| *n == assoc.name)
                                .map(|(_, t)| t.clone())
                                .or_else(|| {
                                    baml_compiler2_hir_ty::interfaces::
                                        interface_associated_type_default(
                                            self.db,
                                            iface_loc,
                                            assoc.name.clone(),
                                        )
                                        .map(|(default, _decl_site_diags)| {
                                            baml_type_runtime::substitute_ty(
                                                &default,
                                                &default_bindings,
                                            )
                                        })
                                })
                                // Neither pinned nor defaulted: a diagnosed
                                // incomplete impl — keep the top type for
                                // error recovery.
                                .unwrap_or_else(|| Tir2Ty::BuiltinUnknown {
                                    attr: TyAttr::default(),
                                })
                        })
                        .collect();
                    let mut tys = Vec::with_capacity(1 + iface_args.len() + assoc_tys.len());
                    tys.push(self_subject);
                    tys.extend(iface_args);
                    tys.extend(assoc_tys);
                    tys
                };
                let frame_type_arg_ops = self.emit_frame_type_arg_ops(&frame_tys);
                let ntypeargs = frame_type_arg_ops.len();
                let mut all_args = frame_type_arg_ops;
                all_args.push(Operand::Copy(Place::Local(self_local)));
                all_args.extend(self.lower_call_arg_operands(expr_id, args));
                let runtime_id_operand = self.lower_runtime_id_operand(runtime_id);
                let target = self.builder.create_block();
                let unwind = self.catch_context.as_ref().map(|c| c.unwind_target);
                self.builder.call_with_type_args_and_runtime_id(
                    callee_op,
                    all_args,
                    ntypeargs,
                    runtime_id_operand,
                    dest,
                    target,
                    unwind,
                );
                self.builder.set_current_block(target);
                return;
            }
        }
        // BEP-044: intercept Path forms whose final segment is a method
        // call on an interface-typed receiver.
        //
        //   `<local>.<method>()` (2 segments) — receiver inferred interface
        //   `<local>.<field>.<method>()` (3+ segments) — field chain whose
        //   prefix is interface-typed
        if let AstExpr::Path(segments) = &callee_expr {
            // Any path of length ≥ 2 may end in a method call whose
            // receiver is interface-typed. The receiver type is recorded
            // by TIR at the segment just before the method name (or, for
            // a 2-segment path, is the root local's declared type).
            //
            // The segment just before the method name may be a real field
            // access (`r.a.b.c.d.e.speak()`) whose static type is an interface.
            if segments.len() >= 2
                && let Some(recv_root_local) = self.local_for_path(callee, &segments[0])
            {
                let method_name = segments.last().unwrap().clone();
                let prefix_idx = segments.len() - 2;
                let recv_seg_idx = if segments.len() == 2 { 0 } else { prefix_idx };
                let recv_tir_ty = self
                    .tir_path_segment_type((self.current_metadata_scope, callee, recv_seg_idx))
                    .cloned()
                    .or_else(|| {
                        if segments.len() == 2
                            && segments[0].as_str() == "self"
                            && let Some(self_param) = self
                                .enclosing_generic_params()
                                .into_iter()
                                .find(|param| param.as_str() == "Self")
                            && self.generic_param_bounds.contains_key(&self_param)
                        {
                            Some(Tir2Ty::TypeVar(self_param, baml_type::TyAttr::default()))
                        } else {
                            None
                        }
                    })
                    // A lambda-parameter receiver (`(a: T) -> a.compare(b)`) has
                    // no `path_segment_types` entry; recover its declared type so
                    // a method on its (bounded) type variable dispatches.
                    .or_else(|| {
                        if segments.len() == 2 {
                            self.lambda_param_tir_types.get(&segments[0]).cloned()
                        } else {
                            None
                        }
                    })
                    // A bounded-type-var function parameter receiver (`a.lt(b)`
                    // with `a: T extends Compare`) likewise has no recorded
                    // segment type; its declared type keeps `T` as a `TypeVar`,
                    // so the dispatch below routes it to a virtual call.
                    .or_else(|| {
                        if segments.len() == 2 {
                            self.binding_id_for_path(callee, &segments[0])
                                .and_then(|bid| {
                                    self.source_param_tir_ty_for_binding(&segments[0], bid)
                                })
                        } else {
                            None
                        }
                    });
                let iface_dispatch_opt: Option<InterfaceTypeView> = if segments.len() == 2 {
                    self.source_param_interface_view_for_name_at(callee, &segments[0], &method_name)
                        .or_else(|| {
                            recv_tir_ty.as_ref().and_then(|ty| {
                                self.interface_dispatch_target_for_member(ty, &method_name)
                            })
                        })
                } else {
                    recv_tir_ty
                        .as_ref()
                        .and_then(|ty| self.interface_dispatch_target_for_member(ty, &method_name))
                }
                // Concrete receiver whose method comes from an impl (blanket,
                // out-of-body, or in-body) — the providing interface, resolved
                // through the canonical L1 substrate.
                .or_else(|| {
                    recv_tir_ty
                        .as_ref()
                        .and_then(|ty| self.dispatch_target_for_concrete(ty, &method_name))
                });
                if let Some((iface_tn, iface_type_args, iface_assoc)) = iface_dispatch_opt {
                    // Decide how many leading segments form the receiver
                    // value (the rest are type qualifiers).
                    let prefix_is_qualifier = segments.len() >= 3
                        && segments[prefix_idx].as_str() == iface_tn.name().as_str();
                    let receiver_segments_end = if prefix_is_qualifier {
                        prefix_idx
                    } else {
                        segments.len() - 1
                    };
                    let receiver_segments = &segments[..receiver_segments_end];
                    let recv_local = self.lower_path_receiver_to_local(
                        callee,
                        receiver_segments,
                        recv_root_local,
                    );
                    // Every interface-mediated call dispatches open-world via a
                    // virtual call — same routing as the member-access dispatch
                    // site (`try_lower_interface_dispatch`); the VM resolves the
                    // impl from the receiver's runtime concrete type (containers
                    // included). Key the call on the interface that *declares* the
                    // method (which may be a `requires` super-interface of the
                    // receiver's static interface).
                    let (decl_tn, decl_args, decl_assoc) = self.interface_view_declaring_method(
                        &(iface_tn, iface_type_args, iface_assoc),
                        &method_name,
                    );
                    if self.emit_virtual_call(
                        recv_local,
                        &decl_tn,
                        &decl_args,
                        &decl_assoc,
                        &method_name,
                        expr_id,
                        args,
                        runtime_id,
                        &dest,
                    ) {
                        return;
                    }
                }
                // Parallel to the interface case: the receiver may instead be a
                // union of concrete classes (a local or field chain bound to a
                // `match`/`if` whose arms are different classes). Same receiver
                // type slot, same field-chain lowering.
                else if let Some(members) = self
                    .tir_path_segment_type((self.current_metadata_scope, callee, prefix_idx))
                    .and_then(Self::tir_union_members)
                {
                    let receiver_segments = &segments[..segments.len() - 1];
                    let recv_local = self.lower_path_receiver_to_local(
                        callee,
                        receiver_segments,
                        recv_root_local,
                    );
                    if self.emit_union_class_dispatch(
                        recv_local,
                        &members,
                        &method_name,
                        DispatchCallLowering {
                            expr_id,
                            args,
                            runtime_id,
                            dest: &dest,
                        },
                    ) {
                        return;
                    }
                    // Union whose members share the method through an interface:
                    // the runtime value is one concrete member, so dispatch
                    // open-world on the shared interface.
                    if let Some((decl_tn, decl_args, decl_assoc)) =
                        self.union_virtual_dispatch_view(&members, &method_name)
                        && self.emit_virtual_call(
                            recv_local,
                            &decl_tn,
                            &decl_args,
                            &decl_assoc,
                            &method_name,
                            expr_id,
                            args,
                            runtime_id,
                            &dest,
                        )
                    {
                        return;
                    }
                }
            }
        }

        // Operator-style `recv.to_string()` -> `string.from(recv)` fallback. Runs
        // after all real dispatch (interface/union above, method resolution below)
        // has been attempted, so a `baml.ToString` implementor always wins first;
        // only a `to_string` call with no resolved method reaches the fallback.
        if self.try_lower_to_string_fallback(expr_id, callee, args, &dest) {
            return;
        }
        // Same fallback for `recv.to_json()` -> `baml.json.from(recv)`.
        if self.try_lower_to_json_fallback(expr_id, callee, args, &dest) {
            return;
        }
        // Static-constructor `Type.from_json(j)` -> `baml.json.to<Type>(j)`.
        if self.try_lower_from_json_static_fallback(expr_id, callee, args, &dest) {
            return;
        }

        // Check if callee is a method call (MemberAccess or multi-segment Path with a
        // MemberResolution::BoundMethod/UnboundMethod/Free). Field and Variant resolutions are not callable.
        // If the base is a real value (not a package namespace), prepend it as self.
        let mut receiver_base_for_class_type_args: Option<AstExprId> = None;
        let mut receiver_path_tir_ty: Option<Tir2Ty> = None;
        let (callee_operand, arg_operands) = if let AstExpr::MemberAccess { base, .. } =
            &callee_expr
        {
            if self
                .tir_resolution(self.expr_metadata_key(callee))
                .is_some_and(|r| {
                    matches!(
                        r,
                        MemberResolution::BoundMethod { .. }
                            | MemberResolution::UnboundMethod { .. }
                            | MemberResolution::Free { .. }
                            | MemberResolution::InterfaceVirtualMethod { .. }
                            | MemberResolution::InterfaceConcreteMethod { .. }
                    ) || matches!(r, MemberResolution::External(_))
                })
            {
                // Check if base is a value receiver or a bare type/package path.
                // Type-name bases like `Label<int>.method` can have concrete
                // TIR types (`Interface`, `Class`) but are not runtime values.
                let base_is_value = match &self.body.exprs[*base] {
                    AstExpr::Path(segments) if !segments.is_empty() => {
                        self.binding_id_for_path(*base, &segments[0]).is_some()
                    }
                    _ => self
                        .tir_expr_type(self.expr_metadata_key(*base))
                        .map(|ty| !matches!(ty, Tir2Ty::Unknown { .. }))
                        .unwrap_or(false),
                };
                // Check if the resolved method expects a `self` receiver.
                // Static methods (e.g. ParseCache.new) have no `self` param
                // and must not get the class reference prepended as an argument.
                let method_takes_self = {
                    self.tir_resolution(self.expr_metadata_key(callee))
                        .is_some_and(|r| match r {
                            MemberResolution::BoundMethod { func_loc, .. }
                            | MemberResolution::UnboundMethod { func_loc, .. }
                            | MemberResolution::Free { func_loc }
                            | MemberResolution::InterfaceConcreteMethod { func_loc, .. } => {
                                let sig =
                                    baml_compiler2_ppir::function_signature(self.db, *func_loc);
                                sig.params
                                    .first()
                                    .is_some_and(|param| param.name.as_str() == "self")
                            }
                            // A virtual interface-method call is always on a receiver, so it
                            // takes `self`; there is no static body to inspect.
                            MemberResolution::InterfaceVirtualMethod { .. } => true,
                            MemberResolution::External(external) => external.takes_self,
                            _ => false,
                        })
                };
                if base_is_value && method_takes_self {
                    // Instance method call: arr.length() — prepend receiver as self.
                    // For immediate calls, emit the callee as a plain function constant
                    // (not MakeBoundMethod) since the receiver is passed explicitly as self.
                    let receiver_op = self.lower_to_operand(*base);
                    receiver_base_for_class_type_args = Some(*base);
                    let callee_op = {
                        let resolution =
                            self.tir_resolution(self.expr_metadata_key(callee)).cloned();
                        match resolution
                            .as_ref()
                            .and_then(|r| resolution_to_item_ref(self.db, r))
                        {
                            Some(item) => Operand::Constant(Constant::Function(item)),
                            None => self.lower_to_operand(callee),
                        }
                    };
                    let mut all_args = vec![receiver_op];
                    all_args.extend(self.lower_call_arg_operands(expr_id, args));
                    (callee_op, all_args)
                } else {
                    // Non-self method or package function reference:
                    // e.g. Factory<int>.create(42), baml.Array.length(array).
                    // Resolve the callee as a plain function constant using
                    // resolution_to_item_ref to avoid lower_member_access emitting
                    // MakeBoundMethod (which would try to load the base type as a
                    // runtime value).
                    let callee_op = {
                        let resolution =
                            self.tir_resolution(self.expr_metadata_key(callee)).cloned();
                        match resolution
                            .as_ref()
                            .and_then(|r| resolution_to_item_ref(self.db, r))
                        {
                            Some(item) => Operand::Constant(Constant::Function(item)),
                            None => self.lower_to_operand(callee),
                        }
                    };
                    (callee_op, self.lower_call_arg_operands(expr_id, args))
                }
            } else {
                let callee_op = self.lower_to_operand(callee);
                (callee_op, self.lower_call_arg_operands(expr_id, args))
            }
        } else if let AstExpr::Path(segments) = &callee_expr {
            // Check path_member_resolutions first (local-rooted paths like `self.method()`
            // or `obj.field.method()`). The last resolution determines if the final segment
            // is a method call (e.g. for `user.profile.items.slice`, resolutions are
            // [Field{profile}, Field{items}, Method{slice}] — last() is Method).
            let is_local_method = segments.len() >= 2
                && self
                    .tir_path_member_resolutions(self.expr_metadata_key(callee))
                    .and_then(|resolutions| resolutions.last())
                    .is_some_and(|r| {
                        matches!(
                            r,
                            MemberResolution::BoundMethod { .. }
                                | MemberResolution::UnboundMethod { .. }
                                | MemberResolution::InterfaceVirtualMethod { .. }
                                | MemberResolution::InterfaceConcreteMethod { .. }
                        ) || matches!(r, MemberResolution::External(external) if external.takes_self)
                    });
            // Also check flat resolutions (package-path method call, kept for compatibility).
            let is_pkg_method = !is_local_method
                && segments.len() >= 2
                && self
                    .tir_resolution(self.expr_metadata_key(callee))
                    .is_some_and(|r| {
                        matches!(
                            r,
                            MemberResolution::BoundMethod { .. }
                                | MemberResolution::UnboundMethod { .. }
                                | MemberResolution::InterfaceVirtualMethod { .. }
                                | MemberResolution::InterfaceConcreteMethod { .. }
                        ) || matches!(r, MemberResolution::External(external) if external.takes_self)
                    });

            if is_local_method {
                // Multi-segment path callee with a local-rooted Method resolution.
                // The last segment is the method; segments[0..n-1] form the receiver.
                // e.g. `self.method()` → receiver=self, `user.profile.items.slice()` → receiver=user.profile.items.
                //
                // For immediate calls we emit the callee as a plain function constant
                // (not MakeBoundMethod) since the receiver is passed explicitly as self.
                let receiver_segments = &segments[..segments.len() - 1];
                let method_resolution = self
                    .tir_path_member_resolutions(self.expr_metadata_key(callee))
                    .and_then(|resolutions| resolutions.last())
                    .cloned();
                let callee_op = match method_resolution
                    .as_ref()
                    .and_then(|r| resolution_to_item_ref(self.db, r))
                {
                    Some(item) => Operand::Constant(Constant::Function(item)),
                    None => self.lower_to_operand(callee),
                };
                let method_takes_self = method_resolution.as_ref().is_some_and(|r| match r {
                    MemberResolution::BoundMethod { func_loc, .. }
                    | MemberResolution::UnboundMethod { func_loc, .. }
                    | MemberResolution::InterfaceConcreteMethod { func_loc, .. } => {
                        let sig = baml_compiler2_ppir::function_signature(self.db, *func_loc);
                        sig.params
                            .first()
                            .is_some_and(|param| param.name.as_str() == "self")
                    }
                    MemberResolution::InterfaceVirtualMethod { .. } => true,
                    MemberResolution::External(external) => external.takes_self,
                    _ => false,
                });
                if !method_takes_self {
                    (callee_op, self.lower_call_arg_operands(expr_id, args))
                } else {
                    let receiver_op = if receiver_segments.len() == 1 {
                        // Simple local variable receiver (e.g. `self`).
                        self.place_for_path(callee, &receiver_segments[0])
                            .map_or_else(|| Operand::Constant(Constant::Null), Operand::Copy)
                    } else {
                        // Multi-segment receiver (e.g. `user.profile.items`): lower as field chain.
                        let recv_ty = self.expr_ty(callee); // approximation; actual type not critical here
                        let recv_local = self.builder.temp(recv_ty);
                        self.lower_multi_segment_path_as_field_chain(
                            callee,
                            receiver_segments,
                            Place::local(recv_local),
                        );
                        Operand::Copy(Place::local(recv_local))
                    };
                    let prefix_idx = segments.len() - 2;
                    receiver_path_tir_ty = self
                        .tir_path_segment_type((self.current_metadata_scope, callee, prefix_idx))
                        .cloned();
                    let mut all_args = vec![receiver_op];
                    all_args.extend(self.lower_call_arg_operands(expr_id, args));
                    (callee_op, all_args)
                }
            } else if is_pkg_method {
                // Package-path method call (via flat resolutions): same treatment.
                // For immediate calls, emit the callee as a plain function constant
                // (not MakeBoundMethod) since the receiver is passed explicitly as self.
                let flat_resolution = self.tir_resolution(self.expr_metadata_key(callee)).cloned();
                let callee_op = match flat_resolution
                    .as_ref()
                    .and_then(|r| resolution_to_item_ref(self.db, r))
                {
                    Some(item) => Operand::Constant(Constant::Function(item)),
                    None => self.lower_to_operand(callee),
                };
                let receiver_op = self.place_for_path(callee, &segments[0]).map(Operand::Copy);
                if let Some(receiver_op) = receiver_op {
                    let prefix_idx = segments.len() - 2;
                    receiver_path_tir_ty = self
                        .tir_path_segment_type((self.current_metadata_scope, callee, prefix_idx))
                        .cloned();
                    let mut all_args = vec![receiver_op];
                    all_args.extend(self.lower_call_arg_operands(expr_id, args));
                    (callee_op, all_args)
                } else {
                    (callee_op, self.lower_call_arg_operands(expr_id, args))
                }
            } else {
                let callee_op = self.lower_to_operand(callee);
                (callee_op, self.lower_call_arg_operands(expr_id, args))
            }
        } else {
            let callee_op = self.lower_to_operand(callee);
            (callee_op, self.lower_call_arg_operands(expr_id, args))
        };

        let target = self.builder.create_block();
        let unwind = self.catch_context.as_ref().map(|c| c.unwind_target);

        // Check if callee is `reflect.type_of<T>()` — a value-producing intrinsic.
        // Unlike void intrinsics (log.*), this emits an assignment
        // of `Rvalue::LoadType(template)` to `dest` rather than a StatementKind::Intrinsic.
        if let Some(template) = self.check_type_of_intrinsic(callee, expr_id) {
            self.builder.assign(dest, Rvalue::LoadType(template));
            self.builder.goto(target);
            self.builder.set_current_block(target);
            return;
        }

        // `Package.current()` is lexical, like `type.of`: bake the enclosing
        // package identity at this call site instead of emitting a callable
        // reference to the compiler-owned declaration.
        if matches!(
            &callee_operand,
            Operand::Constant(Constant::Function(item))
                if item.to_string() == "baml.reflect.Package.current"
        ) {
            let package = file_package(self.db, self.file).package.to_string();
            self.builder.assign(dest, Rvalue::CurrentPackage(package));
            self.builder.goto(target);
            self.builder.set_current_block(target);
            return;
        }

        // Check if callee is `.length()` on a container — emit Rvalue::Len instead of Call.
        if let Operand::Constant(Constant::Function(ref item)) = callee_operand {
            let name = item.to_string();
            if name == "baml.Array.length"
                || name == "baml.Map.length"
                || name == "baml.string.length"
                || name == "baml.Uint8Array.length"
            {
                if let Some(receiver_operand) = arg_operands.first() {
                    let place = match receiver_operand {
                        Operand::Copy(p) | Operand::Move(p) => p.clone(),
                        Operand::Constant(_) => {
                            let tmp = self.builder.temp(baml_type::RuntimeTy::unknown());
                            self.builder
                                .assign(Place::Local(tmp), Rvalue::Use(receiver_operand.clone()));
                            Place::Local(tmp)
                        }
                    };
                    self.builder.assign(dest, Rvalue::Len(place));
                    self.builder.goto(target);
                    self.builder.set_current_block(target);
                    return;
                }
            }
        }

        // Check if callee is a compiler intrinsic (log.*).
        // Intrinsics are void side effects — emit as a statement, not a call.
        if let Some(op) = self.check_intrinsic(callee) {
            self.builder.push_statement(
                StatementKind::Intrinsic {
                    op,
                    args: arg_operands,
                },
                None,
            );
            self.builder.goto(target);
            self.builder.set_current_block(target);
            return;
        }

        // ── Emit LoadType temps for call type arguments ──────────────────────
        // When the call carries type args, either explicit (`describe<User>()`)
        // or inferred by TIR, materialise each as a `type` value on the stack
        // before the regular value args.
        // The VM pops these `ntypeargs` Object::Type values into the new frame's
        // `type_args` vec so that inner `reflect.type_of<T>()` calls can
        // substitute them at runtime.
        // Check if callee resolves to a builtin IO function (sys-op). Sys-op
        // glue reads only its declared value args plus any synthetic trailing
        // `type` operands needed for generic params that are not already
        // represented by ordinary `type` value params.
        let sys_op_type_arg_count = self.sys_op_synthetic_type_arg_count(callee);
        let is_sys_op = sys_op_type_arg_count.is_some();
        let call_type_arg_operands =
            self.lower_call_type_args(expr_id, true, sys_op_type_arg_count);

        // ── Prepend receiver's class-level type args ─────────────────────────
        // For `b.describe()` where `b: Box<int>`, the method `describe` is compiled
        // as a direct call `describe(b)` (not via MakeBoundMethod). The VM's
        // BoundMethod path for seeding frame.type_args is bypassed, so we instead
        // emit LoadType for each class-level type arg and prepend them before
        // the method's own type args. This preserves De Bruijn ordering:
        //   frame.type_args = [class_T, class_U, ..., fn_A, fn_B, ...]
        // matching `enclosing_generic_params()` = class_params ++ fn_params.
        //
        // There are two receiver paths:
        //   1. MemberAccess callee (`base.method()`): receiver type from `expr_types[recv_base_id]`.
        //   2. Path callee (`b.describe()` compiled as Path(["b","describe"])): receiver type
        //      from `path_root_types[callee_expr_id]` (TIR records root segment type there).
        let receiver_tir_ty: Option<Tir2Ty> =
            if let Some(recv_base_id) = receiver_base_for_class_type_args {
                self.tir_expr_type(self.expr_metadata_key(recv_base_id))
                    .cloned()
            } else {
                receiver_path_tir_ty
            };
        let receiver_class_type_args: Vec<Tir2Ty> =
            match (&callee_operand, receiver_tir_ty.as_ref()) {
                (_, Some(Tir2Ty::Class(_, class_type_args, _))) => class_type_args.clone(),
                (
                    Operand::Constant(Constant::Function(ItemRef::Method {
                        package,
                        namespace,
                        class,
                        ..
                    })),
                    Some(Tir2Ty::List(inner, _) | Tir2Ty::EvolvingList(inner, _)),
                ) if package.as_str() == "baml"
                    && namespace.is_empty()
                    && class.as_str() == "Array" =>
                {
                    vec![inner.as_ref().clone()]
                }
                (
                    Operand::Constant(Constant::Function(ItemRef::Method {
                        package,
                        namespace,
                        class,
                        ..
                    })),
                    Some(Tir2Ty::Map { key, value, .. } | Tir2Ty::EvolvingMap(key, value, _)),
                ) if package.as_str() == "baml"
                    && namespace.is_empty()
                    && class.as_str() == "Map" =>
                {
                    vec![key.as_ref().clone(), value.as_ref().clone()]
                }
                _ => Vec::new(),
            };
        let receiver_class_type_arg_operands: Vec<Operand> = if !receiver_class_type_args.is_empty()
        {
            let generic_params = self.enclosing_generic_params();
            receiver_class_type_args
                .iter()
                .map(|ty_arg| {
                    let template = self.ty_to_template(ty_arg, &generic_params);
                    let temp = self.builder.temp(RuntimeTy::type_type());
                    self.builder
                        .assign(Place::local(temp), Rvalue::LoadType(template));
                    Operand::Copy(Place::local(temp))
                })
                .collect()
        } else {
            vec![]
        };

        let type_arg_operands: Vec<Operand> = if !receiver_class_type_arg_operands.is_empty() {
            let mut combined = receiver_class_type_arg_operands;
            combined.extend(call_type_arg_operands);
            combined
        } else {
            call_type_arg_operands
        };
        let ntypeargs = type_arg_operands.len();
        let runtime_type_check = self.call_requires_runtime_type_check(expr_id);

        // Prepend type-arg operands before the value-arg operands.
        // (For regular BAML calls, type args are leading so the callee's frame
        // can pop them into `frame.type_args` before reading value args.)
        let all_arg_operands_for_call = if ntypeargs > 0 {
            let mut combined = type_arg_operands.clone();
            combined.extend(arg_operands.iter().cloned());
            combined
        } else {
            arg_operands.clone()
        };

        // BEP-034 `baml.future.__await_any(futures)` lowers to a dedicated
        // `Terminator::AwaitAny` suspend point (like `await`), not a call.
        if self.check_await_any(callee) {
            // The single value arg is the array of futures. (`__await_any` has
            // two type params T,E used only for type checking; the runtime
            // terminator just needs the array operand.)
            let futures_operand = arg_operands
                .into_iter()
                .next()
                .expect("__await_any takes exactly one (array) argument");
            match &dest {
                Place::Local(l) => {
                    self.builder
                        .await_any(futures_operand, Place::Local(*l), target, unwind);
                }
                _ => {
                    // Projection/capture destination: await into a temp, then
                    // assign across (mirrors the regular-call path below).
                    let call_ty = self.expr_ty(expr_id);
                    let tmp = self.builder.temp(call_ty);
                    self.builder
                        .await_any(futures_operand, Place::local(tmp), target, unwind);
                    self.builder.set_current_block(target);
                    let after = self.builder.create_block();
                    self.builder
                        .assign(dest, Rvalue::Use(Operand::Copy(Place::local(tmp))));
                    self.builder.goto(after);
                    self.builder.set_current_block(after);
                    return;
                }
            }
            self.builder.set_current_block(target);
            return;
        }

        if is_sys_op {
            // BEP-034 phase D′: sys-ops now lower to a single
            // `Terminator::SysOp` that runs the op inline in the
            // engine and binds the return value directly into `dest`
            // — no intermediate `Future` heap object, no separate
            // `Await` terminator, no `FutureManager` entry.
            //
            // The bytecode emit just becomes:
            //     <args ...>
            //     SYS_OP g
            //     <store dest>
            let dest_local = match dest {
                Place::Local(l) => l,
                _ => self.builder.temp(RuntimeTy::Null {
                    attr: TyAttr::default(),
                }),
            };
            // For generic IO builtins (`$rust_io_function` with type params),
            // the compiler may inject synthetic trailing value-arg slots for
            // runtime `baml_type::RuntimeTy` descriptors. The Rust glue reads them
            // positionally after the regular value args, so append them here
            // instead of prepending them like regular BAML frame type args.
            let sys_op_arg_operands = if ntypeargs > 0 {
                let mut combined = arg_operands;
                combined.extend(type_arg_operands);
                combined
            } else {
                arg_operands
            };
            let runtime_id_operand = self.lower_runtime_id_operand(runtime_id);
            self.builder.sys_op_with_runtime_id(
                callee_operand,
                sys_op_arg_operands,
                runtime_id_operand,
                Place::Local(dest_local),
                target,
                unwind,
            );
        } else {
            // Call destinations must be Place::Local in MIR. If `dest` is a
            // projection (Field/Index) or a capture, call into a temp local
            // first, then assign from the temp to the real destination.
            match &dest {
                Place::Local(_) => {
                    let runtime_id_operand = self.lower_runtime_id_operand(runtime_id);
                    self.builder.call_with_runtime_type_check(
                        callee_operand,
                        all_arg_operands_for_call,
                        ntypeargs,
                        runtime_type_check,
                        runtime_id_operand,
                        dest,
                        target,
                        unwind,
                    );
                }
                _ => {
                    let call_ty = self.expr_ty(expr_id);
                    let tmp = self.builder.temp(call_ty);
                    let runtime_id_operand = self.lower_runtime_id_operand(runtime_id);
                    self.builder.call_with_runtime_type_check(
                        callee_operand,
                        all_arg_operands_for_call,
                        ntypeargs,
                        runtime_type_check,
                        runtime_id_operand,
                        Place::local(tmp),
                        target,
                        unwind,
                    );
                    self.builder.set_current_block(target);
                    let after = self.builder.create_block();
                    self.builder
                        .assign(dest, Rvalue::Use(Operand::Copy(Place::local(tmp))));
                    self.builder.goto(after);
                    self.builder.set_current_block(after);
                    return;
                }
            }
        }

        self.builder.set_current_block(target);
    }

    /// Whether `callee` resolves to a `BoundMethod` — i.e. the call uses method
    /// convention (`self` passed implicitly via the receiver). Mirrors TIR's
    /// `callee_uses_method_call_convention`, which strips `self` so the call
    /// plan's `param_index` becomes receiver-relative.
    fn callee_uses_method_convention(&self, callee: AstExprId) -> bool {
        use crate::inference_provider::MemberResolution;
        let key = self.expr_metadata_key(callee);
        matches!(
            self.tir_resolution(key),
            Some(MemberResolution::BoundMethod { .. })
        ) || matches!(
            self.tir_resolution(key),
            Some(MemberResolution::External(external)) if external.takes_self
        ) || matches!(
            self.tir_path_member_resolutions(key)
                .and_then(|resolutions| resolutions.last()),
            Some(MemberResolution::BoundMethod { .. })
        ) || matches!(
            self.tir_path_member_resolutions(key)
                .and_then(|resolutions| resolutions.last()),
            Some(MemberResolution::External(external)) if external.takes_self
        )
    }

    fn sys_op_callee(&self, callee: AstExprId) -> Option<FunctionLoc<'db>> {
        use baml_compiler2_ast::BuiltinKind;

        // ── Path callee (single- or multi-segment) ─────────────────────────────
        if let AstExpr::Path(segments) = &self.body.exprs[callee] {
            let func_loc = if segments.len() == 1 {
                let span_start = self
                    .source_map
                    .as_ref()
                    .map(|sm| sm.expr_span(callee).start())
                    .unwrap_or_default();
                let resolved = resolve_name_at_in_scope(
                    self.db,
                    self.file,
                    span_start,
                    &segments[0],
                    self.scope_func_name.as_ref(),
                );
                match resolved {
                    ResolvedName::Builtin(Definition::Function(fl)) => Some(fl),
                    ResolvedName::Item(Definition::Function(fl)) => Some(fl),
                    _ => None,
                }
            } else {
                // Multi-segment: check path_member_resolutions first (local-rooted paths
                // like `file.read_string`), then fall back to flat resolutions (package paths).
                // The last resolution in path_member_resolutions is the final-segment resolution.
                let from_pmr = self
                    .tir_path_member_resolutions(self.expr_metadata_key(callee))
                    .and_then(|resolutions| resolutions.last())
                    .and_then(|res| resolution_func_loc(res));
                if from_pmr.is_some() {
                    from_pmr
                } else {
                    self.tir_resolution(self.expr_metadata_key(callee))
                        .and_then(|res| resolution_func_loc(res))
                }
            };
            if let Some(fl) = func_loc {
                let body = baml_compiler2_ppir::function_body(self.db, fl);
                if let FunctionBody::Builtin(BuiltinKind::Io) = body.as_ref() {
                    return Some(fl);
                }
            }
        }

        // ── NEW: MemberAccess callee (e.g. f.read, sock.recv) ──────────────────
        if let AstExpr::MemberAccess { .. } = &self.body.exprs[callee] {
            if let Some(resolution) = self.tir_resolution(self.expr_metadata_key(callee)) {
                let func_loc = resolution_func_loc(resolution);
                if let Some(fl) = func_loc {
                    let body = baml_compiler2_ppir::function_body(self.db, fl);
                    if let FunctionBody::Builtin(BuiltinKind::Io) = body.as_ref() {
                        return Some(fl);
                    }
                }
            }
        }

        None
    }

    fn check_await_any(&self, callee: AstExprId) -> bool {
        matches!(
            self.callee_builtin_kind(callee),
            Some(baml_compiler2_ast::BuiltinKind::AwaitAny)
        )
    }

    fn callee_builtin_kind(&self, callee: AstExprId) -> Option<baml_compiler2_ast::BuiltinKind> {
        // ── Path callee (single- or multi-segment) ─────────────────────────────
        if let AstExpr::Path(segments) = &self.body.exprs[callee] {
            let func_loc = if segments.len() == 1 {
                let span_start = self
                    .source_map
                    .as_ref()
                    .map(|sm| sm.expr_span(callee).start())
                    .unwrap_or_default();
                let resolved = resolve_name_at_in_scope(
                    self.db,
                    self.file,
                    span_start,
                    &segments[0],
                    self.scope_func_name.as_ref(),
                );
                match resolved {
                    ResolvedName::Builtin(Definition::Function(fl)) => Some(fl),
                    ResolvedName::Item(Definition::Function(fl)) => Some(fl),
                    _ => None,
                }
            } else {
                // Multi-segment: check path_member_resolutions first (local-rooted paths
                // like `file.read_string`), then fall back to flat resolutions (package paths).
                // The last resolution in path_member_resolutions is the final-segment resolution.
                let from_pmr = self
                    .tir_path_member_resolutions(self.expr_metadata_key(callee))
                    .and_then(|resolutions| resolutions.last())
                    .and_then(|res| resolution_func_loc(res));
                if from_pmr.is_some() {
                    from_pmr
                } else {
                    self.tir_resolution(self.expr_metadata_key(callee))
                        .and_then(|res| resolution_func_loc(res))
                }
            };
            if let Some(fl) = func_loc {
                let body = baml_compiler2_ppir::function_body(self.db, fl);
                if let FunctionBody::Builtin(kind) = body.as_ref() {
                    return Some(*kind);
                }
            }
        }

        // ── NEW: MemberAccess callee (e.g. f.read, sock.recv) ──────────────────
        if let AstExpr::MemberAccess { .. } = &self.body.exprs[callee] {
            if let Some(resolution) = self.tir_resolution(self.expr_metadata_key(callee)) {
                let func_loc = resolution_func_loc(resolution);
                if let Some(fl) = func_loc {
                    let body = baml_compiler2_ppir::function_body(self.db, fl);
                    if let FunctionBody::Builtin(kind) = body.as_ref() {
                        return Some(*kind);
                    }
                }
            }
        }

        None
    }

    fn sys_op_synthetic_type_arg_count(&self, callee: AstExprId) -> Option<usize> {
        use baml_compiler2_ast::BuiltinKind;

        // ── Path callee (single- or multi-segment) ─────────────────────────────
        if let AstExpr::Path(segments) = &self.body.exprs[callee] {
            let func_loc = if segments.len() == 1 {
                let span_start = self
                    .source_map
                    .as_ref()
                    .map(|sm| sm.expr_span(callee).start())
                    .unwrap_or_default();
                let resolved = resolve_name_at_in_scope(
                    self.db,
                    self.file,
                    span_start,
                    &segments[0],
                    self.scope_func_name.as_ref(),
                );
                match resolved {
                    ResolvedName::Builtin(Definition::Function(fl)) => Some(fl),
                    ResolvedName::Item(Definition::Function(fl)) => Some(fl),
                    _ => None,
                }
            } else {
                // Multi-segment: check path_member_resolutions first (local-rooted paths
                // like `file.read_string`), then fall back to flat resolutions (package paths).
                // The last resolution in path_member_resolutions is the final-segment resolution.
                let from_pmr = self
                    .tir_path_member_resolutions(self.expr_metadata_key(callee))
                    .and_then(|resolutions| resolutions.last())
                    .and_then(|res| resolution_func_loc(res));
                if from_pmr.is_some() {
                    from_pmr
                } else {
                    self.tir_resolution(self.expr_metadata_key(callee))
                        .and_then(|res| resolution_func_loc(res))
                }
            };
            if let Some(fl) = func_loc {
                let body = baml_compiler2_ppir::function_body(self.db, fl);
                if let FunctionBody::Builtin(BuiltinKind::Io) = body.as_ref() {
                    return Some(self.synthetic_type_arg_count_for_sys_op(fl));
                }
            }
        }

        // ── NEW: MemberAccess callee (e.g. f.read, sock.recv) ──────────────────
        if let AstExpr::MemberAccess { .. } = &self.body.exprs[callee] {
            if let Some(resolution) = self.tir_resolution(self.expr_metadata_key(callee)) {
                let func_loc = resolution_func_loc(resolution);
                if let Some(fl) = func_loc {
                    let body = baml_compiler2_ppir::function_body(self.db, fl);
                    if let FunctionBody::Builtin(BuiltinKind::Io) = body.as_ref() {
                        return Some(self.synthetic_type_arg_count_for_sys_op(fl));
                    }
                }
            }
        }

        None
    }

    fn synthetic_type_arg_count_for_sys_op(
        &self,
        func_loc: baml_compiler2_hir::loc::FunctionLoc<'_>,
    ) -> usize {
        let func = baml_compiler2_ppir::item_data::function_data(self.db, func_loc);
        let declared_type_value_params = func
            .params
            .iter()
            .filter(|param| {
                matches!(
                    param.type_ref.map(|id| &func.type_refs[id].kind),
                    Some(baml_compiler2_hir::type_ref::TypeRefKind::Type)
                )
            })
            .count();
        func.generic_params
            .len()
            .saturating_sub(declared_type_value_params)
    }

    /// Check if the callee resolves to a `$compiler_intrinsic` function and return the
    /// corresponding `IntrinsicOp`. Follows the same resolution pattern as
    /// `sys_op_synthetic_type_arg_count`.
    fn check_intrinsic(&self, callee: AstExprId) -> Option<IntrinsicOp> {
        use baml_compiler2_ast::BuiltinKind;

        // ── Path callee (single- or multi-segment) ─────────────────────────────
        if let AstExpr::Path(segments) = &self.body.exprs[callee] {
            let func_loc = if segments.len() == 1 {
                let span_start = self
                    .source_map
                    .as_ref()
                    .map(|sm| sm.expr_span(callee).start())
                    .unwrap_or_default();
                let resolved = resolve_name_at_in_scope(
                    self.db,
                    self.file,
                    span_start,
                    &segments[0],
                    self.scope_func_name.as_ref(),
                );
                match resolved {
                    ResolvedName::Builtin(Definition::Function(fl)) => Some(fl),
                    ResolvedName::Item(Definition::Function(fl)) => Some(fl),
                    _ => None,
                }
            } else {
                let from_pmr = self
                    .tir_path_member_resolutions(self.expr_metadata_key(callee))
                    .and_then(|resolutions| resolutions.last())
                    .and_then(|res| resolution_func_loc(res));
                if from_pmr.is_some() {
                    from_pmr
                } else {
                    self.tir_resolution(self.expr_metadata_key(callee))
                        .and_then(|res| resolution_func_loc(res))
                }
            };
            if let Some(fl) = func_loc {
                let body = baml_compiler2_ppir::function_body(self.db, fl);
                if let FunctionBody::Builtin(BuiltinKind::Intrinsic) = body.as_ref() {
                    let item_ref = def_to_item_ref(self.db, Definition::Function(fl));
                    return match item_ref.to_string().as_str() {
                        "log.info" => Some(IntrinsicOp::Log(LogLevel::Info)),
                        "log.debug" => Some(IntrinsicOp::Log(LogLevel::Debug)),
                        "log.warn" => Some(IntrinsicOp::Log(LogLevel::Warn)),
                        "log.error" => Some(IntrinsicOp::Log(LogLevel::Error)),
                        _ => None,
                    };
                }
            }
        }

        None
    }
}

// ─── 3.6: reflect.type_of intrinsic ─────────────────────────────────────────

impl LoweringContext<'_> {
    /// Detect a `reflect.type_of<T>()` call and, if found, resolve the type
    /// argument and return the corresponding `TyTemplate`.
    ///
    /// Returns `Some(template)` when:
    /// - The callee is the `baml.reflect.type_of` `$compiler_intrinsic`.
    /// - The call carries exactly one type argument.
    /// - The type argument resolves to a concrete `RuntimeTy` (no `TypeVar` leaves).
    ///
    /// Returns `None` when the callee is not `type_of` **or** when the type
    /// argument contains a `TypeVar` (generic-parameter reference).  The latter
    /// case is deferred to template lowering, which produces
    /// `TyTemplate::TypeArgRef` leaves; attempting it here would emit a broken
    /// `LoadType` instruction.
    fn check_type_of_intrinsic(
        &self,
        callee: AstExprId,
        call_expr_id: AstExprId,
    ) -> Option<TyTemplate> {
        use baml_compiler2_ast::BuiltinKind;

        // ── 1. Check the callee resolves to `baml.reflect.type_of` ──────────
        let func_loc = if let AstExpr::Path(segments) = &self.body.exprs[callee] {
            if segments.len() == 1 {
                let span_start = self
                    .source_map
                    .as_ref()
                    .map(|sm| sm.expr_span(callee).start())
                    .unwrap_or_default();
                let resolved = resolve_name_at_in_scope(
                    self.db,
                    self.file,
                    span_start,
                    &segments[0],
                    self.scope_func_name.as_ref(),
                );
                match resolved {
                    ResolvedName::Builtin(
                        baml_compiler2_hir::contributions::Definition::Function(fl),
                    ) => Some(fl),
                    ResolvedName::Item(
                        baml_compiler2_hir::contributions::Definition::Function(fl),
                    ) => Some(fl),
                    _ => None,
                }
            } else {
                let from_pmr = self
                    .tir_path_member_resolutions(self.expr_metadata_key(callee))
                    .and_then(|resolutions| resolutions.last())
                    .and_then(|res| resolution_func_loc(res));
                if from_pmr.is_some() {
                    from_pmr
                } else {
                    self.tir_resolution(self.expr_metadata_key(callee))
                        .and_then(|res| resolution_func_loc(res))
                }
            }
        } else {
            None
        }?;

        let body = baml_compiler2_ppir::function_body(self.db, func_loc);
        if !matches!(
            body.as_ref(),
            baml_compiler2_hir::body::FunctionBody::Builtin(BuiltinKind::Intrinsic)
        ) {
            return None;
        }
        let item_ref = def_to_item_ref(
            self.db,
            baml_compiler2_hir::contributions::Definition::Function(func_loc),
        );
        if item_ref.to_string().as_str() != "baml.type.of" {
            return None;
        }

        // ── 2. Extract the single type argument ─────────────────────────────
        let type_args = if let AstExpr::Call { type_args, .. } = &self.body.exprs[call_expr_id] {
            type_args.clone()
        } else {
            return None;
        };
        let type_arg = type_args.into_iter().next()?;
        let AstTypeArg::Static(type_arg) = type_arg else {
            return None;
        };

        // Include the enclosing class + function generic params so that `T`
        // in `reflect.type_of<T>()` resolves to `Tir2Ty::TypeVar("T")` rather
        // than an unresolved-type error — both for free generic functions and
        // for methods on generic classes.  The order (class params first,
        // then function params) mirrors TIR's `enclosing_class_generic_params
        // ++ user_generic_params` convention used in `callable.rs`.
        let generic_params = self.enclosing_generic_params();

        // ── 4. Build TyTemplate — TypeVar → TypeArgRef(N) ─────────────────────
        let template = self.type_expr_to_template(&type_arg, &generic_params);
        Some(template)
    }

    fn type_expr_to_template(
        &self,
        type_arg: &AstTypeExpr,
        generic_params: &[ParamTy],
    ) -> TyTemplate {
        // `Self.Item` in a default-method body is the assoc-name frame slot — desugar
        // before the frame-slot fast path so it maps to its `TypeArgRef`.
        let type_arg = &self.desugar_body_type_expr(type_arg);
        if let Some(template) = Self::direct_frame_type_arg_template(type_arg, generic_params) {
            return template;
        }
        let tir_ty = self.lower_type_arg_to_tir(type_arg, generic_params);
        self.ty_to_template(&tir_ty, generic_params)
    }

    /// Lower a written type-argument expression to its `Tir2Ty`, resolving names
    /// against the canonical (PPIR-merged) package items and with the enclosing
    /// generic params in scope (so `T` becomes `Tir2Ty::TypeVar("T")`). A `_`
    /// wildcard is a hard error at lowering (`CannotInferType`) and comes back
    /// as `Tir2Ty::Error`, so it never reaches runtime conversion.
    fn lower_type_arg_to_tir(&self, type_arg: &AstTypeExpr, generic_params: &[ParamTy]) -> Tir2Ty {
        let pkg_info = file_package(self.db, self.file);
        let pkg_id = PackageId::new(self.db, pkg_info.package);
        // The canonical (PPIR-merged) package items, NOT HIR's: explicit type
        // args synthesized by PPIR companions reference `*$stream` classes
        // (e.g. `parse<Payload$stream | null, Payload>`), which only exist in
        // the PPIR-expanded item universe. Resolving against HIR's original
        // items lowered them to `Unknown` → `Void` and broke `ParseCache.new`
        // at runtime.
        let pkg_items = baml_compiler2_ppir::package_items(self.db, pkg_id);
        lower_expr_in_scope(
            self.db,
            type_arg,
            pkg_items,
            &pkg_info.namespace_path,
            generic_params,
            &self.enclosing_generic_param_bounds(),
            self.body_self_tir_ty(),
        )
    }

    fn direct_frame_type_arg_template(
        type_arg: &AstTypeExpr,
        generic_params: &[ParamTy],
    ) -> Option<TyTemplate> {
        let AstTypeExprKind::Path {
            segments,
            generic_args,
            associated_type_bindings,
            ..
        } = &type_arg.kind
        else {
            return None;
        };
        if segments.len() != 1 || !generic_args.is_empty() || !associated_type_bindings.is_empty() {
            return None;
        }
        RuntimeGenericLayout::new(generic_params)
            .slot_by_name(&segments[0])
            .map(TyTemplate::TypeArgRef)
    }

    /// Recursively convert a `Tir2Ty` to a `TyTemplate`.
    ///
    /// `Tir2Ty::TypeVar("T")` whose name appears at position `N` in
    /// `generic_params` maps to `TyTemplate::TypeArgRef(N)`.  All other types
    /// recurse structurally and bottom out at fully-realized leaves.
    fn ty_to_template(&self, ty: &Tir2Ty, generic_params: &[ParamTy]) -> TyTemplate {
        // Delegate to the free `tir2_to_template` so the two routines can never
        // drift apart again (C1). They were previously byte-for-byte twins; a
        // missing `Tir2Ty::Interface` arm in both voided generic interface args
        // to `Box<void>` (BEP-044 wf3 #6/#7).
        tir2_to_template(ty, self.resolved_aliases, generic_params)
    }

    /// Return the list of generic parameter names in scope for the
    /// **enclosing** function being lowered.  Empty for top-level expressions
    /// that have no enclosing generic function.
    ///
    /// When the enclosing function is a method on a generic class, the
    /// class-level params come first, followed by the function-level params
    /// — matching TIR's `enclosing_class_generic_params ++ generic_params`
    /// convention (see `baml_compiler2_hir_ty::callable`).  This keeps MIR's
    /// view of in-scope generics consistent with how TIR types the body.
    ///
    /// Runtime lowering is responsible for seeding this frame layout: direct
    /// method calls prepend receiver class args, and interface dispatch seeds
    /// either static guard args or the matched receiver instance's class args.
    fn enclosing_generic_params(&self) -> Vec<ParamTy> {
        let mut params = self
            .func_loc
            .map(|fl| baml_compiler2_hir_ty::lower::function_generic_frame(self.db, fl))
            .unwrap_or_default();
        params.extend(self.lambda_generic_params.iter().cloned());
        params.extend(self.runtime_type_binding_params.iter().cloned());
        params
    }

    fn enclosing_runtime_type_arg_templates(&self) -> Vec<TyTemplate> {
        RuntimeGenericLayout::new(&self.enclosing_generic_params())
            .slots()
            .map(TyTemplate::TypeArgRef)
            .collect()
    }

    /// The interface bounds of the type variables in scope for this function body, keyed by
    /// name — the bounds analog of [`Self::enclosing_generic_params`]. Combines the function's
    /// own + enclosing class/interface bounds (`function_in_scope_generic_param_bounds`) with
    /// the enclosing out-of-body impl's generics' bounds (which that query does not cover), so a
    /// `T.member` projection in a method body resolves through `T`'s declared bound instead of
    /// erasing to `unknown`.
    fn enclosing_generic_param_bounds(
        &self,
    ) -> FxHashMap<ParamTy, Vec<baml_type::interned::InterfaceRef>> {
        let Some(fl) = self.func_loc else {
            return FxHashMap::default();
        };
        // hir_ty's one declaration-bounds road: class prefix, interface
        // Self env, free-impl generics, own params.
        baml_compiler2_hir_ty::lower::function_generic_bounds(self.db, fl)
    }

    /// Emit `LoadType` temps for a list of type args seeding a callee frame,
    /// returning one `Operand` per arg (in order). Used by the static
    /// `default.<method>()` bypass and by inferred-type-arg call lowering.
    /// `TypeVar`s are lowered against the *caller's* `enclosing_generic_params`
    /// so they substitute against the caller's `frame.type_args` at runtime
    /// (mirroring the receiver-class-type-args path for direct method calls).
    fn emit_frame_type_arg_ops(&mut self, tys: &[Tir2Ty]) -> Vec<Operand> {
        if tys.is_empty() {
            return Vec::new();
        }

        let generic_params = self.enclosing_generic_params();
        tys.iter()
            .map(|ty| {
                let template = self.ty_to_template(ty, &generic_params);
                let temp = self.builder.temp(RuntimeTy::type_type());
                self.builder
                    .assign(Place::local(temp), Rvalue::LoadType(template));
                Operand::Copy(Place::local(temp))
            })
            .collect()
    }

    fn lower_call_type_args(
        &mut self,
        call_expr_id: AstExprId,
        include_inferred: bool,
        max_count: Option<usize>,
    ) -> Vec<Operand> {
        if max_count == Some(0) {
            return Vec::new();
        }
        let Some(plan) = self
            .tir_call_plan(self.expr_metadata_key(call_expr_id))
            .cloned()
        else {
            return Vec::new();
        };

        // The inference plan is authoritative for every written slot. In
        // particular, extraction-contract types and mixed
        // `Static`/`unreflect(expr)` calls must never be re-lowered from AST
        // syntax under MIR's ordinary type position.
        if !plan.slots.is_empty() {
            let limit = max_count.unwrap_or(usize::MAX);
            let generic_params = self.enclosing_generic_params();
            let mut operands = Vec::with_capacity(plan.slots.len().min(limit));
            for slot in plan.slots.iter().take(limit) {
                match slot {
                    crate::inference_provider::CallTypeArgPlan::Static { emission_ty, .. } => {
                        let template = self.ty_to_template(emission_ty, &generic_params);
                        let temp = self.builder.temp(RuntimeTy::type_type());
                        self.builder
                            .assign(Place::local(temp), Rvalue::LoadType(template));
                        operands.push(Operand::Copy(Place::local(temp)));
                    }
                    crate::inference_provider::CallTypeArgPlan::Runtime { operand, .. } => {
                        operands.push(self.lower_to_operand(*operand));
                    }
                }
            }
            return operands;
        }

        if !include_inferred || plan.explicit {
            return Vec::new();
        }

        // Inferred calls thread only the callable-owned suffix. The receiver
        // or interface dispatcher supplies the owner prefix separately.
        let mut inferred_type_args: Vec<_> = plan.type_args[plan.own_offset..]
            .iter()
            .map(|ty| ty.clone().widen_fresh())
            .collect();
        let caller_generic_params = self.enclosing_generic_params();
        for ty in &mut inferred_type_args {
            if baml_type_runtime::contains_typevar_where(ty, &|name| {
                !caller_generic_params.iter().any(|param| param == name)
            }) {
                *ty = Tir2Ty::BuiltinUnknown {
                    attr: TyAttr::default(),
                };
            }
        }
        if let Some(max_count) = max_count {
            inferred_type_args.truncate(max_count);
        }
        // Seed every inferred arg — including an all-`unknown` list. It is a
        // system invariant that a callee frame supplies every slot its
        // templates reference (`TyTemplate::substitute` reports an
        // out-of-range ref as a frame-layout error), so a generic callee's
        // frame is always seeded at full declared width; a `T` inferred to the
        // top type is an explicit `unknown` slot, which is how
        // `reflect.type_of<T>()` under an unknown-typed call still reflects
        // the honest top type.
        self.emit_frame_type_arg_ops(&inferred_type_args)
    }

    fn call_requires_runtime_type_check(&self, call_expr_id: AstExprId) -> bool {
        use crate::inference_provider::{CallTypeArgPlan, RuntimeCheck};

        let scope = self.tables.for_scope(self.current_metadata_scope);
        let Some(plan) = scope.call_plan(call_expr_id) else {
            return false;
        };
        if plan
            .slots
            .iter()
            .any(|slot| matches!(slot, CallTypeArgPlan::Runtime { .. }))
            || !plan.deferred_checks.is_empty()
        {
            return true;
        }

        // Lexical runtime-type bindings defer checks in the body's durable
        // ledger rather than in one call plan. Associate argument checks back
        // to this call through its parameter bindings; a bound check is active
        // for the current lexical frame as a whole.
        scope.runtime_checks().iter().any(|check| match check {
            RuntimeCheck::Argument { arg, .. } => plan.provided_args().any(|it| it == *arg),
            RuntimeCheck::Bound { .. } => !self.runtime_type_binding_params.is_empty(),
        })
    }

    /// Lower `foo<int>` (a `GenericApply` value). If the base resolves to a
    /// function `ItemRef` and all type args are fully concrete, emit a pooled,
    /// interned `Constant::GenericFunction` (pointer-stable; seeds
    /// `frame.type_args` when called). Otherwise fall back to lowering the base
    /// value with type args erased — for exotic bases (bound methods, lambdas)
    /// or param-dependent args (`foo<T>` inside a generic function).
    fn lower_generic_apply(&mut self, base: AstExprId, type_args: &[AstTypeExpr], dest: Place) {
        let Some(item) = self.try_resolve_generic_apply_base(base) else {
            // Non-`ItemRef` base (a local/captured generic function value):
            // there is no function global to pool, so specialize the *runtime
            // value* — evaluate it and wrap it in a closure carrying the
            // (frame-resolved) type args — instead of silently erasing them.
            let value = self.lower_to_operand(base);
            let type_arg_templates = self.generic_apply_type_arg_templates(type_args);
            self.builder.assign(
                dest,
                Rvalue::MakeGenericFunctionFromValue {
                    value,
                    type_arg_templates,
                },
            );
            return;
        };
        let templates = self.generic_apply_type_arg_templates(type_args);
        if templates.iter().all(TyTemplate::is_fully_concrete) {
            // Concrete args → pooled, interned compile-time constant
            // (pointer-stable identity). Each template is fully concrete, so it
            // narrows directly to a `RealizedTy` — the value the runtime carries.
            let concrete: Vec<RealizedTy> = templates
                .iter()
                .map(|t| {
                    RealizedTy::try_from(t)
                        .unwrap_or_else(|e| unreachable!("checked fully concrete: {e}"))
                })
                .collect();
            self.builder.assign(
                dest,
                Rvalue::Use(Operand::Constant(Constant::GenericFunction {
                    item,
                    type_args: concrete,
                })),
            );
        } else {
            // A type arg depends on an enclosing generic param (`foo<T>` inside
            // a generic fn) → build the value at runtime, resolving the
            // templates against the current frame's type_args.
            self.builder.assign(
                dest,
                Rvalue::MakeGenericFunction {
                    item,
                    type_arg_templates: templates,
                },
            );
        }
    }

    /// Resolve a `GenericApply` base to the underlying function `ItemRef` (free
    /// function or static/interface method). `None` for bound methods, lambdas,
    /// or anything that is not a function path.
    fn try_resolve_generic_apply_base(&self, base: AstExprId) -> Option<ItemRef> {
        use crate::inference_provider::MemberResolution;
        let is_fn = |r: &MemberResolution<'_>| {
            matches!(
                r,
                MemberResolution::Free { .. }
                    | MemberResolution::UnboundMethod { .. }
                    | MemberResolution::InterfaceVirtualMethod { .. }
                    | MemberResolution::InterfaceConcreteMethod { .. }
            )
        };
        let key = self.expr_metadata_key(base);
        // Multi-segment paths: static methods, qualified free fns (e.g. baml.json.from_string).
        if let Some(item) = self
            .tir_path_member_resolutions(key)
            .and_then(|rs| rs.last())
            .filter(|r| is_fn(r))
            .and_then(|r| resolution_to_item_ref(self.db, r))
        {
            return Some(item);
        }
        // Flat / package resolutions.
        if let Some(item) = self
            .tir_resolution(key)
            .filter(|r| is_fn(r))
            .and_then(|r| resolution_to_item_ref(self.db, r))
        {
            return Some(item);
        }
        // Single-name free function / builtin.
        if let AstExpr::Path(segments) = &self.body.exprs[base]
            && segments.len() == 1
        {
            let span_start = self
                .source_map
                .as_ref()
                .map(|sm| sm.expr_span(base).start())
                .unwrap_or_default();
            match resolve_name_at_in_scope(
                self.db,
                self.file,
                span_start,
                &segments[0],
                self.scope_func_name.as_ref(),
            ) {
                ResolvedName::Item(def @ Definition::Function(_))
                | ResolvedName::Builtin(def @ Definition::Function(_)) => {
                    return Some(def_to_item_ref(self.db, def));
                }
                _ => {}
            }
        }
        None
    }

    /// Resolve `GenericApply` AST type args to `TyTemplate`s. A template is
    /// `is_fully_concrete()` unless the arg references an enclosing generic
    /// param (then it carries a `TypeArgRef`, resolved at runtime).
    fn generic_apply_type_arg_templates(&self, type_args: &[AstTypeExpr]) -> Vec<TyTemplate> {
        let generic_params = self.enclosing_generic_params();
        type_args
            .iter()
            .map(|type_arg| self.type_expr_to_template(type_arg, &generic_params))
            .collect()
    }
}

// ─── 3.7: Helper methods ─────────────────────────────────────────────────────

impl<'db> LoweringContext<'db> {
    fn lower_to_operand(&mut self, expr_id: AstExprId) -> Operand {
        let ty = self.expr_ty(expr_id);
        let temp = self.builder.temp(ty);
        self.lower_expr(expr_id, Place::local(temp));
        Operand::Copy(Place::Local(temp))
    }

    fn lower_throw_operand(&mut self, expr_id: AstExprId) -> Operand {
        self.try_resolve_to_local(expr_id)
            .map_or_else(|| self.lower_to_operand(expr_id), Operand::copy_local)
    }

    fn emit_panic_call(&mut self, message: &str, _expr_id: AstExprId) {
        // Emit a call to baml.sys.panic with the error message
        let callee = Operand::Constant(Constant::Function(ItemRef::Free {
            package: Name::new("baml"),
            namespace: vec![Name::new("sys")],
            name: Name::new("panic"),
        }));
        let msg = Operand::Constant(Constant::String(message.to_string()));
        let temp = self.builder.temp(RuntimeTy::Null {
            attr: TyAttr::default(),
        });
        let unreachable_block = self.builder.create_block();
        self.builder.call(
            callee,
            vec![msg],
            Place::local(temp),
            unreachable_block,
            None,
        );
        self.builder.set_current_block(unreachable_block);
        self.builder.unreachable();
        // Start a new block for any code after this (dead code)
        let dead = self.builder.create_block();
        self.builder.set_current_block(dead);
    }

    fn lower_current_runtime_id(&mut self, dest: Place) {
        let callee = Operand::Constant(Constant::Function(ItemRef::Free {
            package: Name::new("baml"),
            namespace: vec![Name::new("id")],
            name: Name::new("current"),
        }));
        let resume = self.builder.create_block();
        let unwind = self.catch_context.as_ref().map(|c| c.unwind_target);
        self.builder.call(callee, Vec::new(), dest, resume, unwind);
        self.builder.set_current_block(resume);
    }

    fn lower_set_runtime_id(&mut self, value: AstExprId) {
        let callee = Operand::Constant(Constant::Function(ItemRef::Free {
            package: Name::new("baml"),
            namespace: vec![Name::new("id")],
            name: Name::new("set"),
        }));
        let arg = self.lower_to_operand(value);
        let dest = self.builder.temp(RuntimeTy::String {
            attr: TyAttr::default(),
        });
        let resume = self.builder.create_block();
        let unwind = self.catch_context.as_ref().map(|c| c.unwind_target);
        self.builder
            .call(callee, vec![arg], Place::local(dest), resume, unwind);
        self.builder.set_current_block(resume);
    }

    /// The `$id` runtime-identity special form. MIR owns its lowering (reads
    /// → `baml.id.current()`, plain `=` writes → `baml.id.set(...)`); TIR
    /// owns its typing and rejects the invalid shapes (compound assignment,
    /// member access, call-site labels, `$id` bindings) — see
    /// `infer_path` / `Stmt::Assign` / `Stmt::AssignOp` in
    /// `hir_ty`'s inference. Keep the two layers in sync.
    fn is_runtime_id_path(expr: &AstExpr) -> bool {
        matches!(expr, AstExpr::Path(segments) if segments.len() == 1 && segments[0].as_str() == "$id")
    }

    fn lower_if(
        &mut self,
        _expr_id: AstExprId,
        condition: AstExprId,
        then_branch: AstExprId,
        else_branch: Option<AstExprId>,
        dest: Place,
    ) {
        let cond_op = self.lower_to_operand(condition);
        let bb_then = self.builder.create_block();
        let bb_else = self.builder.create_block();
        let bb_join = self.builder.create_block();

        self.builder.branch(cond_op, bb_then, bb_else);

        self.builder.set_current_block(bb_then);
        self.lower_expr(then_branch, dest.clone());
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        self.builder.set_current_block(bb_else);
        if let Some(else_expr) = else_branch {
            self.lower_expr(else_expr, dest);
        } else {
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
        }
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        self.builder.set_current_block(bb_join);
    }

    /// MIR lowering for `if let PATTERN = SCRUTINEE { THEN } else { ELSE }`.
    ///
    /// Same shape as a two-arm match (`PATTERN => then, _ => else`), but we
    /// emit it inline rather than synthesizing a match. The pattern test
    /// jumps to the then-block on success (where we bind names from the
    /// pattern before lowering the body) and to the else-block on failure.
    fn lower_if_let(
        &mut self,
        _expr_id: AstExprId,
        pattern: AstPatId,
        scrutinee: AstExprId,
        then_branch: AstExprId,
        else_branch: Option<AstExprId>,
        dest: Place,
    ) {
        let scrutinee_local = self.try_resolve_to_local(scrutinee).unwrap_or_else(|| {
            let op = self.lower_to_operand(scrutinee);
            let ty = self.expr_ty(scrutinee);
            self.operand_to_local(op, ty)
        });

        let bb_then = self.builder.create_block();
        let bb_else = self.builder.create_block();
        let bb_join = self.builder.create_block();

        self.lower_pattern_test(scrutinee_local, pattern, bb_then, bb_else);

        // Then-branch: bind pattern locals, lower body, restore on exit.
        self.builder.set_current_block(bb_then);
        let saved_locals = self.locals.clone();
        self.bind_pattern(scrutinee_local, pattern);
        self.lower_expr(then_branch, dest.clone());
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }
        self.restore_locals_after_scope(saved_locals);

        // Else-branch: no bindings from the pattern, just lower the else
        // (or write Null if absent — same as plain `if` with no else).
        self.builder.set_current_block(bb_else);
        if let Some(else_expr) = else_branch {
            self.lower_expr(else_expr, dest);
        } else {
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
        }
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        self.builder.set_current_block(bb_join);
    }

    fn lower_object(
        &mut self,
        expr_id: AstExprId,
        type_name: &TypePath,
        type_args: &[AstTypeExpr],
        fields: &[baml_compiler2_ast::ObjectExprField],
        spreads: &[baml_compiler2_ast::SpreadField],
        dest: Place,
    ) {
        // Prefer the explicitly written type name. If absent (e.g., when the
        // type is a qualified path like `baml.errors.DevOther`), fall back to
        // the TIR-inferred type to get the short class name.
        //
        // We also extract a `TypeName` for looking up fields in `class_fields`,
        // which is keyed by `TypeName`.
        let ty = self.expr_ty(expr_id);
        let type_name_key: Option<TypeName> = match &ty {
            RuntimeTy::Class(tn, _, _) => Some(tn.clone()),
            _ => None,
        };
        // Prefer the TIR-resolved fully-qualified name (`<package>.<ns>.<name>`)
        // because that matches the bytecode emitter's FQN registry. The parser
        // stores qualified paths verbatim from source (e.g. `root.http.Response`
        // for user types), but the emitter registers user types under the `user.`
        // prefix — so the source-verbatim form would miss the lookup. Falling
        // back to the parser name only when TIR has no type info handles
        // synthetic Object exprs from `lower_cst.rs` that already use registry-
        // matching dotted forms like "ai.Prompt".
        let class_name = if let Some(tn) = &type_name_key {
            if tn.is_runtime_minted() {
                // A mounted runtime type keeps its hidden mint-qualified QTN
                // in the value type, but its class object is linked through
                // the source-visible package alias written at this literal
                // (`app.Export`). The runtime linker resolves that symbol to
                // the dependency's minted class object.
                type_name.to_string()
            } else {
                tn.render_dotted(false)
            }
        } else {
            type_name.to_string()
        };
        let field_slot_count = |field_name_to_idx: &IndexMap<String, usize>| {
            field_name_to_idx
                .values()
                .copied()
                .max()
                .map(|idx| idx + 1)
                .unwrap_or(0)
        };

        if spreads.is_empty() {
            // Lower fields in class-definition order, filling unspecified slots
            // with Null. Source order in the literal does not match definition
            // order, so a partial literal like `ScanOptions { absolute: true }`
            // would otherwise put `absolute` into whichever slot happens to be
            // first. The TIR Object handler resolves the type via its qualified
            // path, so `class_fields.get(tn)` always finds the definition for
            // any user-written class literal.
            let field_operands: Vec<Operand> = if let Some(field_name_to_idx) = type_name_key
                .as_ref()
                .and_then(|tn| self.class_fields.get(tn))
                .cloned()
            {
                let mut result: Vec<Operand> = (0..field_slot_count(&field_name_to_idx))
                    .map(|_| Operand::Constant(Constant::Null))
                    .collect();
                for field in fields {
                    if let Some(&idx) = field_name_to_idx.get(&field.name.to_string()) {
                        result[idx] = self.lower_to_operand(field.value);
                    }
                }
                result
            } else {
                // Synthetic Object exprs without TIR type info (e.g. compiler
                // sugar for retry policies) fall back to source order. These
                // construction sites build full, ordered literals so the order
                // matches the class definition.
                fields
                    .iter()
                    .map(|field| self.lower_to_operand(field.value))
                    .collect()
            };
            let type_arg_templates = self.object_class_type_arg_templates(expr_id, type_args);
            self.builder.assign(
                dest,
                Rvalue::Aggregate {
                    kind: AggregateKind::Class {
                        name: class_name,
                        type_arg_templates,
                    },
                    fields: field_operands,
                },
            );
        } else {
            // Lower spread base(s) and explicit fields eagerly (in source
            // order), then assemble the aggregate respecting override semantics:
            // later source entries override earlier ones for the same class field.

            enum Entry {
                Spread(Local),
                Named(String, Operand),
            }

            let field_count = type_name_key
                .as_ref()
                .and_then(|tn| self.class_fields.get(tn))
                .map(field_slot_count)
                .unwrap_or(0);

            // Lower all spread expressions into locals.
            let spread_locals: Vec<(usize, Local)> = spreads
                .iter()
                .map(|s| {
                    let op = self.lower_to_operand(s.expr);
                    let ty = self.expr_ty(s.expr);
                    (s.position, self.operand_to_local(op, ty))
                })
                .collect();

            // Lower all explicit field expressions into operands.
            // Named fields occupy source positions 0.. excluding spread positions.
            // Assign each named field its source position by counting up and
            // skipping positions occupied by spreads.
            let spread_positions: HashSet<usize> = spreads.iter().map(|s| s.position).collect();
            let explicit_with_pos: Vec<(usize, String, Operand)> = {
                let mut pos = 0usize;
                fields
                    .iter()
                    .map(|field| {
                        while spread_positions.contains(&pos) {
                            pos += 1;
                        }
                        let cur = pos;
                        pos += 1;
                        (
                            cur,
                            field.name.to_string(),
                            self.lower_to_operand(field.value),
                        )
                    })
                    .collect()
            };

            // Build per-class-field operand array. Process all entries in source
            // position order; later entries overwrite earlier ones.
            let field_name_to_idx: &IndexMap<String, usize> = match type_name_key
                .as_ref()
                .and_then(|tn| self.class_fields.get(tn))
            {
                Some(m) => m,
                None => {
                    // Unknown class — just emit named fields in order.
                    let field_operands: Vec<Operand> = fields
                        .iter()
                        .map(|field| self.lower_to_operand(field.value))
                        .collect();
                    let type_arg_templates =
                        self.object_class_type_arg_templates(expr_id, type_args);
                    self.builder.assign(
                        dest,
                        Rvalue::Aggregate {
                            kind: AggregateKind::Class {
                                name: class_name,
                                type_arg_templates,
                            },
                            fields: field_operands,
                        },
                    );
                    return;
                }
            };

            // Merge all entries into a single sorted list by source position.
            let mut entries: Vec<(usize, Entry)> = Vec::new();
            for (pos, local) in &spread_locals {
                entries.push((*pos, Entry::Spread(*local)));
            }
            for (pos, name, op) in explicit_with_pos {
                entries.push((pos, Entry::Named(name, op)));
            }
            entries.sort_by_key(|(pos, _)| *pos);

            // Initialize all fields to null, then apply entries in order.
            let mut result: Vec<Operand> = (0..field_count)
                .map(|_| Operand::Constant(Constant::Null))
                .collect();

            for (_, entry) in &entries {
                match entry {
                    Entry::Spread(local) => {
                        // A spread fills every field from the base object.
                        for (idx, slot) in result.iter_mut().enumerate().take(field_count) {
                            *slot = Operand::Copy(Place::Field {
                                base: Box::new(Place::Local(*local)),
                                field: idx,
                            });
                        }
                    }
                    Entry::Named(name, op) => {
                        if let Some(&idx) = field_name_to_idx.get(name) {
                            result[idx] = op.clone();
                        }
                    }
                }
            }

            let type_arg_templates = self.object_class_type_arg_templates(expr_id, type_args);
            self.builder.assign(
                dest,
                Rvalue::Aggregate {
                    kind: AggregateKind::Class {
                        name: class_name,
                        type_arg_templates,
                    },
                    fields: result,
                },
            );
        }
    }

    fn lower_member_access(
        &mut self,
        expr_id: AstExprId,
        base: AstExprId,
        field: &Name,
        dest: Place,
    ) {
        // Check if TIR resolved this to a method or free function — if so, emit a function constant
        // (unbound) or MakeBoundMethod (bound). Field and Variant resolutions fall through to the
        // existing lowering paths below.
        if let Some(resolution) = self
            .tir_resolution(self.expr_metadata_key(expr_id))
            .cloned()
        {
            use crate::inference_provider::MemberResolution;
            match &resolution {
                MemberResolution::BoundMethod { .. } => {
                    // Bound method reference: lower receiver and emit MakeBoundMethod.
                    let item = resolution_to_item_ref(self.db, &resolution);
                    if let Some(item) = item {
                        let receiver_op = self.lower_to_operand(base);
                        self.builder.assign(
                            dest,
                            Rvalue::MakeBoundMethod {
                                item_ref: item,
                                receiver: receiver_op,
                            },
                        );
                        return;
                    }
                }
                MemberResolution::UnboundMethod { .. } | MemberResolution::Free { .. } => {
                    // Unbound method or free function reference: emit a plain function constant.
                    let item = resolution_to_item_ref(self.db, &resolution);
                    if let Some(item) = item {
                        self.builder.assign(
                            dest,
                            Rvalue::Use(Operand::Constant(Constant::Function(item))),
                        );
                        return;
                    }
                }
                MemberResolution::InterfaceVirtualMethod { .. }
                | MemberResolution::InterfaceConcreteMethod { .. } => {
                    // A member-access base is a *value*: the reference must capture
                    // the receiver and bind its impl at runtime — the virtual-bound
                    // path below handles it.
                }
                MemberResolution::External(external) => {
                    use baml_compiler2_hir_ty::callable::ExternalCallTarget;
                    match &external.target {
                        ExternalCallTarget::Method { .. } if external.takes_self => {
                            if let Some(item) = resolution_to_item_ref(self.db, &resolution) {
                                let receiver_op = self.lower_to_operand(base);
                                self.builder.assign(
                                    dest,
                                    Rvalue::MakeBoundMethod {
                                        item_ref: item,
                                        receiver: receiver_op,
                                    },
                                );
                                return;
                            }
                        }
                        ExternalCallTarget::Interface { .. } => {}
                        ExternalCallTarget::Free { .. } | ExternalCallTarget::Method { .. } => {
                            if let Some(item) = resolution_to_item_ref(self.db, &resolution) {
                                self.builder.assign(
                                    dest,
                                    Rvalue::Use(Operand::Constant(Constant::Function(item))),
                                );
                                return;
                            }
                        }
                    }
                }
                MemberResolution::Field { .. }
                | MemberResolution::Variant { .. }
                | MemberResolution::InterfaceVirtualField { .. }
                | MemberResolution::ExternalField { .. }
                | MemberResolution::ExternalVariant { .. }
                | MemberResolution::ExternalInterfaceVirtualField { .. } => {
                    // Fall through — handled by the existing field / enum-variant / interface
                    // virtual-field lowering below (a virtual field read on an existential).
                }
            }
        }

        // An interface method referenced as a *value* on a generic- or
        // interface-typed receiver (`let f = x.eq`): there is no single concrete
        // method to bind statically, so bind the receiver's impl method by its
        // runtime type at bind time — the value analogue of the virtual call. The
        // declaring interface is resolved *before* lowering the receiver so a
        // field access (no such method) falls through to the field path below
        // without evaluating the receiver expression twice.
        if let Some(view) = self.dispatch_target_for_member_access(expr_id, base, field)
            && self.mir_interface_declares_method(&view.0, field)
        {
            let recv_op = self.lower_to_operand(base);
            let recv_local = self.builder.temp(self.expr_ty(base));
            self.builder
                .assign(Place::local(recv_local), Rvalue::Use(recv_op));
            self.emit_virtual_bound_method(recv_local, &view, field, &dest);
            return;
        }

        // Check if TIR resolved this to an enum variant (e.g. baml.HttpMethod.Get via package path)
        if let Some(Tir2Ty::EnumVariant(qtn, variant, _)) = self
            .tir_expr_type(self.expr_metadata_key(expr_id))
            .cloned()
            .as_ref()
        {
            let enum_ref = ItemRef::EnumType {
                package: qtn.package().clone(),
                namespace: qtn.namespace().clone(),
                name: qtn.name().clone(),
            };
            self.builder.assign(
                dest,
                Rvalue::Use(Operand::Constant(Constant::EnumVariant {
                    enum_ref,
                    variant: variant.clone(),
                })),
            );
            return;
        }

        // Check if this is a package path intermediate (e.g. `baml.HttpMethod` in
        // `baml.HttpMethod.Get`). TIR marks these as RuntimeTy::Unknown. Emit null placeholder.
        // CRITICAL: only treat the expression as a namespace intermediate if the BASE
        // is also Unknown (i.e. `baml` in `baml.HttpMethod`). If the base has a
        // concrete type, this is a real field access whose field type happens to be
        // Unknown (unresolved type annotation). In that case, fall through to emit
        // the field projection.
        if let Some(Tir2Ty::Unknown { .. }) = self.tir_expr_type(self.expr_metadata_key(expr_id)) {
            let base_is_also_unknown = self
                .tir_expr_type(self.expr_metadata_key(base))
                .map(|ty| matches!(ty, Tir2Ty::Unknown { .. }))
                .unwrap_or(true);
            if base_is_also_unknown {
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
                return;
            }
            // Base is a real value (non-Unknown type) — fall through to field projection
        }

        // Regular field access
        let base_ty = self.expr_ty(base);
        let base_op = self.lower_to_operand(base);
        let field_str = field.to_string();

        // Unwrap Optional — when called from lower_optional_member_access,
        // the base type is T? but we've already null-checked, so use the inner type.
        let unwrapped_ty = base_ty.strip_null();

        // Look up field index from class_fields
        let field_idx = if let RuntimeTy::Class(tn, _, _) = &unwrapped_ty {
            self.class_fields
                .get(tn)
                .and_then(|fields| fields.get(&field_str))
                .copied()
        } else {
            None
        };

        let base_local = self.operand_to_local(base_op, base_ty);

        if let Some(idx) = field_idx {
            self.builder.assign(
                dest,
                Rvalue::Use(Operand::Copy(Place::Field {
                    base: Box::new(Place::Local(base_local)),
                    field: idx,
                })),
            );
        } else {
            // TIR's own resolution first — it names the interface the access was
            // *checked* through, including the shared interface of a union receiver,
            // which no inspection of the receiver's type can recover.
            let handled_interface_field = if let Some(((tn, args, assoc), _index)) =
                self.tir_virtual_field_view(self.expr_metadata_key(expr_id))
            {
                self.try_lower_interface_field_access(base_local, &tn, &args, &assoc, field, &dest)
            } else {
                // Fallback for receivers TIR recorded no virtual-field resolution
                // for. `field` selects among a bounded type variable's bound
                // conjunction, where the field may come from any conjunct.
                self.interface_receiver_for_field_access(base, field, &unwrapped_ty)
                    .is_some_and(|(iface_tn, iface_type_args, iface_assoc)| {
                        self.try_lower_interface_field_access(
                            base_local,
                            &iface_tn,
                            &iface_type_args,
                            &iface_assoc,
                            field,
                            &dest,
                        )
                    })
            };
            let handled_union_field = handled_interface_field
                || self.lower_union_class_field_access(
                    expr_id,
                    base_local,
                    &unwrapped_ty,
                    field,
                    &dest,
                );
            if handled_union_field {
                return;
            }
            if let RuntimeTy::Class(tn, _, _) = &unwrapped_ty {
                self.emit_panic_call(
                    &format!(
                        "internal compiler error: MIR failed to resolve field access \
                         .{} against class definition '{}' (module_path: {:?}). \
                         This class should be in class_fields but isn't.",
                        field_str,
                        tn.name(),
                        tn.module_path(),
                    ),
                    expr_id,
                );
                return;
            }
            // Dynamic map access — only valid for map types, unknown, etc.
            let key_local = self.builder.temp(RuntimeTy::String {
                attr: TyAttr::default(),
            });
            self.builder.assign(
                Place::local(key_local),
                Rvalue::Use(Operand::Constant(Constant::String(field_str))),
            );
            self.builder.assign(
                dest,
                Rvalue::Use(Operand::Copy(Place::Index {
                    base: Box::new(Place::Local(base_local)),
                    index: key_local,
                    kind: IndexKind::Map,
                })),
            );
        }
    }

    fn interface_receiver_for_field_access(
        &self,
        base: AstExprId,
        field: &Name,
        unwrapped_ty: &RuntimeTy,
    ) -> Option<InterfaceTypeView> {
        if let Some(target) = self.interface_dispatch_target_for_expr_member(base, field) {
            return Some(target);
        }

        match unwrapped_ty {
            RuntimeTy::Class(tn, _, _) if self.is_interface_type_name(tn) => {
                Some((tn.clone(), Vec::new(), Vec::new()))
            }
            RuntimeTy::Interface(tn, _, _, _) if self.is_interface_type_name(tn) => {
                Some((tn.clone(), Vec::new(), Vec::new()))
            }
            _ => None,
        }
    }

    fn interface_receiver_for_path_prefix(
        &self,
        expr_id: AstExprId,
        prefix_idx: usize,
        member: &Name,
        current_ty: &RuntimeTy,
    ) -> Option<InterfaceTypeView> {
        if let Some(target) = self
            .tir_path_segment_type((self.current_metadata_scope, expr_id, prefix_idx))
            .and_then(|ty| self.interface_dispatch_target_for_member(ty, member))
        {
            return Some(target);
        }
        if prefix_idx == 0
            && let Some(target) = self
                .tir_path_root_type(self.expr_metadata_key(expr_id))
                .and_then(|ty| self.interface_dispatch_target_for_member(ty, member))
        {
            return Some(target);
        }

        match current_ty {
            RuntimeTy::Class(tn, _, _) if self.is_interface_type_name(tn) => {
                Some((tn.clone(), Vec::new(), Vec::new()))
            }
            RuntimeTy::Interface(tn, _, _, _) if self.is_interface_type_name(tn) => {
                Some((tn.clone(), Vec::new(), Vec::new()))
            }
            _ => None,
        }
    }

    fn class_receiver_for_path_prefix(
        &self,
        expr_id: AstExprId,
        prefix_idx: usize,
        current_ty: &RuntimeTy,
    ) -> Option<(TypeName, Vec<RuntimeTy>)> {
        let tir_prefix_ty = if prefix_idx == 0 {
            self.tir_path_root_type(self.expr_metadata_key(expr_id))
        } else {
            self.tir_path_segment_type((self.current_metadata_scope, expr_id, prefix_idx))
        };
        if let Some(target) = tir_prefix_ty.and_then(|ty| self.class_dispatch_target_for_tir_ty(ty))
        {
            return Some(target);
        }

        match current_ty {
            RuntimeTy::Class(tn, type_args, _) => Some((tn.clone(), type_args.clone())),
            _ => None,
        }
    }

    fn lower_union_class_field_access(
        &mut self,
        _expr_id: AstExprId,
        base_local: Local,
        base_ty: &RuntimeTy,
        field: &Name,
        dest: &Place,
    ) -> bool {
        let Some(candidates) = self.class_union_field_candidates(base_ty, field) else {
            return false;
        };

        let bb_entry = self.builder.current_block();
        let bb_join = self.builder.create_block();
        let bb_otherwise = self.builder.create_block();

        let tag_local = self.builder.temp(RuntimeTy::Int {
            attr: TyAttr::default(),
        });
        self.builder.assign(
            Place::local(tag_local),
            Rvalue::TypeTag(Place::local(base_local)),
        );

        let mut arms = Vec::with_capacity(candidates.len());
        let mut arm_names = Vec::with_capacity(candidates.len());
        for (tag, class_name, field_idx) in candidates {
            let bb_body = self.builder.create_block();
            arms.push((tag, bb_body));
            arm_names.push((tag, class_name.name().to_string()));

            self.builder.set_current_block(bb_body);
            self.builder.assign(
                dest.clone(),
                Rvalue::Use(Operand::Copy(Place::Field {
                    base: Box::new(Place::Local(base_local)),
                    field: field_idx,
                })),
            );
            self.builder.goto(bb_join);
        }

        self.builder.set_current_block(bb_otherwise);
        self.builder.unreachable();

        self.builder.set_current_block(bb_entry);
        self.builder.switch(
            Operand::Copy(Place::Local(tag_local)),
            arms,
            bb_otherwise,
            true,
            arm_names,
        );
        self.builder.set_current_block(bb_join);
        true
    }

    fn class_union_field_candidates(
        &self,
        ty: &RuntimeTy,
        field: &Name,
    ) -> Option<Vec<(i64, TypeName, usize)>> {
        // A union's arms are the whole candidate set — the type itself closes it, so
        // a runtime switch over them is complete by construction. That is the only
        // legitimate shape here: an interface's implementor set is open, and with
        // generic impls unbounded, so it is never enumerable.
        let class_names: Vec<TypeName> = match ty {
            RuntimeTy::Union(members, _) => members
                .iter()
                .filter_map(|m| match m {
                    RuntimeTy::Class(n, _, _) => Some(n.clone()),
                    _ => None,
                })
                .collect(),
            _ => return None,
        };
        if class_names.is_empty() {
            return None;
        }

        let mut candidates = Vec::new();
        for class_name in &class_names {
            let field_idx = self
                .class_fields
                .get(class_name)
                .and_then(|fields| fields.get(field.as_str()))
                .copied()?;
            let tag = self.class_type_tags.get(class_name).copied()?;
            if !candidates
                .iter()
                .any(|(existing_tag, _, _)| *existing_tag == tag)
            {
                candidates.push((tag, class_name.clone(), field_idx));
            }
        }

        (!candidates.is_empty()).then_some(candidates)
    }

    fn lower_index(&mut self, base: AstExprId, index: AstExprId, dest: Place) {
        let base_ty = self.expr_ty(base);
        let base_op = self.lower_to_operand(base);
        let index_ty = self.expr_ty(index);
        let index_op = self.lower_to_operand(index);
        self.emit_index_access(base_op, &base_ty, index_op, index_ty, dest);
    }

    /// Emit the element read for `base[index]` from already-lowered operands.
    /// Shared by `lower_index` and `lower_optional_index_access` so a
    /// side-effectful index expression is evaluated exactly once.
    fn emit_index_access(
        &mut self,
        base_op: Operand,
        base_ty: &RuntimeTy,
        index_op: Operand,
        index_ty: RuntimeTy,
        dest: Place,
    ) {
        let base_local = self.operand_to_local(base_op, base_ty.clone());
        let index_local = self.operand_to_local(index_op, index_ty);

        // Unwrap Optional — when called from lower_optional_index,
        // the base type is T? but we've already null-checked.
        let unwrapped_ty = base_ty.strip_null();

        let kind = if matches!(
            &unwrapped_ty,
            RuntimeTy::List(..) | RuntimeTy::Uint8Array { .. }
        ) {
            IndexKind::Array
        } else {
            IndexKind::Map
        };

        self.builder.assign(
            dest,
            Rvalue::Use(Operand::Copy(Place::Index {
                base: Box::new(Place::Local(base_local)),
                index: index_local,
                kind,
            })),
        );
    }

    /// If the expression is a simple local variable reference (single-segment path
    /// resolving to a known local), return its Local directly without allocating
    /// a temp or emitting a copy.
    fn try_resolve_to_local(&self, expr_id: AstExprId) -> Option<Local> {
        let expr = &self.body.exprs[expr_id];
        if let AstExpr::Path(segments) = expr {
            if segments.len() == 1 {
                if let Some(local) = self.local_for_path(expr_id, &segments[0]) {
                    return Some(local);
                }
            }
        }
        None
    }

    /// Convert an operand to a local, materializing a temp if necessary.
    fn operand_to_local(&mut self, op: Operand, ty: RuntimeTy) -> Local {
        match op {
            Operand::Copy(Place::Local(l)) | Operand::Move(Place::Local(l)) => l,
            _ => {
                let temp = self.builder.temp(ty);
                self.builder.assign(Place::local(temp), Rvalue::Use(op));
                temp
            }
        }
    }

    /// BEP-044: emit a type-tag switch over the implementor set when calling
    /// a method on an interface-typed receiver. Each arm invokes the
    /// concrete implementor's `<class>.<method>` as a static call.
    ///
    /// Returns `true` when dispatch was emitted. Returns `false` (without
    /// touching the builder) when the receiver isn't interface-typed or no
    /// implementors are registered — the regular call lowering then runs.
    fn try_lower_interface_dispatch(
        &mut self,
        expr_id: AstExprId,
        base: AstExprId,
        method: &Name,
        args: &[AstExprId],
        runtime_id: Option<AstExprId>,
        dest: &Place,
    ) -> bool {
        let dispatch_target = self
            .interface_dispatch_target_for_expr_member(base, method)
            .or_else(|| {
                self.tir_expr_type(self.expr_metadata_key(base))
                    .and_then(|ty| self.dispatch_target_for_concrete(ty, method))
            });
        let Some((iface_tn, iface_type_args, iface_assoc)) = dispatch_target else {
            return false;
        };
        let receiver_op = self.lower_to_operand(base);
        let receiver_ty = self.expr_ty(base);
        let recv_local = self.operand_to_local(receiver_op, receiver_ty);
        // Every interface-mediated call dispatches open-world via a virtual call:
        // the VM resolves the impl from the receiver's runtime concrete type
        // (containers included — array/map values carry their element types), so a
        // statically-undetermined receiver (a bounded type-var, an existential,
        // `Self` in a default body) and a concrete one route identically. Key the
        // call on the interface that *declares* `method` (which may be a `requires`
        // super-interface of the receiver's static interface).
        let (decl_tn, decl_args, decl_assoc) =
            self.interface_view_declaring_method(&(iface_tn, iface_type_args, iface_assoc), method);
        self.emit_virtual_call(
            recv_local,
            &decl_tn,
            &decl_args,
            &decl_assoc,
            method,
            expr_id,
            args,
            runtime_id,
            dest,
        )
    }

    /// UFCS twin of interface dispatch. Its call plan is self-inclusive, so
    /// lower the complete source argument list once and peel off the receiver.
    fn try_lower_interface_ufcs_dispatch(
        &mut self,
        expr_id: AstExprId,
        receiver: AstExprId,
        method: &Name,
        args: &[AstExprId],
        runtime_id: Option<AstExprId>,
        dest: &Place,
    ) -> bool {
        let dispatch_target = self
            .interface_dispatch_target_for_expr_member(receiver, method)
            .or_else(|| {
                self.tir_expr_type(self.expr_metadata_key(receiver))
                    .and_then(|ty| self.dispatch_target_for_concrete(ty, method))
            });
        let Some((iface_tn, iface_type_args, iface_assoc)) = dispatch_target else {
            return false;
        };
        let mut arg_ops = self.lower_call_arg_operands(expr_id, args);
        if arg_ops.is_empty() {
            return false;
        }
        let receiver_op = arg_ops.remove(0);
        let (decl_tn, decl_args, decl_assoc) =
            self.interface_view_declaring_method(&(iface_tn, iface_type_args, iface_assoc), method);
        self.emit_virtual_call_with_value_operands(
            receiver_op,
            &decl_tn,
            &decl_args,
            &decl_assoc,
            method,
            expr_id,
            arg_ops,
            runtime_id,
            dest,
        )
    }

    /// Emit an open-world [`Terminator::VirtualCall`] dispatching `method` of
    /// interface `iface_tn` on `recv_local`. The receiver is passed as the first
    /// value argument; the VM reads its runtime concrete type as `Self` and
    /// resolves the impl (coherence guarantees at most one). Always succeeds
    /// (returns `true`): the type checker has already proved the receiver
    /// implements the interface, so no compile-time candidate enumeration — and
    /// hence no closed-world fall-through — is needed.
    #[expect(clippy::too_many_arguments)]
    fn emit_virtual_call(
        &mut self,
        recv_local: Local,
        iface_tn: &TypeName,
        iface_type_args: &[Tir2Ty],
        iface_assoc: &[(Name, Tir2Ty)],
        method: &Name,
        expr_id: AstExprId,
        args: &[AstExprId],
        runtime_id: Option<AstExprId>,
        dest: &Place,
    ) -> bool {
        let arg_ops = self.lower_call_arg_operands(expr_id, args);
        self.emit_virtual_call_with_value_operands(
            Operand::Copy(Place::Local(recv_local)),
            iface_tn,
            iface_type_args,
            iface_assoc,
            method,
            expr_id,
            arg_ops,
            runtime_id,
            dest,
        )
    }

    #[expect(clippy::too_many_arguments)]
    fn emit_virtual_call_with_value_operands(
        &mut self,
        receiver: Operand,
        iface_tn: &TypeName,
        iface_type_args: &[Tir2Ty],
        iface_assoc: &[(Name, Tir2Ty)],
        method: &Name,
        expr_id: AstExprId,
        arg_ops: Vec<Operand>,
        runtime_id: Option<AstExprId>,
        dest: &Place,
    ) -> bool {
        let method_arg_count = self
            .interface_method_generic_count(iface_tn, method)
            .unwrap_or(0);
        let type_arg_ops = self.lower_call_type_args(expr_id, true, None);
        let method_type_arg_ops = if method_arg_count == 0 {
            Vec::new()
        } else {
            let skip = type_arg_ops.len().saturating_sub(method_arg_count);
            type_arg_ops[skip..].to_vec()
        };
        let ntypeargs = method_type_arg_ops.len();
        let mut all_args = Vec::with_capacity(ntypeargs + arg_ops.len() + 1);
        all_args.extend(method_type_arg_ops);
        all_args.push(receiver);
        all_args.extend(arg_ops);
        // Non-generic interfaces (`Equals`/`Compare`) carry empty args/assoc; a
        // parameterized interface bakes its arguments into the template. The
        // template reaches the VM through `LoadType` — which substitutes the
        // caller's frame type args — so an interface arg that is an enclosing
        // generic (`Lorem<T>` inside a generic fn) lowers to its `TypeArgRef`
        // slot and arrives at the resolver realized, disambiguating a type
        // implementing the same interface at several instantiations.
        let generic_params = self.enclosing_generic_params();
        let iface_template = tir2_interface_to_template(
            iface_tn,
            iface_type_args,
            iface_assoc,
            self.resolved_aliases,
            &generic_params,
        );
        let unwind = self.catch_context.as_ref().map(|c| c.unwind_target);
        let runtime_id_operand = self.lower_runtime_id_operand(runtime_id);
        let result_ty = self.expr_ty(expr_id);
        self.emit_virtual_call_with_operands(
            iface_template,
            method.as_str(),
            all_args,
            ntypeargs,
            self.call_requires_runtime_type_check(expr_id),
            runtime_id_operand,
            result_ty,
            unwind,
            dest.clone(),
        );
        true
    }

    /// Emit the `VirtualCall` terminator itself, given operands that are already
    /// lowered. Shared by the method-call funnel ([`Self::emit_virtual_call`],
    /// which builds its operands from an AST call) and by operator lowering,
    /// which has no call expression to read arguments from.
    ///
    /// `args` must be laid out as `[method_type_args… ++ receiver ++ value_args…]`
    /// with exactly `ntypeargs` leading type args, mirroring `Call`. **The
    /// receiver is `args[ntypeargs]`** — the VM reads its runtime concrete type
    /// as `Self` and resolves the impl off that, so passing the operands in the
    /// wrong order silently dispatches on the wrong value. `Self` is taken from
    /// the value, not the operand form, so a receiver may be any `Operand`
    /// (`emit_virtual_call` always passes a local; operator lowering may pass a
    /// constant, as in `true < false`).
    ///
    /// The call splits the block — the resolved impl may be user bytecode — so
    /// lowering resumes in a fresh one. `VirtualCall`'s destination must be a
    /// `Place::Local`; a projection (field/index) or capture is dispatched into
    /// a `result_ty`-typed temp and assigned through in the resume block,
    /// mirroring how `lower_call`/`lower_await` normalize their destinations.
    #[expect(clippy::too_many_arguments)]
    fn emit_virtual_call_with_operands(
        &mut self,
        iface: TyTemplateInterface,
        method: &str,
        args: Vec<Operand>,
        ntypeargs: usize,
        runtime_type_check: bool,
        runtime_id: Option<Operand>,
        result_ty: RuntimeTy,
        unwind: Option<BlockId>,
        dest: Place,
    ) {
        let resume = self.builder.create_block();
        let (call_dest, projection_dest) = match dest {
            Place::Local(_) => (dest, None),
            projection => {
                let tmp = self.builder.temp(result_ty);
                (Place::local(tmp), Some(projection))
            }
        };
        self.builder.virtual_call_with_runtime_type_check(
            iface,
            method.to_string(),
            args,
            ntypeargs,
            runtime_type_check,
            runtime_id,
            call_dest.clone(),
            resume,
            unwind,
        );
        self.builder.set_current_block(resume);
        if let Some(projection) = projection_dest {
            self.builder
                .assign(projection, Rvalue::Use(Operand::Copy(call_dest)));
        }
    }

    /// Emit an [`Rvalue::MakeVirtualBoundMethod`] binding `method` of the interface
    /// `view` on `recv_local` — the value analogue of [`Self::emit_virtual_call`].
    /// The declaring-interface narrowing and the runtime-converted interface
    /// template mirror the call form; the VM resolves the receiver's impl at bind
    /// time and produces a `BoundMethod` carrying the impl's realized frame.
    fn emit_virtual_bound_method(
        &mut self,
        recv_local: Local,
        view: &InterfaceTypeView,
        method: &Name,
        dest: &Place,
    ) {
        let (decl_tn, decl_args, decl_assoc) = self.interface_view_declaring_method(view, method);
        // Route through `tir2_to_template` like the call form: an interface arg
        // that is an enclosing generic lowers to its `TypeArgRef` frame slot and
        // arrives at the bind-time resolver realized.
        let generic_params = self.enclosing_generic_params();
        let iface_template = tir2_interface_to_template(
            &decl_tn,
            &decl_args,
            &decl_assoc,
            self.resolved_aliases,
            &generic_params,
        );
        self.builder.assign(
            dest.clone(),
            Rvalue::MakeVirtualBoundMethod {
                iface: iface_template,
                method: method.to_string(),
                receiver: Operand::Copy(Place::Local(recv_local)),
                // These value sites reference the method bare (`x.eq`); a
                // *specialized* generic-method value (`x.map<int>`) reaches MIR
                // as a generic-apply over the produced value, not here.
                type_args: Vec::new(),
            },
        );
    }

    /// Lower the receiver of a method-call path (`receiver_segments` — the path
    /// up to but excluding the method/qualifier) to a single local: a bare root
    /// local is used directly; a field chain is materialized into a temp. Shared
    /// by the interface- and union-receiver dispatch paths.
    fn lower_path_receiver_to_local(
        &mut self,
        callee: AstExprId,
        receiver_segments: &[Name],
        recv_root_local: Local,
    ) -> Local {
        if receiver_segments.len() <= 1 {
            return recv_root_local;
        }
        let recv_ty_idx = receiver_segments.len() - 1;
        let recv_ty = self
            .tir_path_segment_type((self.current_metadata_scope, callee, recv_ty_idx))
            .cloned()
            .map(|t| self.convert_tir_ty_for_runtime(&t))
            .unwrap_or_else(|| RuntimeTy::BuiltinUnknown {
                attr: TyAttr::default(),
            });
        let local = self.builder.temp(recv_ty);
        self.lower_multi_segment_path_as_field_chain(
            callee,
            receiver_segments,
            Place::local(local),
        );
        local
    }

    /// Resolve `class.method` to a callable `ItemRef` by simple name.
    fn class_method_item_ref_by_name(&self, class_tn: &TypeName, method: &Name) -> Option<ItemRef> {
        use baml_compiler2_ppir::item_data::{class_data, function_data};
        let class_loc = self.resolve_class_loc_by_type_name(class_tn)?;
        let func_loc = class_data(self.db, class_loc)
            .methods
            .iter()
            .copied()
            .find(|&fl| function_data(self.db, fl).name == *method)?;
        Some(method_item_ref(self.db, class_loc, func_loc))
    }

    /// A method call whose receiver is a *union of concrete classes* (e.g. the
    /// `Dog | Cat` produced by `if`/`match` arms) — dispatch by runtime class.
    /// Each member must declare `method`; otherwise this isn't a uniform call we
    /// can lower and we fall through (the caller reports the real error).
    fn try_lower_union_dispatch(
        &mut self,
        expr_id: AstExprId,
        base: AstExprId,
        method: &Name,
        args: &[AstExprId],
        runtime_id: Option<AstExprId>,
        dest: &Place,
    ) -> bool {
        let Some(members) = self
            .tir_expr_type(self.expr_metadata_key(base))
            .and_then(Self::tir_union_members)
        else {
            return false;
        };
        // Lower the receiver once; copy it into every arm.
        let receiver_op = self.lower_to_operand(base);
        let receiver_ty = self.expr_ty(base);
        let recv_local = self.operand_to_local(receiver_op, receiver_ty);
        self.emit_union_class_dispatch(
            recv_local,
            &members,
            method,
            DispatchCallLowering {
                expr_id,
                args,
                runtime_id,
                dest,
            },
        )
    }

    /// A method call whose receiver is a union that contains at least one
    /// *interface* member (e.g. `Animal | Vehicle`, where every member declares
    /// `method`). BEP-044: a method every union member provides through a shared
    /// interface dispatches open-world — the runtime value is one concrete member,
    /// so a virtual call keyed on the shared interface resolves its impl. Falls
    /// through (returns false) when the members share no providing interface, so
    /// the caller can report the real error.
    fn try_lower_union_iface_dispatch(
        &mut self,
        expr_id: AstExprId,
        base: AstExprId,
        method: &Name,
        args: &[AstExprId],
        runtime_id: Option<AstExprId>,
        dest: &Place,
    ) -> bool {
        let Some(members) = self
            .tir_expr_type(self.expr_metadata_key(base))
            .and_then(Self::tir_union_members)
        else {
            return false;
        };
        let Some((decl_tn, decl_args, decl_assoc)) =
            self.union_virtual_dispatch_view(&members, method)
        else {
            return false;
        };
        let receiver_op = self.lower_to_operand(base);
        let receiver_ty = self.expr_ty(base);
        let recv_local = self.operand_to_local(receiver_op, receiver_ty);
        self.emit_virtual_call(
            recv_local,
            &decl_tn,
            &decl_args,
            &decl_assoc,
            method,
            expr_id,
            args,
            runtime_id,
            dest,
        )
    }

    /// The declaring-interface view for a `method` call on a union receiver.
    /// Every member must provide the same realized interface; otherwise the
    /// caller must retain per-member dispatch (or report the checker's error).
    fn union_virtual_dispatch_view(
        &self,
        members: &[Tir2Ty],
        method: &Name,
    ) -> Option<InterfaceTypeView> {
        let declaring_view = |member: &Tir2Ty| {
            self.interface_dispatch_target_for_member(member, method)
                .or_else(|| self.dispatch_target_for_concrete(member, method))
                .map(|view| self.interface_view_declaring_method(&view, method))
        };
        let first = declaring_view(members.first()?)?;
        members
            .iter()
            .skip(1)
            .all(|member| declaring_view(member).as_ref() == Some(&first))
            .then_some(first)
    }

    /// Emit a class-tag dispatch switch for a method call whose receiver
    /// (`recv_local`) has the union type `members`. Returns false (lowering
    /// nothing) unless every member is a class declaring `method`.
    fn emit_union_class_dispatch(
        &mut self,
        recv_local: Local,
        members: &[Tir2Ty],
        method: &Name,
        call: DispatchCallLowering<'_>,
    ) -> bool {
        let mut arms: Vec<(TypeName, ItemRef)> = Vec::new();
        for member in members {
            let Tir2Ty::Class(qtn, _, _) = member else {
                return false;
            };
            let class_tn = qtn.clone();
            let Some(item_ref) = self.class_method_item_ref_by_name(&class_tn, method) else {
                return false;
            };
            arms.push((class_tn, item_ref));
        }
        if arms.is_empty() {
            return false;
        }

        let arg_ops = self.lower_call_arg_operands(call.expr_id, call.args);
        let runtime_id_operand = self.lower_runtime_id_operand(call.runtime_id);
        let unwind = self.catch_context.as_ref().map(|c| c.unwind_target);

        let bb_join = self.builder.create_block();
        let bb_otherwise = self.builder.create_block();
        let mut next_check = self.builder.current_block();
        for (idx, (class_tn, item_ref)) in arms.iter().enumerate() {
            let bb_body = self.builder.create_block();
            let bb_next = if idx + 1 == arms.len() {
                bb_otherwise
            } else {
                self.builder.create_block()
            };
            self.builder.set_current_block(next_check);
            self.emit_is_type_branch(
                recv_local,
                RuntimeTy::Class(class_tn.clone(), Vec::new(), TyAttr::default()),
                bb_body,
                bb_next,
            );
            self.builder.set_current_block(bb_body);
            let callee_op = Operand::Constant(Constant::Function(item_ref.clone()));
            let mut all_args = vec![Operand::Copy(Place::Local(recv_local))];
            all_args.extend(arg_ops.iter().cloned());
            self.builder.call_with_type_args_and_runtime_id(
                callee_op,
                all_args,
                0,
                runtime_id_operand.clone(),
                call.dest.clone(),
                bb_join,
                unwind,
            );
            next_check = bb_next;
        }
        self.builder.set_current_block(bb_otherwise);
        self.builder.unreachable();
        self.builder.set_current_block(bb_join);
        true
    }

    fn interface_method_generic_count(&self, iface_tn: &TypeName, method: &Name) -> Option<usize> {
        use baml_compiler2_ppir::item_data::{function_data, interface_data};
        if let Some(baml_compiler2_hir_ty::package_interface::ExportedType::Interface {
            required_methods,
            default_methods,
            ..
        }) = baml_compiler2_hir_ty::package_interface::mounted_type_row(self.db, iface_tn)
        {
            return required_methods
                .iter()
                .chain(default_methods)
                .find_map(|function| {
                    (function.name == *method).then_some(function.generic_params.len())
                });
        }
        let iface_pkg_name = iface_tn.package();
        let iface_pkg_items = self.resolve_class_pkg_items_by_name(iface_pkg_name);
        let iface_ns: Vec<Name> = iface_tn.namespace().clone();
        let Definition::Interface(iface_loc) =
            iface_pkg_items.lookup_type(&iface_ns, iface_tn.name())?
        else {
            return None;
        };
        let iface_data = interface_data(self.db, iface_loc);
        iface_data
            .default_methods
            .iter()
            .find_map(|&fn_loc| {
                let func = function_data(self.db, fn_loc);
                (func.name == *method).then_some(func.generic_params.len())
            })
            .or_else(|| {
                iface_data
                    .required_methods
                    .iter()
                    .find_map(|sig| (sig.name == *method).then_some(sig.generic_params.len()))
            })
    }

    /// Read `field` through an interface view whose receiver's concrete type is not
    /// known statically, as an open-world [`Rvalue::VirtualFieldAccess`].
    ///
    /// Returns `false` only when no interface in the view's `requires` closure
    /// declares `field` — a resolution failure the caller falls through on. It never
    /// enumerates implementors: an interface's implementor set is open, and with
    /// generic impls unbounded, so no compile-time candidate list could be complete.
    fn try_lower_interface_field_access(
        &mut self,
        recv_local: Local,
        iface_tn: &TypeName,
        iface_type_args: &[Tir2Ty],
        iface_assoc: &[(Name, Tir2Ty)],
        field: &Name,
        dest: &Place,
    ) -> bool {
        let view = (
            iface_tn.clone(),
            iface_type_args.to_vec(),
            iface_assoc.to_vec(),
        );
        let Some((iface, field_index)) = self.virtual_field_wire_target(&view, field) else {
            return false;
        };
        self.emit_virtual_field_access(recv_local, iface, field_index, field, dest);
        true
    }

    /// What a virtual access to `field` through `view` puts on the wire: `view` narrowed
    /// to the interface that *declares* `field`, lowered to the constraint template the
    /// instruction carries, paired with the field's index in that interface.
    ///
    /// `None` when no interface in the view's `requires` closure declares `field`. This
    /// is the only fallible step in a virtual field access, which is why both the read
    /// and the write path funnel through it — the write path calls it before lowering
    /// any operand so its fall-through stays side-effect-free.
    fn virtual_field_wire_target(
        &mut self,
        view: &InterfaceTypeView,
        field: &Name,
    ) -> Option<(TyTemplateInterface, u32)> {
        let ((decl_tn, decl_args, decl_assoc), field_index) =
            self.interface_view_declaring_field(view, field)?;
        let generic_params = self.enclosing_generic_params();
        Some((
            tir2_interface_to_template(
                &decl_tn,
                &decl_args,
                &decl_assoc,
                self.resolved_aliases,
                &generic_params,
            ),
            field_index,
        ))
    }

    /// Emit the open-world read of `field` off `recv_local` through `iface`.
    fn emit_virtual_field_access(
        &mut self,
        recv_local: Local,
        iface: TyTemplateInterface,
        field_index: u32,
        field: &Name,
        dest: &Place,
    ) {
        self.builder.assign(
            dest.clone(),
            Rvalue::VirtualFieldAccess {
                iface,
                receiver: Operand::Copy(Place::Local(recv_local)),
                field_index,
                field: field.clone(),
            },
        );
    }

    /// The interface view an assignment *target* `recv.field` resolves through, when
    /// the field is served by an interface rather than a slot on the receiver's own
    /// class.
    ///
    /// TIR records a resolution for field *reads*; an assignment target does not go
    /// through that path, so this falls back to deriving the view from the
    /// receiver's type — the same derivation the read path uses when TIR has nothing
    /// recorded. `None` means the target is an ordinary place and the caller should
    /// lower it as one.
    fn virtual_field_assign_view(
        &mut self,
        target: AstExprId,
        base: AstExprId,
        field: &Name,
    ) -> Option<InterfaceTypeView> {
        if let Some((view, _index)) = self.tir_virtual_field_view(self.expr_metadata_key(target)) {
            return Some(view);
        }
        let base_ty = self.expr_ty(base).strip_null();
        // `field` selects among a bounded type variable's bound conjunction, where
        // the field may be declared by any conjunct.
        self.interface_receiver_for_field_access(base, field, &base_ty)
    }

    /// The [`VirtualFieldTarget`] an assignment target denotes when it is an
    /// interface-field access, for either spelling: `a.b` reaches MIR as a
    /// multi-segment `Path` when its root is a local, and as a `MemberAccess` when the
    /// base is a general expression. `None` for an ordinary place.
    ///
    /// Every fallible step runs *before* any operand is lowered, so a `None` leaves no
    /// emitted code behind. That ordering is load-bearing: the caller falls through to
    /// `lower_lvalue` + `lower_expr`, which re-lower the target and the value from the
    /// AST, so a receiver materialized here would be a side-effecting expression
    /// evaluated twice plus a block of dead statements.
    fn virtual_field_assign_target(&mut self, target: AstExprId) -> Option<VirtualFieldTarget> {
        match &self.body.exprs[target] {
            AstExpr::MemberAccess { base, member } => {
                let (base, field) = (*base, member.clone());
                let view = self.virtual_field_assign_view(target, base, &field)?;
                let (iface, field_index) = self.virtual_field_wire_target(&view, &field)?;
                let recv_op = self.lower_to_operand(base);
                let receiver = self.operand_to_local(recv_op, self.expr_ty(base));
                Some(VirtualFieldTarget {
                    receiver,
                    field,
                    iface,
                    field_index,
                })
            }
            AstExpr::Path(segments) if segments.len() >= 2 => {
                let segments = segments.clone();
                let field = segments.last().expect("checked non-empty").clone();
                let prefix_idx = segments.len() - 2;
                let prefix_ty = self
                    .tir_path_segment_type((self.current_metadata_scope, target, prefix_idx))
                    .cloned()
                    .map(|t| self.convert_tir_ty_for_runtime(&t))?;
                let view = self
                    .interface_receiver_for_path_prefix(target, prefix_idx, &field, &prefix_ty)?;
                let (iface, field_index) = self.virtual_field_wire_target(&view, &field)?;
                let root_local = self.local_for_path(target, &segments[0])?;
                let receiver = self.lower_path_receiver_to_local(
                    target,
                    &segments[..segments.len() - 1],
                    root_local,
                );
                Some(VirtualFieldTarget {
                    receiver,
                    field,
                    iface,
                    field_index,
                })
            }
            _ => None,
        }
    }

    /// Lower `recv.field = value` where `recv.field` is an interface field, as a
    /// [`StatementKind::VirtualFieldStore`]. Returns `false` when the target is not
    /// a virtual interface-field access, leaving the ordinary `Place` path to it.
    ///
    /// This has to bypass `lower_lvalue` entirely: the destination slot depends on
    /// the receiver's impl, so there is no `Place` to build. Before this existed the
    /// target fell through to a dynamic map-key store and the VM rejected it with
    /// `expected Map, got Instance`.
    fn try_lower_virtual_field_assign(&mut self, target: AstExprId, value: AstExprId) -> bool {
        let Some(field_target) = self.virtual_field_assign_target(target) else {
            return false;
        };
        // Take the value as an operand rather than parking it in a temp: a temp
        // defined here is a single-use local the emitter may classify as inlinable
        // and drop the store for, while this statement still emits a plain load of
        // it — reading an uninitialized slot.
        let value_op = self.lower_to_operand(value);
        self.emit_interface_field_store(&field_target, value_op);
        true
    }

    /// `recv.field op= value` on an interface field: read through the virtual
    /// access, apply the operator, then write back through the virtual store.
    /// A `Place`-based read-modify-write is unavailable for the same reason as
    /// [`Self::try_lower_virtual_field_assign`].
    fn try_lower_virtual_field_assign_op(
        &mut self,
        target: AstExprId,
        op: AstAssignOp,
        value: AstExprId,
    ) -> bool {
        let Some(field_target) = self.virtual_field_assign_target(target) else {
            return false;
        };
        let current = self.builder.temp(self.expr_ty(target));
        self.emit_virtual_field_access(
            field_target.receiver,
            field_target.iface.clone(),
            field_target.field_index,
            &field_target.field,
            &Place::local(current),
        );
        // Apply the operator to the temp through the ordinary path, so union
        // operator drivers and overload dispatch behave exactly as they do for a
        // class field; only the read and the write-back are virtual.
        self.emit_assign_op(Place::local(current), target, op, value);
        self.emit_interface_field_store(&field_target, Operand::Copy(Place::Local(current)));
        true
    }

    /// Write `value` to `field` through an interface view — the store counterpart of
    /// [`Self::emit_virtual_field_access`].
    ///
    /// Infallible, like `emit_virtual_call`: the wire interface and field index were
    /// resolved by [`Self::virtual_field_assign_target`] before any operand was
    /// lowered, so nothing is left to fail on and there is no fall-through to leave
    /// half-emitted.
    fn emit_interface_field_store(&mut self, target: &VirtualFieldTarget, value: Operand) {
        self.builder.virtual_field_store(
            target.iface.clone(),
            Operand::Copy(Place::Local(target.receiver)),
            target.field_index,
            target.field.clone(),
            value,
        );
    }

    /// Reduce every determinable associated-type projection in `ty` to a fixpoint, via the
    /// canonical `normalize` (the checker's own reduction — no parallel resolver).
    fn resolve_ty_projections(&self, ty: &Tir2Ty) -> Tir2Ty {
        baml_type::normalize::normalize(ty, &self.hir_facts())
    }

    /// The `hir_ty` facts oracle over this context's carried bounds - the
    /// ONE alias/projection/subtype authority (aliases resolve through
    /// definitions directly; no precomputed map).
    fn hir_facts(&self) -> baml_compiler2_hir_ty::facts::Facts<'db> {
        baml_compiler2_hir_ty::facts::Facts::with_bounds(self.db, self.generic_param_bounds.clone())
    }

    /// The declared bounds of an *unreduced* (symbolic-base) projection
    /// `(base as I).member` — interface `I`'s `type member extends J & K`, realized.
    ///
    /// A conjunction, like a generic parameter's: empty for a non-projection, an
    /// unqualified projection, or an unbounded associated type.
    fn resolve_projection_bounds(&self, ty: &Tir2Ty) -> Vec<Tir2Ty> {
        use baml_type::normalize::TypeContext;
        let Tir2Ty::AssociatedTypeProjection {
            interface: iface,
            member,
            ..
        } = ty
        else {
            return Vec::new();
        };
        self.hir_facts()
            .associated_type_bound(iface, member.clone())
            .iter()
            .map(baml_type::Interface::to_ty)
            .collect()
    }

    fn interface_closure_type_name_views(
        &self,
        iface_tn: &TypeName,
        iface_type_args: &[Tir2Ty],
        iface_assoc: &[(Name, Tir2Ty)],
    ) -> Option<Vec<InterfaceTypeView>> {
        let iface_ns: Vec<Name> = iface_tn.namespace().clone();
        if !self.is_interface_type_name(iface_tn) {
            return None;
        }
        // The root view is the request itself, verbatim; only the
        // `requires` EXPANSION goes through hir_ty's realized closure.
        // A TIR-internal sentinel in an argument (`Unknown`/`Evolving`,
        // dual-provider only) cannot intern - it degrades to the error
        // sentinel FOR THE WALK, while the root view keeps the plain
        // originals.
        let interned = |ty: &Tir2Ty| {
            baml_compiler2_hir_ty::impls::try_interned_ty(ty)
                .unwrap_or_else(baml_type::interned::Ty::error)
        };
        let root = baml_type::interned::InterfaceRef::new(
            baml_type::TypeName::new(
                iface_tn.package().clone(),
                iface_ns,
                iface_tn.name().clone(),
            ),
            iface_type_args
                .iter()
                .map(interned)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            iface_assoc
                .iter()
                .map(|(name, ty)| (name.clone(), interned(ty)))
                .collect(),
        );
        let subject = root.existential();
        let mut views: Vec<InterfaceTypeView> = vec![(
            iface_tn.clone(),
            iface_type_args.to_vec(),
            iface_assoc.to_vec(),
        )];
        views.extend(
            baml_compiler2_hir_ty::impls::direct_requires_closure(self.db, &root, &subject, 8)
                .into_iter()
                .map(|reference| {
                    // A required interface's pins realize as PROJECTIONS on
                    // the subject (`(subject as Iterator).Error`); the
                    // oracle reduces them here - runtime dispatch types
                    // carry the reduced members, exactly as TIR's eager
                    // substitution emitted them.
                    let reduce = |ty: &baml_type::interned::Ty| -> Tir2Ty {
                        self.resolve_ty_projections(&ty.to_plain())
                    };
                    (
                        reference.name.clone(),
                        reference.generics.iter().map(reduce).collect(),
                        reference
                            .associated_types
                            .iter()
                            .map(|(name, ty)| (name.clone(), reduce(ty)))
                            .collect(),
                    )
                }),
        );
        Some(views)
    }

    fn resolve_class_loc_by_type_name(
        &self,
        class_tn: &TypeName,
    ) -> Option<baml_compiler2_hir::loc::ClassLoc<'db>> {
        let pkg_name = class_tn.package();
        let pkg_items = self.resolve_class_pkg_items_by_name(pkg_name);
        let ns: Vec<Name> = class_tn.namespace().clone();
        let Some(Definition::Class(class_loc)) = pkg_items.lookup_type(&ns, class_tn.name()) else {
            return None;
        };
        Some(class_loc)
    }

    fn resolve_implements_target_view(
        &self,
        target: baml_compiler2_hir::type_ref::TypeRefId,
        associated_type_bindings: &[baml_compiler2_ppir::item_data::AssociatedTypeBindingData],
        class_loc: baml_compiler2_hir::loc::ClassLoc<'db>,
    ) -> Option<InterfaceTypeView> {
        let class_file = class_loc.file(self.db);
        let class_pkg = baml_compiler2_hir::file_package::file_package(self.db, class_file);
        let class_pkg_id = PackageId::new(self.db, class_pkg.package.clone());
        let class_pkg_items = package_items(self.db, class_pkg_id);
        let class_data = baml_compiler2_ppir::item_data::class_data(self.db, class_loc);
        // `target` and the class-side `associated_type_bindings` index the class's
        // own arena; the interface's associated-type defaults index the interface's.
        let target_store = &class_data.type_refs;
        let target_loc = resolve_ref_to_interface_loc(
            self.db,
            target_store,
            target,
            class_pkg_items,
            &class_pkg.namespace_path,
        )?;
        let target_data = baml_compiler2_ppir::item_data::interface_data(self.db, target_loc);
        let class_generic_params =
            baml_compiler2_hir_ty::lower::class_generic_frame(self.db, class_loc);
        let target_generic_params =
            baml_compiler2_hir_ty::lower::interface_declared_params(self.db, target_loc);
        let target_frame_params =
            baml_compiler2_hir_ty::lower::interface_frame(self.db, target_loc);
        let target_qtn = qualify_def(
            self.db,
            Definition::Interface(target_loc),
            &target_data.name,
        );
        let target_args = lower_ref_interface_target_args(
            self.db,
            target_store,
            target,
            class_pkg_items,
            &class_pkg.namespace_path,
            &class_generic_params,
            &baml_compiler2_hir_ty::lower::class_generic_bounds(self.db, class_loc),
        );
        let target_iface_pkg =
            baml_compiler2_hir::file_package::file_package(self.db, target_loc.file(self.db));
        let mut bindings = baml_type_runtime::bind_type_vars(&target_generic_params, &target_args);
        for param in &class_generic_params {
            bindings
                .entry(param.clone())
                .or_insert_with(|| Tir2Ty::TypeVar(param.clone(), baml_type::TyAttr::default()));
        }
        let associated_bindings = target_data
            .associated_types
            .iter()
            .filter_map(|assoc| {
                if let Some(binding) = associated_type_bindings
                    .iter()
                    .find(|binding| binding.name == assoc.name)
                    && let Some(type_ref) = binding.type_ref
                {
                    let ty = lower_ref_with_bindings(
                        self.db,
                        target_store,
                        type_ref,
                        class_pkg_items,
                        &class_pkg.namespace_path,
                        &bindings,
                        &baml_compiler2_hir_ty::lower::class_generic_bounds(self.db, class_loc),
                    );
                    let assoc_param = target_frame_params
                        .iter()
                        .find(|param| param.name() == &assoc.name)
                        .expect("associated type is in the interface environment");
                    bindings.insert(assoc_param.clone(), ty.clone());
                    return Some((assoc.name.clone(), ty));
                }
                assoc.default.map(|default| {
                    let ty = lower_ref_with_bindings(
                        self.db,
                        &target_data.type_refs,
                        default,
                        class_pkg_items,
                        &target_iface_pkg.namespace_path,
                        &bindings,
                        &baml_compiler2_hir_ty::lower::class_generic_bounds(self.db, class_loc),
                    );
                    let assoc_param = target_frame_params
                        .iter()
                        .find(|param| param.name() == &assoc.name)
                        .expect("associated type is in the interface environment");
                    bindings.insert(assoc_param.clone(), ty.clone());
                    (assoc.name.clone(), ty)
                })
            })
            .collect();
        Some((target_qtn, target_args, associated_bindings))
    }

    /// BEP-044: when the enclosing function is the override declared
    /// inside an `implements I { ... }` block, return `I`'s target type
    /// expression. `None` for free functions, top-level class methods,
    /// and interface default-method bodies.
    fn implements_block_iface_target(
        &self,
    ) -> Option<&'db baml_compiler2_ppir::item_data::MethodInterfaceTarget> {
        let func_loc = self.func_loc?;
        baml_compiler2_ppir::item_data::method_interface_target(self.db, func_loc).as_ref()
    }

    /// The enclosing implements-block's subject type — what `Self` denotes in
    /// this impl method's body: the class at its own generic params for an
    /// in-body block (structural for the builtin containers, matching TIR's
    /// receiver typing), or the free impl's `for` pattern lowered over the
    /// impl's generics. `None` when the enclosing function is not an impl
    /// method.
    fn implements_subject_tir_ty(&self) -> Option<Tir2Ty> {
        use baml_compiler2_ppir::item_data::{
            ImplSubjectData, MethodOwner, impl_block_data, method_owner,
        };
        let fl = self.func_loc?;
        match method_owner(self.db, fl)? {
            MethodOwner::Class(class_loc) => {
                // The declared receiver at the class's own frame, through
                // the builtin-container bridge (`Array` self IS `T[]`).
                Some(baml_compiler2_hir_ty::lower::class_self_ty(self.db, class_loc).to_plain())
            }
            MethodOwner::FreeImpl(impl_loc) => {
                let block = impl_block_data(self.db, impl_loc);
                let ImplSubjectData::Free { for_target, .. } = &block.subject else {
                    // A `FreeImpl` owner is recorded only for out-of-body
                    // blocks, whose subject is always `Free`.
                    return None;
                };
                let pkg_info = file_package(self.db, self.file);
                let pkg_id = PackageId::new(self.db, pkg_info.package.clone());
                let pkg_items = package_items(self.db, pkg_id);
                let generic_params = self.enclosing_generic_params();
                let bounds = self.enclosing_generic_param_bounds();
                Some(lower_ref_in_scope(
                    self.db,
                    &block.type_refs,
                    *for_target,
                    pkg_items,
                    &pkg_info.namespace_path,
                    &generic_params,
                    &bounds,
                    None,
                ))
            }
            // Interface default methods have no implements-block target
            // (`method_interface_target` is `None` for them), so callers
            // gated on that never reach this arm with an `Interface` owner.
            MethodOwner::Interface(_) => None,
        }
    }

    /// What a *body-position* `Self` denotes in the function being lowered —
    /// the rigid `Self` type variable for an interface-owned body (carried at
    /// frame slot 0), or the enclosing implements-block's subject, statically
    /// substituted. Mirrors TIR's `body_self_ty` (`inference.rs`), so MIR's
    /// re-lowering of written body type expressions (explicit type args,
    /// pattern annotations) resolves `Self` to the same type TIR checked.
    /// `None` for plain class methods and free functions, where a body
    /// `Self` is an unresolved name TIR already diagnosed.
    fn body_self_tir_ty(&self) -> Option<Tir2Ty> {
        use baml_compiler2_ppir::item_data::{MethodOwner, method_interface_target, method_owner};
        let fl = self.func_loc?;
        match method_owner(self.db, fl)? {
            MethodOwner::Interface(iface_loc) => {
                let self_param = baml_compiler2_hir_ty::lower::interface_frame(self.db, iface_loc)
                    .into_iter()
                    .next()
                    .expect("interface frame starts with Self");
                Some(Tir2Ty::TypeVar(self_param, baml_type::TyAttr::default()))
            }
            MethodOwner::Class(_) => method_interface_target(self.db, fl)
                .is_some()
                .then(|| self.implements_subject_tir_ty())
                .flatten(),
            MethodOwner::FreeImpl(_) => self.implements_subject_tir_ty(),
        }
    }

    fn resolve_class_pkg_items_by_name(
        &self,
        pkg_name: &Name,
    ) -> &'db baml_compiler2_hir::package::PackageItems<'db> {
        let pkg_id = PackageId::new(self.db, pkg_name.clone());
        package_items(self.db, pkg_id)
    }
}

// ─── Statement lowering ───────────────────────────────────────────────────────

impl LoweringContext<'_> {
    fn lower_stmt(&mut self, stmt_id: AstStmtId) {
        let prev_span = self.builder.current_source_span;
        if let Some(span) = self.span_for_stmt(stmt_id) {
            self.builder.current_source_span = Some(span);
        }

        let stmt = self.body.stmts[stmt_id].clone();
        match stmt {
            AstStmt::Expr(expr_id) => {
                let ty = self.expr_ty(expr_id);
                let temp = self.builder.temp(ty);
                self.lower_expr(expr_id, Place::local(temp));
            }

            AstStmt::TypeBinding { name, value } => {
                let binding = self
                    .tir_type_binding(stmt_id)
                    .cloned()
                    .expect("a typed TypeBinding statement has a durable binding plan");
                debug_assert_eq!(binding.name, name);
                debug_assert_eq!(binding.operand, value);
                let value = self.lower_to_operand(binding.operand);
                self.runtime_type_binding_params
                    .push(binding.parameter.clone());
                let slot = RuntimeGenericLayout::new(&self.enclosing_generic_params())
                    .slot(&binding.parameter)
                    .expect("a just-bound runtime type parameter has a frame slot");
                self.builder.push_statement(
                    StatementKind::Intrinsic {
                        op: IntrinsicOp::BindType(slot as usize),
                        args: vec![value],
                    },
                    self.builder.current_source_span,
                );
            }

            // `let PATTERN = init else { … };` — refutable binding lowered
            // as a two-way pattern test. On match: bind into the current
            // scope (locals survive past the statement); on miss: lower the
            // else expression (guaranteed `RuntimeTy::Never` by TIR, so no
            // successor edge is needed). Handled before the structural
            // arms below because a refutable destructure ends up here too.
            AstStmt::Let {
                pattern,
                initializer,
                else_branch: Some(else_expr),
                ..
            } => {
                // Materialize the scrutinee into a local once. The
                // scrutinee carries the BROAD initializer type (e.g.
                // `int | string`), not the pattern's narrowed match type —
                // narrowing only kicks in on the match arm, after the
                // refutable test.
                let scrutinee_ty = initializer
                    .map(|init| self.expr_ty(init))
                    .unwrap_or_else(|| self.pat_ty(pattern));
                let scrutinee = self.builder.temp(scrutinee_ty);
                if let Some(init) = initializer {
                    self.lower_expr(init, Place::local(scrutinee));
                } else {
                    self.builder.assign(
                        Place::local(scrutinee),
                        Rvalue::Use(Operand::Constant(Constant::Null)),
                    );
                }

                let bb_match = self.builder.create_block();
                let bb_fail = self.builder.create_block();
                self.lower_pattern_test(scrutinee, pattern, bb_match, bb_fail);

                // Fail path: lower the else expression. TIR enforced that
                // the else has type `!`, so this block has no successor and
                // we don't emit a join edge. Use a throwaway temp as dest
                // because diverging expressions don't write through it.
                self.builder.set_current_block(bb_fail);
                let else_ty = self.expr_ty(else_expr);
                let else_dest = self.builder.temp(else_ty);
                self.lower_expr(else_expr, Place::local(else_dest));
                // If for any reason lowering produced a fall-through (e.g.
                // recovery state with an Unknown-typed else), terminate the
                // block with an unreachable so the CFG stays valid.
                if !self.builder.is_current_terminated() {
                    self.builder.unreachable();
                }

                // Match path: bind pattern names into the enclosing scope.
                // No saved/restored locals — these flow forward like a
                // plain `let`.
                self.builder.set_current_block(bb_match);
                self.bind_pattern_inner(scrutinee, pattern, pattern, pattern, false);

                let names: Vec<Name> = self.body.patterns[pattern]
                    .bound_names(&self.body.patterns)
                    .into_iter()
                    .cloned()
                    .collect();
                for name in names {
                    if let Some(&local) = self.locals.get(&name)
                        && let Some(binding_id) =
                            self.binding_id_for_statement_name(stmt_id, pattern, &name)
                    {
                        self.binding_locals.insert(binding_id, local);
                    }
                }
            }

            AstStmt::Let {
                pattern,
                initializer,
                ..
            } if self.pattern_contains_structural(pattern) => {
                let local_ty = self.pat_ty(pattern);
                let scrutinee = self.builder.temp(local_ty);

                if let Some(init) = initializer {
                    self.lower_expr(init, Place::local(scrutinee));
                } else {
                    self.builder.assign(
                        Place::local(scrutinee),
                        Rvalue::Use(Operand::Constant(Constant::Null)),
                    );
                }

                self.bind_pattern_inner(scrutinee, pattern, pattern, pattern, false);

                let names: Vec<Name> = self.body.patterns[pattern]
                    .bound_names(&self.body.patterns)
                    .into_iter()
                    .cloned()
                    .collect();
                for name in names {
                    if let Some(&local) = self.locals.get(&name)
                        && let Some(binding_id) =
                            self.binding_id_for_statement_name(stmt_id, pattern, &name)
                    {
                        self.binding_locals.insert(binding_id, local);
                    }
                }
            }

            AstStmt::Let {
                pattern,
                initializer,
                ..
            } => {
                // Extract binding names from pattern. A simple `let x` has
                // one name; a chain `let x: let y: let z` has three. The
                // first name owns the declared slot (the init writes into
                // it directly); remaining names alias via copy-assignment.
                let pat = self.body.patterns[pattern].clone();
                let names: Vec<Name> = pat
                    .bound_names(&self.body.patterns)
                    .into_iter()
                    .cloned()
                    .collect();
                let first_name = names.first().cloned();

                let local_ty = self.pat_ty(pattern);
                let local = self
                    .builder
                    .declare_local(first_name.clone(), local_ty.clone(), None);

                if let Some(init) = initializer {
                    self.lower_expr(init, Place::local(local));
                } else {
                    self.builder.assign(
                        Place::local(local),
                        Rvalue::Use(Operand::Constant(Constant::Null)),
                    );
                }

                if let Some(first_name) = first_name {
                    if let Some(binding_id) =
                        self.binding_id_for_statement_name(stmt_id, pattern, &first_name)
                    {
                        self.binding_locals.insert(binding_id, local);
                    }
                    self.locals.insert(first_name, local);
                }

                // Additional chain-link bindings get their own locals that
                // copy from the first. `let x: let y` ⇒ y = x at runtime.
                for extra in names.iter().skip(1) {
                    let alias =
                        self.builder
                            .declare_local(Some(extra.clone()), local_ty.clone(), None);
                    self.builder.assign(
                        Place::local(alias),
                        Rvalue::Use(Operand::Copy(Place::Local(local))),
                    );
                    if let Some(binding_id) =
                        self.binding_id_for_statement_name(stmt_id, pattern, extra)
                    {
                        self.binding_locals.insert(binding_id, alias);
                    }
                    self.locals.insert(extra.clone(), alias);
                }
            }

            AstStmt::While {
                condition,
                body,
                after,
                ..
            } => {
                let bb_cond = self.builder.create_block();
                let bb_body = self.builder.create_block();
                let bb_after = if after.is_some() {
                    self.builder.create_block()
                } else {
                    bb_cond
                };
                let bb_exit = self.builder.create_block();

                let prev_loop = self.loop_context.take();
                self.loop_context = Some(LoopContext {
                    break_target: bb_exit,
                    continue_target: bb_after,
                    defer_depth: self.defer_stack.len(),
                });

                if !self.builder.is_current_terminated() {
                    self.builder.goto(bb_cond);
                }

                self.builder.set_current_block(bb_cond);
                let cond_op = self.lower_to_operand(condition);
                self.builder.branch(cond_op, bb_body, bb_exit);

                self.builder.set_current_block(bb_body);
                let body_ty = self.expr_ty(body);
                let body_temp = self.builder.temp(body_ty);
                self.lower_expr(body, Place::local(body_temp));

                if !self.builder.is_current_terminated() {
                    self.builder.goto(bb_after);
                }

                if after.is_some() {
                    self.builder.set_current_block(bb_after);
                }
                if let Some(after_stmt) = after {
                    self.lower_stmt(after_stmt);
                }

                if !self.builder.is_current_terminated() {
                    self.builder.goto(bb_cond);
                }

                self.loop_context = prev_loop;
                self.builder.set_current_block(bb_exit);
            }

            // `while let PATTERN = SCRUTINEE { BODY }` — a loop header that
            // re-evaluates the scrutinee and re-tests the refutable pattern each
            // iteration. A structural cross of `AstStmt::While` (loop scaffold +
            // LoopContext + back-edge) and `lower_if_let` (refutable
            // `lower_pattern_test` + scoped fresh-cell binding). On match: bind
            // + run body + jump back to the header. On miss: exit the loop.
            AstStmt::WhileLet {
                pattern,
                scrutinee,
                body,
            } => {
                let bb_header = self.builder.create_block();
                let bb_body = self.builder.create_block();
                let bb_exit = self.builder.create_block();

                // `continue` re-enters the header (re-evaluates scrutinee +
                // re-tests the pattern); `break` jumps to the exit. Save/swap/
                // restore loop_context so nested loops work — mirrors While.
                let prev_loop = self.loop_context.take();
                self.loop_context = Some(LoopContext {
                    break_target: bb_exit,
                    continue_target: bb_header,
                    defer_depth: self.defer_stack.len(),
                });

                if !self.builder.is_current_terminated() {
                    self.builder.goto(bb_header);
                }

                // Header: resolve the scrutinee to a local, then run the
                // refutable test (match -> body, miss -> exit). For a bare-path
                // scrutinee this resolves to its OWN local, so a body that
                // mutates that local is observed when the header re-tests it on
                // the next pass (re-evaluation without a copy); other expressions
                // are re-lowered into a fresh local each pass. Mirrors
                // `lower_if_let`'s scrutinee handling exactly.
                self.builder.set_current_block(bb_header);
                let scrutinee_local = self.try_resolve_to_local(scrutinee).unwrap_or_else(|| {
                    let op = self.lower_to_operand(scrutinee);
                    let ty = self.expr_ty(scrutinee);
                    self.operand_to_local(op, ty)
                });
                self.lower_pattern_test(scrutinee_local, pattern, bb_body, bb_exit);

                // Body: bind pattern locals (scoped to the body, re-bound per
                // iteration via fresh cells so a closure created each pass
                // captures a distinct cell), record binding_locals for
                // go-to-definition, lower the body (result discarded), then jump
                // back to the header.
                self.builder.set_current_block(bb_body);
                let saved_locals = self.locals.clone();
                self.bind_pattern_with_fresh_cells(scrutinee_local, pattern);
                let names: Vec<Name> = self.body.patterns[pattern]
                    .bound_names(&self.body.patterns)
                    .into_iter()
                    .cloned()
                    .collect();
                for name in names {
                    if let Some(&local) = self.locals.get(&name)
                        && let Some(binding_id) =
                            self.binding_id_for_statement_name(stmt_id, pattern, &name)
                    {
                        self.binding_locals.insert(binding_id, local);
                    }
                }
                let body_temp = self.builder.temp(RuntimeTy::Void {
                    attr: TyAttr::default(),
                });
                self.lower_expr(body, Place::local(body_temp));
                if !self.builder.is_current_terminated() {
                    self.builder.goto(bb_header);
                }
                self.restore_locals_after_scope(saved_locals);

                // Exit.
                self.loop_context = prev_loop;
                self.builder.set_current_block(bb_exit);
            }

            // For loops use the Iterable interface: evaluate the collection,
            // call iter(), then repeatedly call next() until Done.
            AstStmt::For {
                binding,
                collection,
                body,
            } => {
                let coll_tir_ty = self
                    .tir_expr_type(self.expr_metadata_key(collection))
                    .cloned();
                let iterable_view = coll_tir_ty
                    .as_ref()
                    .and_then(|ty| self.iterable_view_for_tir_ty(ty));

                if let Some(iterable_view) = iterable_view {
                    self.lower_iterable_for_loop(stmt_id, binding, collection, body, iterable_view);
                } else {
                    self.emit_panic_call("for loop collection is not iterable", collection);
                }
            }

            AstStmt::Return(expr) => {
                let ret = Local(0); // _0 is always the return place
                if let Some(e) = expr {
                    self.lower_expr(e, Place::local(ret));
                }
                // Run all pending defers (LIFO).
                self.replay_defers_to_depth(0);
                self.builder.goto(self.exit_block);
                // Create a dead successor block for the builder cursor
                // (subsequent statements in the same block-list are dead code)
                let dead = self.builder.create_block();
                self.builder.set_current_block(dead);
                // Dead block is unterminated — subsequent stmts are lowered as
                // dead code (matching AstStmt::Throw behavior at lower.rs:1653-1658).
                // Phase 1 eliminates unreachable blocks.
            }

            AstStmt::Throw { value } => {
                let val_op = self.lower_throw_operand(value);
                // Defers run via the block's unwind landing pads: the throw's PC is inside the
                // enclosing defer region(s), so the exception table routes it to
                // the innermost defer pad (BEP-042 Stage 2). We do NOT inline-
                // replay here — that would double-run the defers.
                if self.operand_is_marked_rethrow(&val_op) {
                    self.builder.rethrow(val_op);
                } else {
                    self.builder.throw(val_op);
                }
                let dead = self.builder.create_block();
                self.builder.set_current_block(dead);
            }

            AstStmt::Break => {
                if let Some(ref loop_ctx) = self.loop_context {
                    let target = loop_ctx.break_target;
                    let defer_depth = loop_ctx.defer_depth;
                    // Run defers declared in the loop body.
                    self.replay_defers_to_depth(defer_depth);
                    self.builder.goto(target);
                }
                let dead = self.builder.create_block();
                self.builder.set_current_block(dead);
            }

            AstStmt::Continue => {
                if let Some(ref loop_ctx) = self.loop_context {
                    let target = loop_ctx.continue_target;
                    let defer_depth = loop_ctx.defer_depth;
                    // Run defers declared in the loop body.
                    self.replay_defers_to_depth(defer_depth);
                    self.builder.goto(target);
                }
                let dead = self.builder.create_block();
                self.builder.set_current_block(dead);
            }

            AstStmt::Defer { body } => {
                // BEP-042: register the defer body. It emits NO code here; it is
                // replayed (re-lowered inline, LIFO) at every exit of the
                // enclosing scope by `replay_defers_to_depth`, and popped when
                // the enclosing `lower_scoped_block` truncates `defer_stack`.
                self.defer_stack.push(body);
            }

            AstStmt::Assign { target, value } => {
                let target_expr = &self.body.exprs[target];
                if Self::is_runtime_id_path(target_expr) {
                    self.lower_set_runtime_id(value);
                } else if let AstExpr::OptionalChain { expr: inner } = target_expr {
                    let inner = *inner;
                    self.lower_assign_optional_chain(inner, value);
                } else if self.try_lower_virtual_field_assign(target, value) {
                    // Handled: the destination slot is only known once the
                    // receiver's impl is resolved, so there is no `Place` to
                    // assign through.
                } else {
                    let place = self.lower_lvalue(target);
                    self.lower_expr(value, place);
                }
            }

            AstStmt::AssignOp { target, op, value } => {
                let target_expr = &self.body.exprs[target];
                if let AstExpr::OptionalChain { expr: inner } = target_expr {
                    let inner = *inner;
                    self.lower_assign_op_optional_chain(inner, op, value);
                } else if self.try_lower_virtual_field_assign_op(target, op, value) {
                    // Handled — see `AstStmt::Assign`.
                } else {
                    let place = self.lower_lvalue(target);
                    self.emit_assign_op(place, target, op, value);
                }
            }

            AstStmt::Missing => {
                let callee = Operand::Constant(Constant::Function(ItemRef::Free {
                    package: Name::new("baml"),
                    namespace: vec![Name::new("sys")],
                    name: Name::new("panic"),
                }));
                let msg = Operand::Constant(Constant::String("missing statement".to_string()));
                let temp = self.builder.temp(RuntimeTy::Null {
                    attr: TyAttr::default(),
                });
                let unreachable_block = self.builder.create_block();
                self.builder.call(
                    callee,
                    vec![msg],
                    Place::local(temp),
                    unreachable_block,
                    None,
                );
                self.builder.set_current_block(unreachable_block);
                self.builder.unreachable();
                let dead = self.builder.create_block();
                self.builder.set_current_block(dead);
            }

            AstStmt::HeaderComment { .. } => {}
        }

        self.builder.current_source_span = prev_span;
    }

    fn convert_assign_op(op: AstAssignOp) -> BinOp {
        match op {
            AstAssignOp::Add => BinOp::Add,
            AstAssignOp::Sub => BinOp::Sub,
            AstAssignOp::Mul => BinOp::Mul,
            AstAssignOp::Div => BinOp::Div,
            AstAssignOp::Mod => BinOp::Mod,
            AstAssignOp::BitAnd => BinOp::BitAnd,
            AstAssignOp::BitOr => BinOp::BitOr,
            AstAssignOp::BitXor => BinOp::BitXor,
            AstAssignOp::Shl => BinOp::Shl,
            AstAssignOp::Shr => BinOp::Shr,
        }
    }

    fn lower_lvalue(&mut self, expr_id: AstExprId) -> Place {
        let expr = self.body.exprs[expr_id].clone();
        match &expr {
            AstExpr::Path(segments) if segments.len() == 1 => {
                if let Some(place) = self.place_for_path(expr_id, &segments[0]) {
                    place
                } else {
                    // Unresolved single-segment assignment target. This is
                    // only reachable for programs TIR already rejected (an
                    // unresolved name, or a special form like `$id` in a
                    // position its TIR checks forbid). Fail loudly at runtime
                    // instead of silently writing into a throwaway temp —
                    // a silent temp here is how `$id = ...` once compiled to
                    // a no-op (MIR has no compile-diagnostic channel).
                    self.emit_panic_call(
                        &format!(
                            "internal compiler error: MIR failed to resolve assignment \
                             target `{}` (TIR should have rejected this program)",
                            segments[0]
                        ),
                        expr_id,
                    );
                    let temp = self.builder.temp(RuntimeTy::Null {
                        attr: TyAttr::default(),
                    });
                    Place::Local(temp)
                }
            }
            AstExpr::Path(segments) if segments.len() >= 2 => {
                // Multi-segment path lvalue: `a.b` or `a.b.c`.
                // Chain field projections from the root local or capture.
                let (mut current_place, mut current_ty) =
                    if let Some(place) = self.place_for_path(expr_id, &segments[0]) {
                        let ty = match place {
                            Place::Local(local) => self
                                .path_root_ty(expr_id)
                                .unwrap_or_else(|| self.builder.local_ty(local)),
                            Place::Capture(_) => self.path_root_ty(expr_id).unwrap_or_else(|| {
                                RuntimeTy::BuiltinUnknown {
                                    attr: TyAttr::default(),
                                }
                            }),
                            _ => unreachable!("path roots are locals or captures"),
                        };
                        (place, ty)
                    } else {
                        let tmp = self.builder.temp(RuntimeTy::Null {
                            attr: TyAttr::default(),
                        });
                        (
                            Place::Local(tmp),
                            RuntimeTy::Null {
                                attr: TyAttr::default(),
                            },
                        )
                    };

                for (offset, seg) in segments[1..].iter().enumerate() {
                    let seg_idx = offset + 1;
                    if let Some((tn, class_type_args)) =
                        self.class_receiver_for_path_prefix(expr_id, seg_idx - 1, &current_ty)
                    {
                        if let Some(fields) = self.class_fields.get(&tn) {
                            if let Some(&idx) = fields.get(seg.as_str()) {
                                let next_ty = self.class_field_ty(&tn, seg, &class_type_args);
                                current_place = Place::Field {
                                    base: Box::new(current_place),
                                    field: idx,
                                };
                                current_ty = next_ty;
                                continue;
                            }
                        }
                    }
                    // Dynamic map fallback for non-class base or unknown field
                    let key_local = self.builder.temp(RuntimeTy::String {
                        attr: TyAttr::default(),
                    });
                    self.builder.assign(
                        Place::local(key_local),
                        Rvalue::Use(Operand::Constant(Constant::String(seg.to_string()))),
                    );
                    current_place = Place::Index {
                        base: Box::new(current_place),
                        index: key_local,
                        kind: IndexKind::Map,
                    };
                    break;
                }
                current_place
            }
            AstExpr::MemberAccess { base, member } => {
                let base_id = *base;
                let member_name = member.clone();
                let base_place = self.lower_lvalue(base_id);
                let base_ty = self.expr_ty(base_id);
                if let RuntimeTy::Class(ref tn, _, _) = base_ty {
                    if let Some(fields) = self.class_fields.get(tn) {
                        if let Some(&idx) = fields.get(member_name.as_str()) {
                            return Place::Field {
                                base: Box::new(base_place),
                                field: idx,
                            };
                        }
                    }
                    self.emit_panic_call(
                        &format!(
                            "internal compiler error: MIR failed to resolve member access \
                             .{} against class definition '{}' (module_path: {:?}). \
                             This class should be in class_fields but isn't.",
                            member_name,
                            tn.name(),
                            tn.module_path(),
                        ),
                        base_id,
                    );
                    // Dead code after panic — return a dummy place
                    let dead = self.builder.temp(RuntimeTy::Null {
                        attr: TyAttr::default(),
                    });
                    return Place::Local(dead);
                }
                // Dynamic map access — only valid for map types, unknown, etc.
                let key_local = self.builder.temp(RuntimeTy::String {
                    attr: TyAttr::default(),
                });
                self.builder.assign(
                    Place::local(key_local),
                    Rvalue::Use(Operand::Constant(Constant::String(member_name.to_string()))),
                );
                Place::Index {
                    base: Box::new(base_place),
                    index: key_local,
                    kind: IndexKind::Map,
                }
            }
            AstExpr::Index { base, index } => {
                let base_id = *base;
                let index_id = *index;
                let base_place = self.lower_lvalue(base_id);
                let index_op = self.lower_to_operand(index_id);
                let base_ty = self.expr_ty(base_id);
                let index_ty = self.expr_ty(index_id);
                let index_local = self.operand_to_local(index_op, index_ty);
                let unwrapped_ty = base_ty.strip_null();
                let kind = if matches!(
                    &unwrapped_ty,
                    RuntimeTy::List(..) | RuntimeTy::Uint8Array { .. }
                ) {
                    IndexKind::Array
                } else {
                    IndexKind::Map
                };
                Place::Index {
                    base: Box::new(base_place),
                    index: index_local,
                    kind,
                }
            }
            AstExpr::OptionalMemberAccess { base, member } => {
                let base_id = *base;
                let member_name = member.clone();

                // Evaluate base once into a temp local
                let base_op = self.lower_to_operand(base_id);
                let base_ty = self.expr_ty(base_id);
                let base_local = self.operand_to_local(base_op, base_ty.clone());

                // Null check using the operand
                let is_null = Rvalue::BinaryOp {
                    op: BinOp::Eq,
                    left: Operand::Copy(Place::Local(base_local)),
                    right: Operand::Constant(Constant::Null),
                };
                let test_local = self.builder.temp(RuntimeTy::Bool {
                    attr: TyAttr::default(),
                });
                self.builder.assign(Place::local(test_local), is_null);

                let bb_continue = self.builder.create_block();
                let bb_null = *self
                    .chain_null_exits
                    .last()
                    .expect("OptionalMemberAccess in lvalue must be inside OptionalChain");
                self.builder.branch(
                    Operand::Copy(Place::Local(test_local)),
                    bb_null,
                    bb_continue,
                );

                self.builder.set_current_block(bb_continue);

                // Project member from the same temp local — no second evaluation
                let base_place = Place::Local(base_local);
                // Unwrap Optional — we've already null-checked, so use the inner type.
                let unwrapped_ty = base_ty.strip_null();
                if let RuntimeTy::Class(tn, _, _) = &unwrapped_ty {
                    if let Some(fields) = self.class_fields.get(tn) {
                        if let Some(&idx) = fields.get(member_name.as_str()) {
                            return Place::Field {
                                base: Box::new(base_place),
                                field: idx,
                            };
                        }
                    }
                }
                // Dynamic map access
                let key_local = self.builder.temp(RuntimeTy::String {
                    attr: TyAttr::default(),
                });
                self.builder.assign(
                    Place::local(key_local),
                    Rvalue::Use(Operand::Constant(Constant::String(member_name.to_string()))),
                );
                Place::Index {
                    base: Box::new(base_place),
                    index: key_local,
                    kind: IndexKind::Map,
                }
            }
            AstExpr::OptionalIndex { base, index } => {
                let base_id = *base;
                let index_id = *index;

                // Evaluate base once into a temp local
                let base_op = self.lower_to_operand(base_id);
                let base_ty = self.expr_ty(base_id);
                let base_local = self.operand_to_local(base_op, base_ty.clone());

                // Null check
                let is_null = Rvalue::BinaryOp {
                    op: BinOp::Eq,
                    left: Operand::Copy(Place::Local(base_local)),
                    right: Operand::Constant(Constant::Null),
                };
                let test_local = self.builder.temp(RuntimeTy::Bool {
                    attr: TyAttr::default(),
                });
                self.builder.assign(Place::local(test_local), is_null);

                let bb_continue = self.builder.create_block();
                let bb_null = *self
                    .chain_null_exits
                    .last()
                    .expect("OptionalIndex in lvalue must be inside OptionalChain");
                self.builder.branch(
                    Operand::Copy(Place::Local(test_local)),
                    bb_null,
                    bb_continue,
                );

                self.builder.set_current_block(bb_continue);

                // Project index from the same temp local
                let index_op = self.lower_to_operand(index_id);
                let index_ty = self.expr_ty(index_id);
                let index_local = self.operand_to_local(index_op, index_ty);
                let unwrapped_ty = base_ty.strip_null();
                let kind = if matches!(
                    &unwrapped_ty,
                    RuntimeTy::List(..) | RuntimeTy::Uint8Array { .. }
                ) {
                    IndexKind::Array
                } else {
                    IndexKind::Map
                };
                Place::Index {
                    base: Box::new(Place::Local(base_local)),
                    index: index_local,
                    kind,
                }
            }
            _ => {
                let ty = self.expr_ty(expr_id);
                let temp = self.builder.temp(ty);
                // A projection assignment may use an arbitrary expression as
                // its base (`store.require().info.title = ...`). Materialize
                // that base exactly once before projecting through it.
                self.lower_expr(expr_id, Place::Local(temp));
                Place::Local(temp)
            }
        }
    }
}

// ─── Match lowering ───────────────────────────────────────────────────────────

impl LoweringContext<'_> {
    fn lower_match(
        &mut self,
        expr_id: AstExprId,
        scrutinee: AstExprId,
        arm_ids: &[baml_compiler2_ast::MatchArmId],
        dest: Place,
    ) {
        let is_exhaustive = self.tir_is_exhaustive_match(self.expr_metadata_key(expr_id));

        let scrutinee_local = self.try_resolve_to_local(scrutinee).unwrap_or_else(|| {
            let op = self.lower_to_operand(scrutinee);
            let ty = self.expr_ty(scrutinee);
            self.operand_to_local(op, ty)
        });

        let bb_join = self.builder.create_block();

        // Collect arms from arena
        let arms: Vec<baml_compiler2_ast::MatchArm> = arm_ids
            .iter()
            .map(|&id| self.body.match_arms[id].clone())
            .collect();

        // Try switch optimization: if all non-wildcard arms have compatible patterns
        // (int literal, enum variant, or type tag) with no guards, emit a Switch.
        let switch_arms: Vec<(AstPatId, AstExprId, Option<AstExprId>)> = arms
            .iter()
            .map(|arm| (arm.pattern, arm.body, arm.guard))
            .collect();

        // Expose the scrutinee's static type to each arm's container type test so
        // it can decide whether a coarse `LIST`/`MAP` tag suffices; restore the
        // enclosing match's scrutinee (if any) once the arms are lowered.
        let saved_scrutinee = self
            .match_scrutinee
            .replace((scrutinee_local, self.expr_ty(scrutinee)));

        let switched = self.try_lower_as_switch(
            scrutinee_local,
            &switch_arms,
            dest.clone(),
            bb_join,
            SwitchOtherwise::Match { is_exhaustive },
            None,
        );
        if !switched {
            // Whether any non-final arm's emitted test can reject a value the
            // static exhaustiveness proof counted as matched (a typevar
            // template test meeting a callable with no reconstructible type —
            // see `pattern_test_can_reject_covered_values`). If so, the final
            // arm's test must be emitted (with a trap on fall-through)
            // instead of skipped: a wrongly-rejected value falling into an
            // untested final arm would silently bind at the wrong type,
            // while the trap fails loud.
            let backstop_last_arm = arms.len().checked_sub(1).is_some_and(|last| {
                arms[..last]
                    .iter()
                    .any(|arm| self.pattern_test_can_reject_covered_values(arm.pattern))
            });
            self.lower_match_chain(
                scrutinee_local,
                &arms,
                dest,
                bb_join,
                is_exhaustive,
                backstop_last_arm,
            );
        }

        self.match_scrutinee = saved_scrutinee;
        self.builder.set_current_block(bb_join);
    }

    /// Attempt to lower a match or catch as a Switch terminator.
    /// Returns true if successful, false if the arms aren't switch-eligible.
    ///
    /// Unified entry point for both match and catch switch dispatch.
    /// - `arms`: `(pattern, body_expr, optional_guard)` tuples
    /// - `otherwise`: controls what happens for unmatched values
    /// - `pre_created_blocks`: if `Some`, use these pre-created body blocks instead
    ///   of creating new ones (used by catch, which pre-creates blocks)
    fn try_lower_as_switch(
        &mut self,
        scrutinee: Local,
        arms: &[(AstPatId, AstExprId, Option<AstExprId>)],
        dest: Place,
        join: BlockId,
        otherwise: SwitchOtherwise,
        pre_created_blocks: Option<&[Option<BlockId>]>,
    ) -> bool {
        use std::collections::HashSet;

        if arms.is_empty() {
            return false;
        }

        let is_exhaustive = matches!(
            &otherwise,
            SwitchOtherwise::Match {
                is_exhaustive: true
            }
        );

        // The enclosing match scrutinee's static type, for the tag-sufficiency
        // gate in `classify_pattern_type_tag`. Guarded on the local so the
        // catch path — whose error local is never the registered match
        // scrutinee — stays conservative (`None`).
        let scrutinee_static_ty: Option<RuntimeTy> = self
            .match_scrutinee
            .as_ref()
            .filter(|(local, _)| *local == scrutinee)
            .map(|(_, ty)| ty.clone());

        // Classify arms: collect (i64_value, arm_index) for int literal or enum variant
        // patterns, and check for a trailing wildcard/binding.
        let mut switch_kind: Option<SwitchKind> = None;
        let mut int_arms: Vec<(i64, usize)> = Vec::new();
        let mut otherwise_idx: Option<usize> = None;
        // Deduplicate discriminant values so union patterns don't produce duplicate switch arms.
        let mut seen_values: HashSet<i64> = HashSet::new();

        for (i, &(pattern, _body, guard)) in arms.iter().enumerate() {
            // Guards disqualify switch optimization
            if guard.is_some() {
                return false;
            }
            // Narrowed bindings require NarrowBind's atomic test-and-bind
            // semantics, which Switch cannot preserve.
            if self.pattern_narrow_type(pattern).is_some() {
                return false;
            }

            // Helpers that classify a pattern (the arm pattern itself, or a
            // sub-pattern of an `Or`) into a switch kind. Mutate `switch_kind`
            // and `int_arms`. Return `false` if the pattern disqualifies.
            let pat = &self.body.patterns[pattern];
            let classify_atom = |this: &Self,
                                 atom_id: AstPatId,
                                 atom: &AstPattern,
                                 switch_kind: &mut Option<SwitchKind>,
                                 int_arms: &mut Vec<(i64, usize)>,
                                 seen_values: &mut HashSet<i64>|
             -> bool {
                match atom {
                    // OLD `Literal(Int(val))`: integer switch
                    AstPattern::Type(AstTypeExpr {
                        kind:
                            AstTypeExprKind::Literal {
                                value: AstLiteral::Int(val),
                                ..
                            },
                        ..
                    }) => {
                        match switch_kind.as_ref() {
                            None => *switch_kind = Some(SwitchKind::Integer),
                            Some(SwitchKind::Integer) => {}
                            Some(_) => return false,
                        }
                        if seen_values.insert(*val) {
                            int_arms.push((*val, i));
                        }
                        true
                    }
                    // OLD `EnumVariant { ... }`: integer switch with discriminant.
                    // The new repr puts enum variants inside `Pattern::Type`;
                    // detect via TIR.
                    AstPattern::Type(AstTypeExpr {
                        kind: AstTypeExprKind::Path { .. },
                        ..
                    }) if matches!(
                        this.tir_pat_type(this.pat_metadata_key(atom_id)),
                        Some(Tir2Ty::EnumVariant(_, _, _))
                    ) =>
                    {
                        let Some(Tir2Ty::EnumVariant(qtn, variant, _)) =
                            this.tir_pat_type(this.pat_metadata_key(atom_id))
                        else {
                            unreachable!("guarded by matches! above");
                        };
                        let enum_name = qtn.clone();
                        let variant = variant.clone();
                        match switch_kind.as_ref() {
                            None => {
                                *switch_kind =
                                    Some(SwitchKind::EnumDiscriminant(enum_name.clone()));
                            }
                            Some(SwitchKind::EnumDiscriminant(n)) if *n == enum_name => {}
                            _ => return false,
                        }
                        let idx = this
                            .enum_variants
                            .get(&enum_name)
                            .and_then(|m| m.get(variant.as_str()))
                            .copied();
                        let Some(idx) = idx else { return false };
                        let disc = i64::try_from(idx).expect("discriminant overflow");
                        if seen_values.insert(disc) {
                            int_arms.push((disc, i));
                        }
                        true
                    }
                    // OLD `Type(_)` / `Bind { .. }` (with TIR type): TypeTag.
                    AstPattern::Type(_) | AstPattern::Bind { .. } => {
                        match switch_kind.as_ref() {
                            None => *switch_kind = Some(SwitchKind::TypeTag),
                            Some(SwitchKind::TypeTag) => {}
                            Some(_) => return false,
                        }
                        match this.classify_pattern_type_tag(atom_id, scrutinee_static_ty.as_ref())
                        {
                            Some(tags) => {
                                for tag in tags {
                                    if seen_values.insert(tag) {
                                        int_arms.push((tag, i));
                                    }
                                }
                            }
                            None => return false,
                        }
                        true
                    }
                    _ => false,
                }
            };

            match pat {
                AstPattern::Or(sub_pats) => {
                    for sub_pat_id in sub_pats {
                        let sub_pat = &self.body.patterns[*sub_pat_id];
                        if !classify_atom(
                            self,
                            *sub_pat_id,
                            sub_pat,
                            &mut switch_kind,
                            &mut int_arms,
                            &mut seen_values,
                        ) {
                            return false;
                        }
                    }
                }
                AstPattern::Wildcard => {
                    if i != arms.len() - 1 {
                        return false;
                    }
                    otherwise_idx = Some(i);
                }
                // Plain `let x` without a narrow always acts as the
                // catch-all arm, enabling jump-table dispatch. (Narrowed
                // bindings — e.g. `let n: int` — are encoded as `Chain` and
                // were handled by the `pattern_narrow_type` branch above.)
                AstPattern::Bind { .. } => {
                    if i != arms.len() - 1 {
                        return false;
                    }
                    otherwise_idx = Some(i);
                }
                _ => {
                    if !classify_atom(
                        self,
                        pattern,
                        pat,
                        &mut switch_kind,
                        &mut int_arms,
                        &mut seen_values,
                    ) {
                        return false;
                    }
                }
            }
        }

        // Need at least one int arm to justify a switch.
        if int_arms.is_empty() {
            return false;
        }

        // TypeTag switches only pay off at 4+ arms (JumpTable). For fewer arms
        // the sequential `is_type` chain is more compact because the if-else
        // chain adds copy/pop stack management overhead per arm.
        if matches!(switch_kind, Some(SwitchKind::TypeTag)) && int_arms.len() < 4 {
            return false;
        }

        // Exhaustiveness: for **match** TypeTag switches without a wildcard arm,
        // all typed arms together cover the union — the otherwise block is dead.
        // TIR's `required_match_cases` returns None for class types, so class
        // unions are never marked exhaustive by TIR even when all arms are
        // covered. For match + TypeTag, if there's no wildcard, treat as
        // exhaustive so the last arm skips its comparison and the otherwise
        // block becomes Unreachable.
        //
        // For **catch** expressions, we never mark the switch as exhaustive
        // even when all declared thrown types are covered, because panics can
        // always occur at runtime and must be rethrown via the otherwise block.
        let is_match = matches!(&otherwise, SwitchOtherwise::Match { .. });
        let is_switch_exhaustive = otherwise_idx.is_none()
            && (is_exhaustive || (is_match && matches!(switch_kind, Some(SwitchKind::TypeTag))));

        // Save the entry block — this is where the switch terminator goes
        let bb_entry = self.builder.current_block();

        // Emit discriminant/type-tag extraction before building arm blocks.
        // We must do this before create_block() calls so the assignment goes into bb_entry.
        let switch_operand = match &switch_kind {
            Some(SwitchKind::EnumDiscriminant(_)) => {
                let disc = self.builder.temp(RuntimeTy::Int {
                    attr: TyAttr::default(),
                });
                self.builder.assign(
                    Place::local(disc),
                    Rvalue::Discriminant(Place::local(scrutinee)),
                );
                Operand::Copy(Place::Local(disc))
            }
            Some(SwitchKind::TypeTag) => {
                let tag_local = self.builder.temp(RuntimeTy::Int {
                    attr: TyAttr::default(),
                });
                self.builder.assign(
                    Place::local(tag_local),
                    Rvalue::TypeTag(Place::local(scrutinee)),
                );
                Operand::Copy(Place::Local(tag_local))
            }
            _ => Operand::Copy(Place::Local(scrutinee)),
        };

        // Build body blocks for each arm. Union sub-patterns sharing the same
        // arm_idx reuse a single block (e.g. Active | Pending → same bb).
        let bb_otherwise = self.builder.create_block();
        let mut switch_arms: Vec<(i64, BlockId)> = Vec::new();
        let mut arm_blocks: std::collections::HashMap<usize, BlockId> =
            std::collections::HashMap::new();

        for &(val, arm_idx) in &int_arms {
            if let Some(&existing_bb) = arm_blocks.get(&arm_idx) {
                // Union sub-pattern: reuse the same body block
                switch_arms.push((val, existing_bb));
            } else {
                // Use pre-created block if available, otherwise create a new one
                let bb_body = if let Some(blocks) = pre_created_blocks {
                    blocks[arm_idx].expect("pre-created block missing for arm")
                } else {
                    self.builder.create_block()
                };
                switch_arms.push((val, bb_body));
                arm_blocks.insert(arm_idx, bb_body);

                self.builder.set_current_block(bb_body);
                let (pattern, body, _) = arms[arm_idx];
                let saved_locals = self.locals.clone();
                self.bind_pattern(scrutinee, pattern);
                self.lower_expr(body, dest.clone());
                if !self.builder.is_current_terminated() {
                    self.builder.goto(join);
                }
                self.restore_locals_after_scope(saved_locals);
            }
        }

        // Build arm_names: symbolic labels for the switch arms (debug metadata).
        let arm_names: Vec<(i64, String)> = match &switch_kind {
            Some(SwitchKind::EnumDiscriminant(enum_name)) => {
                if let Some(variants) = self.enum_variants.get(enum_name) {
                    // Build reverse map: variant_idx -> variant_name
                    let reverse: std::collections::HashMap<i64, &str> = variants
                        .iter()
                        .map(|(name, idx)| {
                            (
                                i64::try_from(*idx).expect("discriminant overflow"),
                                name.as_str(),
                            )
                        })
                        .collect();
                    int_arms
                        .iter()
                        .filter_map(|(val, _)| {
                            reverse
                                .get(val)
                                .map(|vname| (*val, format!("{}.{vname}", enum_name.name())))
                        })
                        .collect()
                } else {
                    vec![]
                }
            }
            Some(SwitchKind::TypeTag) => {
                // Reverse map: tag value → human-readable type name.
                let reverse_class: std::collections::HashMap<i64, &str> = self
                    .class_type_tags
                    .iter()
                    .map(|(tn, tag)| (*tag, tn.name().as_str()))
                    .collect();
                int_arms
                    .iter()
                    .map(|(v, _)| {
                        let name = reverse_class
                            .get(v)
                            .map(ToString::to_string)
                            .unwrap_or_else(|| format_type_tag_name(*v));
                        (*v, name)
                    })
                    .collect()
            }
            _ => int_arms.iter().map(|(v, _)| (*v, v.to_string())).collect(),
        };

        // Lower the otherwise block
        self.builder.set_current_block(bb_otherwise);
        if let Some(idx) = otherwise_idx {
            // Wildcard arm present
            if let SwitchOtherwise::Catch {
                error_local,
                needs_throw_if_panic: true,
            } = &otherwise
            {
                let bb_wildcard_body = self.builder.create_block();
                self.builder
                    .throw_if_panic(Operand::Copy(Place::Local(*error_local)), bb_wildcard_body);
                self.builder.set_current_block(bb_wildcard_body);
            }
            let (pattern, body, _) = arms[idx];
            let saved_locals = self.locals.clone();
            self.bind_pattern(scrutinee, pattern);
            self.lower_expr(body, dest);
            if !self.builder.is_current_terminated() {
                self.builder.goto(join);
            }
            self.restore_locals_after_scope(saved_locals);
        } else {
            // No wildcard — decide what the otherwise block does.
            // Use `is_switch_exhaustive` (which may be inferred for TypeTag)
            // rather than the caller's original `is_exhaustive`, so the
            // otherwise block stays consistent with the switch terminator flag.
            if is_switch_exhaustive {
                match &otherwise {
                    SwitchOtherwise::Match { .. } => {
                        self.builder.unreachable();
                    }
                    SwitchOtherwise::Catch { error_local, .. } => {
                        // Even if exhaustive, catch otherwise should rethrow
                        // (the error might not match any arm at runtime).
                        self.builder
                            .rethrow(Operand::Copy(Place::Local(*error_local)));
                    }
                }
            } else {
                match &otherwise {
                    SwitchOtherwise::Catch { error_local, .. } => {
                        self.builder
                            .rethrow(Operand::Copy(Place::Local(*error_local)));
                    }
                    SwitchOtherwise::Match { .. } => {
                        self.builder.goto(join);
                    }
                }
            }
        }

        // For catch with pre-created blocks: redirect wildcard arm's pre-created block
        // to bb_otherwise, since the wildcard body was lowered there.
        if let Some(blocks) = pre_created_blocks {
            for (i, block_opt) in blocks.iter().enumerate() {
                if let Some(block) = block_opt {
                    if otherwise_idx == Some(i) {
                        // Wildcard arm's pre-created block → redirect to otherwise
                        self.builder.set_current_block(*block);
                        self.builder.goto(bb_otherwise);
                    } else if !arm_blocks.contains_key(&i) {
                        // Unreachable pre-created block (e.g. duplicate tag) → terminate it
                        self.builder.set_current_block(*block);
                        self.builder.goto(bb_otherwise);
                    }
                }
            }
        }

        // Emit the switch terminator in the entry block
        self.builder.set_current_block(bb_entry);
        self.builder.switch(
            switch_operand,
            switch_arms,
            bb_otherwise,
            is_switch_exhaustive,
            arm_names,
        );

        true
    }

    fn lower_match_chain(
        &mut self,
        scrutinee: Local,
        arms: &[baml_compiler2_ast::MatchArm],
        dest: Place,
        join: BlockId,
        exhaustive: bool,
        backstop_last_arm: bool,
    ) {
        if arms.is_empty() {
            // No more arms to test. Either a preceding wildcard/binding arm
            // consumed all inputs (making this dead code), or the match is
            // non-exhaustive and a runtime value could reach here. In both
            // cases, jump to the join block so execution continues.
            self.builder.goto(join);
            return;
        }

        let arm = &arms[0];
        let rest = &arms[1..];

        // Exhaustive last arm: skip the pattern test — it must match. Do not
        // take this shortcut for Or-patterns because bindings must come from
        // the specific alternative that matched.
        if exhaustive
            && rest.is_empty()
            && arm.guard.is_none()
            && !matches!(self.body.patterns[arm.pattern], AstPattern::Or(_))
        {
            // When a preceding arm's test can reject a value the static
            // coverage proof counted as matched (`backstop_last_arm` — a
            // typevar template test meeting a callable with no
            // reconstructible type), "it must match" no longer follows from
            // exhaustiveness: the wrongly-rejected value falls through to
            // this arm. Emit the final refutable arm's test anyway and trap
            // on fall-through — a loud panic instead of silently binding the
            // value to a pattern it does not match. Irrefutable last arms
            // (wildcard, bare bind) match everything, so they keep the plain
            // skip.
            let last_is_refutable = !matches!(
                self.body.patterns[arm.pattern],
                AstPattern::Wildcard | AstPattern::Bind { subpat: None, .. }
            );
            if backstop_last_arm && last_is_refutable {
                let bb_body = self.builder.create_block();
                let bb_trap = self.builder.create_block();
                self.lower_pattern_test(scrutinee, arm.pattern, bb_body, bb_trap);
                self.builder.set_current_block(bb_trap);
                self.builder.unreachable();
                self.builder.set_current_block(bb_body);
            }
            let saved_locals = self.locals.clone();
            self.bind_pattern(scrutinee, arm.pattern);
            self.lower_expr(arm.body, dest);
            if !self.builder.is_current_terminated() {
                self.builder.goto(join);
            }
            self.restore_locals_after_scope(saved_locals);
            return;
        }

        if let AstPattern::Or(parts) = self.body.patterns[arm.pattern].clone() {
            let bb_next = self.builder.create_block();
            for (idx, part) in parts.iter().copied().enumerate() {
                let bb_body = self.builder.create_block();
                let bb_alt_next = if idx + 1 == parts.len() {
                    bb_next
                } else {
                    self.builder.create_block()
                };

                self.lower_pattern_test(scrutinee, part, bb_body, bb_alt_next);

                self.builder.set_current_block(bb_body);
                let saved_locals = self.locals.clone();
                self.bind_pattern_inner(scrutinee, part, arm.pattern, part, false);
                if let Some(guard) = arm.guard {
                    let guard_op = self.lower_to_operand(guard);
                    let bb_guarded = self.builder.create_block();
                    self.builder.branch(guard_op, bb_guarded, bb_next);
                    self.builder.set_current_block(bb_guarded);
                }
                self.lower_expr(arm.body, dest.clone());
                if !self.builder.is_current_terminated() {
                    self.builder.goto(join);
                }
                self.restore_locals_after_scope(saved_locals);

                if idx + 1 < parts.len() {
                    self.builder.set_current_block(bb_alt_next);
                }
            }

            self.builder.set_current_block(bb_next);
            self.lower_match_chain(scrutinee, rest, dest, join, exhaustive, backstop_last_arm);
            return;
        }

        let bb_body = self.builder.create_block();
        let bb_next = self.builder.create_block();

        self.lower_pattern_test(scrutinee, arm.pattern, bb_body, bb_next);

        self.builder.set_current_block(bb_body);
        let saved_locals = self.locals.clone();
        self.bind_pattern(scrutinee, arm.pattern);
        if let Some(guard) = arm.guard {
            let guard_op = self.lower_to_operand(guard);
            let bb_guarded = self.builder.create_block();
            self.builder.branch(guard_op, bb_guarded, bb_next);
            self.builder.set_current_block(bb_guarded);
        }
        self.lower_expr(arm.body, dest.clone());
        if !self.builder.is_current_terminated() {
            self.builder.goto(join);
        }
        self.restore_locals_after_scope(saved_locals);

        self.builder.set_current_block(bb_next);
        self.lower_match_chain(scrutinee, rest, dest, join, exhaustive, backstop_last_arm);
    }

    /// Whether lowering `pat`'s runtime test can *reject a value the static
    /// coverage proof counted as matched*. TIR's coverage relation is the
    /// same canonical invariant subtype the emitted tests evaluate, so for
    /// values with a concrete runtime type the two always agree — with one
    /// exception: a value whose concrete type the VM cannot reconstruct
    /// (`value_concrete_ty` is `None` — an opaque native handle) fails every
    /// structural test, including one whose realized binding IS that value's
    /// static type. A non-final `let v: T` arm can therefore wrongly reject
    /// such a value, so an exhaustive match containing one cannot safely skip
    /// its final arm's test (see `lower_match_chain`'s backstop).
    ///
    /// Over-approximation is safe — it costs one extra final test plus a dead
    /// trap block; under-approximation would silently bind a value to a
    /// pattern it does not match.
    fn pattern_test_can_reject_covered_values(&self, pat_id: AstPatId) -> bool {
        match &self.body.patterns[pat_id] {
            AstPattern::Or(parts) => parts
                .iter()
                .any(|p| self.pattern_test_can_reject_covered_values(*p)),
            AstPattern::Bind {
                subpat: Some(sp), ..
            } => self.pattern_test_can_reject_covered_values(*sp),
            // Irrefutable patterns emit no test.
            AstPattern::Wildcard | AstPattern::Bind { subpat: None, .. } => false,
            AstPattern::Type(_)
            | AstPattern::Unreflect(_)
            | AstPattern::Class { .. }
            | AstPattern::Array { .. } => {
                let Some(tir_ty) = self.tir_pat_type(self.pat_metadata_key(pat_id)) else {
                    return false;
                };
                // Typevar-carrying patterns route through the dispatch-guard
                // template (see `lower_pattern_test`); only their realized
                // bindings can name a function type a closure would need to
                // match.
                baml_type_runtime::contains_typevar(tir_ty)
            }
        }
    }

    /// Emit an `IsType` check that handles union types by expanding them
    /// into a chain: try each member, branch to `success` if any matches.
    fn emit_is_type_branch(
        &mut self,
        scrutinee: Local,
        ty: RuntimeTy,
        success: BlockId,
        failure: BlockId,
    ) {
        // BEP-044/BEP-057: testing a value against an *interface* type means "is
        // its concrete runtime type an implementor" — emitted as a single `IsType` on the
        // interface existential itself, which the VM resolves through the canonical
        // membership check against the impl registry (`type_implements`, at the
        // requested instantiation: type args and associated bindings validated
        // exactly). The bytecode never enumerates implementors: a compile-time list
        // is closed-world (an implementor loaded from a later package would silently
        // fail the test).
        //
        // Interfaces reach here already tagged `RuntimeTy::Interface`: TIR resolves
        // an interface reference to `Ty::Interface` (never `Ty::Class`), and
        // `lower_to_runtime` preserves the tag and its instantiation. So no
        // name-based re-tag is needed — matching a `RuntimeTy::Class` against
        // `interface_implementors` would both miss declaration-only interfaces and
        // drop the instantiation.
        if let RuntimeTy::Union(members, _) = ty {
            // For union A | B | C: check A → success, else check B → success,
            // else check C → success, else failure.
            let mut remaining = members.into_iter().peekable();
            while let Some(member) = remaining.next() {
                if remaining.peek().is_none() {
                    // Last member: branch directly to success/failure.
                    self.emit_is_type_branch(scrutinee, member, success, failure);
                } else {
                    // Not last: if this member matches, jump to success;
                    // otherwise try the next member.
                    let next_check = self.builder.create_block();
                    self.emit_is_type_branch(scrutinee, member, success, next_check);
                    self.builder.set_current_block(next_check);
                }
            }
        } else {
            // If this is a parametric arm whose type arguments the emitter would
            // otherwise check structurally, but the enclosing match's scrutinee
            // statically proves a coarse `LIST`/`MAP`/`FUTURE` tag suffices,
            // emit the tag test directly. Guarded on the scrutinee local so an
            // `is` / nested test against a different value stays arg-precise.
            if self
                .match_scrutinee
                .as_ref()
                .is_some_and(|(local, scrutinee_ty)| {
                    *local == scrutinee && parametric_arm_tag_sufficient(&ty, scrutinee_ty)
                })
            {
                let tag = match &ty {
                    RuntimeTy::List(..) => baml_type::typetag::LIST,
                    RuntimeTy::Map { .. } => baml_type::typetag::MAP,
                    RuntimeTy::Future(..) => baml_type::typetag::FUTURE,
                    // `parametric_arm_tag_sufficient` returns false for every
                    // non-parametric arm.
                    other => unreachable!("tag-sufficient non-parametric arm: {other}"),
                };
                self.emit_is_type_tag_branch(scrutinee, tag, success, failure);
                return;
            }
            // Convert RuntimeTy → template so generic class checks (a
            // `RuntimeTy::Class` with concrete args) become an arg-precise
            // `Class` test. For non-generic types the template is a realized
            // leaf — the emitter's tag / class-identity fast path. A residual
            // symbolic position becomes a guard hole the pattern test refuses.
            let guard = ty_to_pattern_template_from_resolved_ty(&ty);
            self.emit_pattern_template_test(scrutinee, guard, success, failure);
        }
    }

    /// Emit an `IsTypeTag` coarse-tag test + branch (the container arm of a
    /// match whose scrutinee proves the tag sufficient).
    fn emit_is_type_tag_branch(
        &mut self,
        scrutinee: Local,
        tag: i64,
        success: BlockId,
        failure: BlockId,
    ) {
        let test = Rvalue::IsTypeTag {
            operand: Operand::Copy(Place::Local(scrutinee)),
            tag,
        };
        let test_local = self.builder.temp(RuntimeTy::Bool {
            attr: TyAttr::default(),
        });
        self.builder.assign(Place::local(test_local), test);
        self.builder
            .branch(Operand::Copy(Place::Local(test_local)), success, failure);
    }

    /// Emit a `match`/`is` pattern test from a pattern-template build.
    ///
    /// A pattern denotes exactly one type per frame; the builders report an
    /// unresolvable position (an associated projection, a type variable
    /// without a frame slot) as `None` at construction, and such a pattern
    /// has no complete test — it is fail-closed: branch straight to `failure`.
    ///
    /// FIXME(typevar-templates): the sound fix for the fail-closed cases is
    /// to resolve them (projection templates over frame refs, the way `Self`
    /// gained its frame slot) or reject them at TIR — never to match-any.
    fn emit_pattern_template_test(
        &mut self,
        scrutinee: Local,
        template: Option<TyTemplate>,
        success: BlockId,
        failure: BlockId,
    ) {
        match template {
            Some(ty_template) => {
                self.emit_is_type_template_branch(scrutinee, ty_template, success, failure);
            }
            None => self.builder.goto(failure),
        }
    }

    /// Emit an `IsType` test + branch for an already-built `TyTemplate`.
    ///
    /// Used directly (instead of [`Self::emit_is_type_branch`]) when the
    /// pattern type still contains the enclosing function's `TypeVar`s: the
    /// caller builds the template via `ty_to_template` so those lower to
    /// `TypeArgRef` leaves resolved against `frame.type_args` at runtime,
    /// rather than being erased to `RuntimeTy::Void` (a constant-false test).
    fn emit_is_type_template_branch(
        &mut self,
        scrutinee: Local,
        ty_template: TyTemplate,
        success: BlockId,
        failure: BlockId,
    ) {
        if self.atomic_pattern_test {
            self.builder.narrow_bind(
                Operand::Copy(Place::Local(scrutinee)),
                ty_template,
                scrutinee,
                success,
                failure,
            );
        } else {
            let test = Rvalue::IsType {
                operand: Operand::Copy(Place::Local(scrutinee)),
                ty_template,
            };
            let test_local = self.builder.temp(RuntimeTy::Bool {
                attr: TyAttr::default(),
            });
            self.builder.assign(Place::local(test_local), test);
            self.builder
                .branch(Operand::Copy(Place::Local(test_local)), success, failure);
        }
    }

    /// The members of a union receiver, transparently unwrapping `Optional`
    /// layers — `(Dog | Named)?` after a null check still dispatches the
    /// field/method on the underlying union. Returns `None` when `ty` isn't a
    /// (optionally-wrapped) union.
    fn tir_union_members(ty: &Tir2Ty) -> Option<Vec<Tir2Ty>> {
        match ty {
            Tir2Ty::Union(members, _) => Some(members.clone()),
            _ => None,
        }
    }

    /// Whether `ty` is (or contains, inside a union/optional) an interface view
    /// whose runtime test must respect type arguments or associated bindings.
    /// Used to opt only these patterns into the TIR-typed test path, leaving
    /// non-interface patterns on the unchanged erased fast path.
    fn tir_ty_needs_interface_shape_test(ty: &Tir2Ty) -> bool {
        match ty {
            Tir2Ty::Interface(_, args, associated_bindings, _) => {
                !args.is_empty() || !associated_bindings.is_empty()
            }
            Tir2Ty::Union(members, _) => {
                members.iter().any(Self::tir_ty_needs_interface_shape_test)
            }
            _ => false,
        }
    }

    fn emit_is_tir_type_branch(
        &mut self,
        scrutinee: Local,
        ty: &Tir2Ty,
        success: BlockId,
        failure: BlockId,
    ) {
        match ty {
            Tir2Ty::Union(members, _) => {
                let mut remaining = members.iter().peekable();
                while let Some(member) = remaining.next() {
                    if remaining.peek().is_none() {
                        self.emit_is_tir_type_branch(scrutinee, member, success, failure);
                    } else {
                        let next_check = self.builder.create_block();
                        self.emit_is_tir_type_branch(scrutinee, member, success, next_check);
                        self.builder.set_current_block(next_check);
                    }
                }
            }
            Tir2Ty::Class(_, type_args, _) if !type_args.is_empty() => {
                // One arg-precise instantiation test. Building it TIR-side lets
                // the enclosing function's type variables (including an
                // interface-owned body's `Self`) lower to `TypeArgRef` frame
                // slots, so the VM compares the value's stored class type args
                // against this frame's realizations invariantly, rather than
                // `convert` erasing them into unresolvable names. Residual
                // symbolic args (projections, a slotless type variable) make
                // the build `None`, failing the test closed.
                //
                // This test is complete on its own — do not add a walk over the
                // class's declared field types. A value's fields already
                // inhabit its own instantiation, so testing them decides
                // nothing the arg comparison hasn't; and under a rigid
                // instantiation (`Cell<T>`) a declared field type is the bare
                // type variable, which no value-level test can decide, so such
                // a walk fails every such pattern closed.
                let generic_params = self.enclosing_generic_params();
                let header = tir2_to_pattern_template(ty, self.resolved_aliases, &generic_params);
                self.emit_pattern_template_test(scrutinee, header, success, failure);
            }
            // An interface pattern (`Slot<int>`, `Source<Item = int>`, or a bare
            // `Named`) is a single membership test against the interface
            // existential itself: the VM's canonical algebra resolves the value's
            // concrete class against the impl registry at the requested
            // instantiation (type args and associated bindings validated exactly;
            // an omitted dimension matches any). The bytecode never enumerates
            // implementors — a compile-time list is closed-world (an implementor
            // in a later-loaded package would silently fail the test). The
            // enclosing function's type variables in the instantiation lower to
            // `TypeArgRef` frame slots, resolved at runtime.
            Tir2Ty::Interface(..) => {
                let generic_params = self.enclosing_generic_params();
                let guard = tir2_to_pattern_template(ty, self.resolved_aliases, &generic_params);
                self.emit_pattern_template_test(scrutinee, guard, success, failure);
            }
            // Singleton-valued types pin a specific runtime value, so emit
            // equality checks rather than type-tag tests. `is_type` on a
            // literal type like `RuntimeTy::Literal("specific")` checks the value's
            // *type* (string) rather than its content — which is too permissive
            // and would let `let x: "specific" => …` fire on any string.
            Tir2Ty::Literal(lit, _, _) => {
                let constant = Self::lower_literal(lit);
                self.emit_value_eq_branch(scrutinee, Operand::Constant(constant), success, failure);
            }
            Tir2Ty::Null { .. } => {
                self.emit_value_eq_branch(
                    scrutinee,
                    Operand::Constant(Constant::Null),
                    success,
                    failure,
                );
            }
            _ => {
                let resolved = self.resolved_aliases.convert(ty);
                self.emit_is_type_branch(scrutinee, resolved, success, failure);
            }
        }
    }

    /// Branch on `scrutinee == rhs` (value equality). Used for singleton-typed
    /// patterns where the type pins a specific value.
    fn emit_value_eq_branch(
        &mut self,
        scrutinee: Local,
        rhs: Operand,
        success: BlockId,
        failure: BlockId,
    ) {
        let test = Rvalue::BinaryOp {
            op: BinOp::Eq,
            left: Operand::Copy(Place::Local(scrutinee)),
            right: rhs,
        };
        let test_local = self.builder.temp(RuntimeTy::Bool {
            attr: TyAttr::default(),
        });
        self.builder.assign(Place::local(test_local), test);
        self.builder
            .branch(Operand::Copy(Place::Local(test_local)), success, failure);
    }

    fn lookup_tir_class_fields(
        &self,
        class_name: &QualifiedTypeName,
        class_type_args: &[Tir2Ty],
    ) -> IndexMap<Name, Tir2Ty> {
        let pkg_id = PackageId::new(self.db, class_name.package().clone());
        let pkg_items_for_class = package_items(self.db, pkg_id);
        let Some(Definition::Class(class_loc)) =
            pkg_items_for_class.lookup_type(class_name.namespace(), class_name.name())
        else {
            return IndexMap::new();
        };

        let file = class_loc.file(self.db);
        let ns_context = file_package(self.db, file).namespace_path;
        let class_data = baml_compiler2_ppir::item_data::class_data(self.db, class_loc);
        let class_generic_params =
            baml_compiler2_hir_ty::lower::class_generic_frame(self.db, class_loc);
        let bindings = baml_type_runtime::bind_type_vars(&class_generic_params, class_type_args);

        let mut result = IndexMap::new();
        for field in &class_data.fields {
            let field_ty = if bindings.is_empty() {
                lower_ref_in_scope(
                    self.db,
                    &class_data.type_refs,
                    field.type_ref,
                    pkg_items_for_class,
                    &ns_context,
                    &class_generic_params,
                    &baml_compiler2_hir_ty::lower::class_generic_bounds(self.db, class_loc),
                    None,
                )
            } else {
                lower_ref_with_bindings(
                    self.db,
                    &class_data.type_refs,
                    field.type_ref,
                    pkg_items_for_class,
                    &ns_context,
                    &bindings,
                    &baml_compiler2_hir_ty::lower::class_generic_bounds(self.db, class_loc),
                )
            };
            result.insert(field.name.clone(), field_ty);
        }
        result
    }

    /// Look up the integer type tag for a type. Returns `Some(tag)` for
    /// primitives (INT=0, STRING=1, etc.) and classes (`CLASS_BASE` + index),
    /// or `None` for types that don't have a tag (unions, generics, etc.).
    fn type_tag_for_ty(&self, ty: &RuntimeTy) -> Option<i64> {
        match ty {
            RuntimeTy::Int { .. } => Some(baml_type::typetag::INT),
            RuntimeTy::Bigint { .. } => Some(baml_type::typetag::BIGINT),
            RuntimeTy::String { .. } => Some(baml_type::typetag::STRING),
            RuntimeTy::Bool { .. } => Some(baml_type::typetag::BOOL),
            RuntimeTy::Null { .. } => Some(baml_type::typetag::NULL),
            RuntimeTy::Float { .. } => Some(baml_type::typetag::FLOAT),
            RuntimeTy::Uint8Array { .. } => Some(baml_type::typetag::UINT8ARRAY),
            // The single shared ENUM tag conflates *all* enum types (and all
            // variants of one enum). That's safe on the switch path only because
            // `switch_member_tag_sufficient` bails an enum-type arm to the precise
            // chain whenever a second enum type shares this tag, so `Color |
            // Status` no longer dedups the second arm away; the chain tests
            // enum-pointer identity (`is Color`). A single-enum-type switch keeps
            // the fast tag.
            RuntimeTy::Enum(..) | RuntimeTy::EnumVariant(..) => Some(baml_type::typetag::ENUM),
            RuntimeTy::List(..) => Some(baml_type::typetag::LIST),
            RuntimeTy::Map { .. } => Some(baml_type::typetag::MAP),
            RuntimeTy::Function { .. } => Some(baml_type::typetag::FUNCTION),
            RuntimeTy::Future(..) => Some(baml_type::typetag::FUTURE),
            RuntimeTy::Type { .. } => Some(baml_type::typetag::TYPE),
            RuntimeTy::Class(tn, _, _) => self.class_type_tags.get(tn).copied(),
            _ => None,
        }
    }

    fn pattern_contains_structural(&self, pat_id: AstPatId) -> bool {
        match &self.body.patterns[pat_id] {
            AstPattern::Class { .. } | AstPattern::Array { .. } => true,
            AstPattern::Or(parts) => parts.iter().any(|p| self.pattern_contains_structural(*p)),
            AstPattern::Wildcard
            | AstPattern::Bind { .. }
            | AstPattern::Type(_)
            | AstPattern::Unreflect(_) => false,
        }
    }

    fn class_pattern_fields(&self, pat_id: AstPatId) -> Vec<baml_compiler2_ast::FieldPat> {
        match &self.body.patterns[pat_id] {
            AstPattern::Class { fields, .. } => fields.clone(),
            _ => Vec::new(),
        }
    }

    fn class_pattern_type_name(&self, pat_id: AstPatId) -> Option<TypeName> {
        let tir_ty = self.tir_pat_type(self.pat_metadata_key(pat_id))?;
        match self.resolved_aliases.convert(tir_ty) {
            RuntimeTy::Class(tn, _, _) => Some(tn),
            _ => None,
        }
    }

    fn class_pattern_field_ty(&self, pat_id: AstPatId, field: &Name) -> Option<RuntimeTy> {
        let tir_ty = self.tir_pat_type(self.pat_metadata_key(pat_id))?;
        let Tir2Ty::Class(qtn, type_args, _) = tir_ty else {
            return None;
        };
        let fields = self.lookup_tir_class_fields(qtn, type_args);
        fields
            .get(field)
            .map(|field_ty| self.resolved_aliases.convert(field_ty))
    }

    fn project_class_pattern_field(
        &mut self,
        scrutinee: Local,
        class_pat_id: AstPatId,
        field_pat_id: AstPatId,
        field: &Name,
    ) -> Option<Local> {
        // BEP-044: an interface head (`Animal { name } => ...`) has no
        // positional field layout. Branch on the raw TIR type so interface
        // patterns project through field-view dispatch instead of class slots.
        if matches!(
            self.tir_pat_type(self.pat_metadata_key(class_pat_id)),
            Some(Tir2Ty::Interface(..))
        ) {
            return self.project_interface_pattern_field(
                scrutinee,
                class_pat_id,
                field_pat_id,
                field,
            );
        }
        let class_tn = self.class_pattern_type_name(class_pat_id)?;
        let field_idx = self
            .class_fields
            .get(&class_tn)?
            .get(field.as_str())
            .copied()?;
        let inferred_pat_ty = self.pat_ty(field_pat_id);
        let source_field_ty = self.class_pattern_field_ty(class_pat_id, field);
        let cached_field_ty = self
            .class_field_types
            .get(&class_tn)
            .and_then(|fields| fields.get(field.as_str()))
            .cloned();
        let field_ty = source_field_ty
            .or_else(|| cached_field_ty.filter(|ty| !Self::is_pattern_type_recovery(ty)))
            .unwrap_or(inferred_pat_ty);
        let field_local = self.builder.temp(field_ty);
        self.builder.assign(
            Place::local(field_local),
            Rvalue::Use(Operand::Copy(Place::Field {
                base: Box::new(Place::Local(scrutinee)),
                field: field_idx,
            })),
        );
        Some(field_local)
    }

    /// BEP-044: project a field bound by an *interface* destructure pattern
    /// (`Animal { name } => …`). The scrutinee's concrete runtime class is not
    /// known statically, so we can't index a fixed field slot. Instead we reuse
    /// the interface field-view dispatch (`try_lower_interface_field_access`) —
    /// the same code that lowers `iface_value.name` — to read the linked field
    /// off whichever implementor the value actually is.
    fn project_interface_pattern_field(
        &mut self,
        scrutinee: Local,
        class_pat_id: AstPatId,
        field_pat_id: AstPatId,
        field: &Name,
    ) -> Option<Local> {
        let tir_ty = self
            .tir_pat_type(self.pat_metadata_key(class_pat_id))?
            .clone();
        let (iface_tn, iface_args, iface_assoc) =
            self.interface_dispatch_target_for_member(&tir_ty, field)?;
        let field_local = self.builder.temp(self.pat_ty(field_pat_id));
        self.try_lower_interface_field_access(
            scrutinee,
            &iface_tn,
            &iface_args,
            &iface_assoc,
            field,
            &Place::local(field_local),
        )
        .then_some(field_local)
    }

    fn const_int_local(&mut self, value: i64) -> Local {
        let local = self.builder.temp(RuntimeTy::Int {
            attr: TyAttr::default(),
        });
        self.builder.assign(
            Place::local(local),
            Rvalue::Use(Operand::Constant(Constant::Int(value))),
        );
        local
    }

    fn const_usize_int_local(&mut self, value: usize) -> Local {
        self.const_int_local(i64::try_from(value).expect("array pattern length/index overflow"))
    }

    fn array_len_local(&mut self, scrutinee: Local) -> Local {
        let len_local = self.builder.temp(RuntimeTy::Int {
            attr: TyAttr::default(),
        });
        self.builder.assign(
            Place::local(len_local),
            Rvalue::Len(Place::local(scrutinee)),
        );
        len_local
    }

    fn lower_array_pattern_length_test(
        &mut self,
        scrutinee: Local,
        has_rest: bool,
        fixed_len: usize,
        success: BlockId,
        failure: BlockId,
    ) {
        let len_local = self.array_len_local(scrutinee);
        let expected = self.const_usize_int_local(fixed_len);
        let test_local = self.builder.temp(RuntimeTy::Bool {
            attr: TyAttr::default(),
        });
        self.builder.assign(
            Place::local(test_local),
            Rvalue::BinaryOp {
                op: if has_rest { BinOp::Ge } else { BinOp::Eq },
                left: Operand::Copy(Place::local(len_local)),
                right: Operand::Copy(Place::local(expected)),
            },
        );
        self.builder
            .branch(Operand::Copy(Place::local(test_local)), success, failure);
    }

    fn project_array_pattern_element_from_start(
        &mut self,
        scrutinee: Local,
        elem_pat: AstPatId,
        index: usize,
    ) -> Local {
        let index_local = self.const_usize_int_local(index);
        self.project_array_pattern_element(scrutinee, elem_pat, index_local)
    }

    fn project_array_pattern_element_from_end(
        &mut self,
        scrutinee: Local,
        elem_pat: AstPatId,
        index_from_end: usize,
    ) -> Local {
        let len_local = self.array_len_local(scrutinee);
        let offset = self.const_usize_int_local(index_from_end);
        let index_local = self.builder.temp(RuntimeTy::Int {
            attr: TyAttr::default(),
        });
        self.builder.assign(
            Place::local(index_local),
            Rvalue::BinaryOp {
                op: BinOp::Sub,
                left: Operand::Copy(Place::local(len_local)),
                right: Operand::Copy(Place::local(offset)),
            },
        );
        self.project_array_pattern_element(scrutinee, elem_pat, index_local)
    }

    fn project_array_pattern_element(
        &mut self,
        scrutinee: Local,
        elem_pat: AstPatId,
        index_local: Local,
    ) -> Local {
        let elem_ty = self.pat_ty(elem_pat);
        let elem_local = self.builder.temp(elem_ty);
        self.builder.assign(
            Place::local(elem_local),
            Rvalue::Use(Operand::Copy(Place::Index {
                base: Box::new(Place::Local(scrutinee)),
                index: index_local,
                kind: IndexKind::Array,
            })),
        );
        elem_local
    }

    fn project_array_pattern_rest(
        &mut self,
        scrutinee: Local,
        rest_pat: AstPatId,
        prefix_len: usize,
        suffix_len: usize,
    ) -> Local {
        let rest_ty = self.pat_ty(rest_pat);
        let rest_local = self.builder.temp(rest_ty);
        let start = self.const_usize_int_local(prefix_len);
        let end = if suffix_len == 0 {
            self.array_len_local(scrutinee)
        } else {
            let len_local = self.array_len_local(scrutinee);
            let suffix = self.const_usize_int_local(suffix_len);
            let end = self.builder.temp(RuntimeTy::Int {
                attr: TyAttr::default(),
            });
            self.builder.assign(
                Place::local(end),
                Rvalue::BinaryOp {
                    op: BinOp::Sub,
                    left: Operand::Copy(Place::local(len_local)),
                    right: Operand::Copy(Place::local(suffix)),
                },
            );
            end
        };
        let target = self.builder.create_block();
        let unwind = self.catch_context.as_ref().map(|c| c.unwind_target);
        self.builder.call(
            Operand::Constant(Constant::Function(ItemRef::Method {
                package: Name::new("baml"),
                namespace: Vec::new(),
                class: Name::new("Array"),
                name: Name::new("slice"),
            })),
            vec![
                Operand::Copy(Place::local(scrutinee)),
                Operand::Copy(Place::local(start)),
                Operand::Copy(Place::local(end)),
            ],
            Place::local(rest_local),
            target,
            unwind,
        );
        self.builder.set_current_block(target);
        rest_local
    }

    fn lower_pattern_test(
        &mut self,
        scrutinee: Local,
        pat_id: AstPatId,
        success: BlockId,
        failure: BlockId,
    ) {
        let scrutinee = if self.atomic_pattern_test {
            let snapshot = self.builder.temp(self.builder.local_ty(scrutinee));
            self.builder.assign(
                Place::local(snapshot),
                Rvalue::Use(Operand::Copy(Place::Local(scrutinee))),
            );
            self.tested_pattern_values
                .insert(self.pat_metadata_key(pat_id), snapshot);
            snapshot
        } else {
            scrutinee
        };
        let pat = self.body.patterns[pat_id].clone();

        // Bind sub-pattern: `let x: <pattern>` defers to the sub-
        // pattern's runtime test (recursively). The bind itself doesn't
        // emit a runtime check; the sub-pattern does.
        if let AstPattern::Bind {
            subpat: Some(sp), ..
        } = &pat
        {
            let saved = self.atomic_pattern_test;
            self.atomic_pattern_test = true;
            self.lower_pattern_test(scrutinee, *sp, success, failure);
            self.atomic_pattern_test = saved;
            return;
        }
        // Array `: T` ascription emits an `is_type` test before the
        // structural shape test below.
        if let AstPattern::Array {
            ascription: Some(ty_expr),
            ..
        } = &pat
        {
            let after_ascription = self.builder.create_block();
            if let Some(tir_ty) = self
                .tir_pat_type(self.pat_metadata_key(pat_id))
                .filter(|ty| !matches!(ty, Tir2Ty::Never { .. }))
                .cloned()
            {
                self.emit_is_tir_type_branch(scrutinee, &tir_ty, after_ascription, failure);
            } else {
                let annotation_ty = self.resolve_type_annotation(ty_expr);
                self.emit_is_type_branch(scrutinee, annotation_ty, after_ascription, failure);
            }
            self.builder.set_current_block(after_ascription);
            // Fall through to the array shape test below.
        }

        match &pat {
            AstPattern::Wildcard => {
                self.builder.goto(success);
            }
            AstPattern::Bind { .. } => {
                // A bare `let e` (no annotation — annotated binds carry the
                // annotation as a subpattern and recursed above) is
                // IRREFUTABLE: arm dispatch is sequential, so the bind takes
                // whatever reaches it; its `pat_types` entry is exhaustiveness
                // bookkeeping, not a runtime dispatch condition. Emitting a
                // type test here is at best a tautology and at worst a
                // miscompile: a rigid generic (e.g. the `E` of a combinator's
                // `catch (e) { let e => … }`) erases to `RuntimeTy::Void` in
                // `convert_tir_ty_for_runtime`, making the test constant-false and the
                // catch arm silently rethrow. (Panic fall-through for catch
                // arms is handled separately by `ThrowIfPanic`.)
                self.builder.goto(success);
            }
            // OLD's Pattern::Type covered structural shape tests; OLD's
            // Pattern::Literal / Pattern::Null / Pattern::EnumVariant were
            // separate variants. The new flat enum collapses all of those
            // into `Pattern::Type(TypeExpr)`, so we dispatch on the inner
            // TypeExpr to recover OLD's per-kind codegen.
            AstPattern::Type(ty_expr) => match &ty_expr.kind {
                AstTypeExprKind::Literal { value: lit, .. } => {
                    let constant = Self::lower_literal(lit);
                    let test = Rvalue::BinaryOp {
                        op: BinOp::Eq,
                        left: Operand::Copy(Place::Local(scrutinee)),
                        right: Operand::Constant(constant),
                    };
                    let test_local = self.builder.temp(RuntimeTy::Bool {
                        attr: TyAttr::default(),
                    });
                    self.builder.assign(Place::local(test_local), test);
                    self.builder
                        .branch(Operand::Copy(Place::Local(test_local)), success, failure);
                }
                AstTypeExprKind::Null { .. } => {
                    let test = Rvalue::BinaryOp {
                        op: BinOp::Eq,
                        left: Operand::Copy(Place::Local(scrutinee)),
                        right: Operand::Constant(Constant::Null),
                    };
                    let test_local = self.builder.temp(RuntimeTy::Bool {
                        attr: TyAttr::default(),
                    });
                    self.builder.assign(Place::local(test_local), test);
                    self.builder
                        .branch(Operand::Copy(Place::Local(test_local)), success, failure);
                }
                AstTypeExprKind::Path { .. }
                    if matches!(
                        self.tir_pat_type(self.pat_metadata_key(pat_id)),
                        Some(Tir2Ty::EnumVariant(_, _, _))
                    ) =>
                {
                    let Some(Tir2Ty::EnumVariant(qtn, variant, _)) =
                        self.tir_pat_type(self.pat_metadata_key(pat_id))
                    else {
                        unreachable!("guarded by matches! above");
                    };
                    let enum_ref = ItemRef::EnumType {
                        package: qtn.package().clone(),
                        namespace: qtn.namespace().clone(),
                        name: qtn.name().clone(),
                    };
                    let variant = variant.clone();
                    let test = Rvalue::BinaryOp {
                        op: BinOp::Eq,
                        left: Operand::Copy(Place::Local(scrutinee)),
                        right: Operand::Constant(Constant::EnumVariant { enum_ref, variant }),
                    };
                    let test_local = self.builder.temp(RuntimeTy::Bool {
                        attr: TyAttr::default(),
                    });
                    self.builder.assign(Place::local(test_local), test);
                    self.builder
                        .branch(Operand::Copy(Place::Local(test_local)), success, failure);
                }
                _ => {
                    // The annotated-bind recursion (`let e: T => …` recurses
                    // into its Type subpattern) has no `pat_types` entry for
                    // the subpattern, so fall back to lowering the annotation
                    // itself with the enclosing generic params in scope.
                    let pat_tir_ty = self
                        .tir_pat_type(self.pat_metadata_key(pat_id))
                        .cloned()
                        .unwrap_or_else(|| self.lower_type_annotation_tir(ty_expr));
                    // A generic-interface pattern (`Slot<int>`) needs the
                    // TIR-typed test, which preserves the type argument and
                    // tests only the implementors of *that* instantiation —
                    // otherwise the erased path matches every implementor and a
                    // `Slot<string>` value falls into a `Slot<int>` arm.
                    if Self::tir_ty_needs_interface_shape_test(&pat_tir_ty) {
                        self.emit_is_tir_type_branch(scrutinee, &pat_tir_ty, success, failure);
                        return;
                    }
                    // A pattern type still carrying the enclosing function's
                    // TypeVars — a bare `T`, `T[]`, `map<_, T>`, a class like
                    // `AllFailed<E>` inside `any<T, E>`, or a union thereof —
                    // must NOT go through `convert_tir_ty_for_runtime`: that
                    // erases TypeVar → Void and the test becomes constant-false
                    // (a silent arm miss). Build a template instead so each
                    // typevar resolves against `frame.type_args` at runtime and
                    // the value is compared against the *realized* binding
                    // (TYPE_SYSTEM.md "Type Variables": at any run-time usage
                    // site a type variable corresponds to exactly one realized
                    // type).
                    //
                    // Each frame `TypeVar` lowers to a `TypeArgRef` and is
                    // compared *invariantly* against the value's realized arg by
                    // the canonical algebra. When inference pins `T` to a supertype
                    // union of the value's actual arg (e.g. a `default: T` arg
                    // subtypes, so `T` reifies to the un-subsumed join `Shape | Sq`
                    // while the value is `Opt<Shape>`), the arm still matches: the
                    // algebra knows `Sq <: Shape` and absorbs `Shape | Sq == Shape`,
                    // so invariant equivalence holds. (The old context-free
                    // comparison could not see that membership, which is why this
                    // used to need a covariant `TypeArgRefOrWildcard` band-aid.)
                    //
                    // `tir2_to_pattern_template` (rather than
                    // `tir2_to_template`) is used because it reports an
                    // unresolvable position — an associated projection, a
                    // slotless type variable — as `None`, which
                    // `emit_pattern_template_test` fails closed, instead of
                    // panicking or erasing.
                    //
                    // `Self` is an ordinary frame type variable here: an
                    // interface-owned body carries it at frame slot 0 (see
                    // `enclosing_generic_params`), so a `Self`-carrying
                    // pattern tests the receiver's realized concrete type.
                    // Class and impl bodies never surface a `TypeVar(Self)`
                    // pattern type (TIR lowers their `Self` to the concrete
                    // receiver), so any residual slotless `Self` is
                    // unresolvable and fails closed like a projection.
                    if baml_type_runtime::contains_typevar(&pat_tir_ty) {
                        let generic_params = self.enclosing_generic_params();
                        let guard = tir2_to_pattern_template(
                            &pat_tir_ty,
                            self.resolved_aliases,
                            &generic_params,
                        );
                        self.emit_pattern_template_test(scrutinee, guard, success, failure);
                        return;
                    }
                    // Other patterns keep the erased fast path (unchanged codegen).
                    let annotation_ty = self.resolved_aliases.convert(&pat_tir_ty);
                    self.emit_is_type_branch(scrutinee, annotation_ty, success, failure);
                }
            },
            AstPattern::Unreflect(type_expr) => {
                let type_value = self.lower_to_operand(*type_expr);
                let test = Rvalue::RuntimeIsType {
                    operand: Operand::Copy(Place::Local(scrutinee)),
                    type_value,
                };
                let test_local = self.builder.temp(RuntimeTy::Bool {
                    attr: TyAttr::default(),
                });
                self.builder.assign(Place::local(test_local), test);
                self.builder
                    .branch(Operand::Copy(Place::Local(test_local)), success, failure);
            }
            AstPattern::Or(sub_pats) => {
                if sub_pats.is_empty() {
                    self.builder.goto(failure);
                    return;
                }
                let n = sub_pats.len();
                for (i, &sub_pat) in sub_pats.iter().enumerate() {
                    let next = if i + 1 < n {
                        self.builder.create_block()
                    } else {
                        failure
                    };
                    self.lower_pattern_test(scrutinee, sub_pat, success, next);
                    if i + 1 < n {
                        self.builder.set_current_block(next);
                    }
                }
            }
            AstPattern::Class { .. } => {
                let class_success = if self.class_pattern_fields(pat_id).is_empty() {
                    success
                } else {
                    self.builder.create_block()
                };

                if let Some(tir_ty) = self.tir_pat_type(self.pat_metadata_key(pat_id)).cloned() {
                    self.emit_is_tir_type_branch(scrutinee, &tir_ty, class_success, failure);
                } else if class_success == success {
                    self.builder.goto(success);
                } else {
                    self.builder.goto(class_success);
                }

                if class_success != success {
                    self.builder.set_current_block(class_success);
                    let fields = self.class_pattern_fields(pat_id);
                    for (idx, field) in fields.iter().enumerate() {
                        let next = if idx + 1 == fields.len() {
                            success
                        } else {
                            self.builder.create_block()
                        };
                        if let Some(field_local) = self.project_class_pattern_field(
                            scrutinee,
                            pat_id,
                            field.pat,
                            &field.field,
                        ) {
                            self.lower_pattern_test(field_local, field.pat, next, failure);
                        } else {
                            self.builder.goto(failure);
                        }
                        if idx + 1 < fields.len() {
                            self.builder.set_current_block(next);
                        }
                    }
                }
            }
            AstPattern::Array {
                prefix,
                rest,
                suffix,
                ascription: _,
            } => {
                let array_success = self.builder.create_block();

                if let Some(tir_ty) = self.tir_pat_type(self.pat_metadata_key(pat_id)).cloned() {
                    self.emit_is_tir_type_branch(scrutinee, &tir_ty, array_success, failure);
                } else {
                    self.builder.goto(array_success);
                }

                self.builder.set_current_block(array_success);
                // A rest sub-pattern needs a test-phase slice projection only
                // when it is refutable (e.g. a `: T` ascription link). Plain
                // bindings and `.._` match unconditionally; bindings get
                // their slice in the arm-binding phase instead, so testing
                // here would just copy the middle twice.
                let rest_test_pat = rest
                    .as_ref()
                    .and_then(|r| r.pat)
                    .filter(|p| !self.is_irrefutable_catch_all(*p));
                let has_rest_test = rest_test_pat.is_some();
                let element_count = prefix.len() + suffix.len();
                let has_nested_tests = element_count > 0 || has_rest_test;
                let after_len = if has_nested_tests {
                    self.builder.create_block()
                } else {
                    success
                };
                self.lower_array_pattern_length_test(
                    scrutinee,
                    rest.is_some(),
                    prefix.len() + suffix.len(),
                    after_len,
                    failure,
                );
                if !has_nested_tests {
                    return;
                }

                self.builder.set_current_block(after_len);
                let rest_entry = has_rest_test.then(|| self.builder.create_block());
                let element_success = rest_entry.unwrap_or(success);
                if element_count == 0 {
                    self.builder.goto(element_success);
                }

                for (idx, elem_pat) in prefix.iter().copied().enumerate() {
                    let next = if idx + 1 == element_count {
                        element_success
                    } else {
                        self.builder.create_block()
                    };
                    let elem_local =
                        self.project_array_pattern_element_from_start(scrutinee, elem_pat, idx);
                    self.lower_pattern_test(elem_local, elem_pat, next, failure);
                    if idx + 1 < element_count {
                        self.builder.set_current_block(next);
                    }
                }

                for (suffix_idx, elem_pat) in suffix.iter().copied().enumerate() {
                    let absolute_idx_from_end = suffix.len() - suffix_idx;
                    let elem_idx = prefix.len() + suffix_idx;
                    let next = if elem_idx + 1 == element_count {
                        element_success
                    } else {
                        self.builder.create_block()
                    };
                    let elem_local = self.project_array_pattern_element_from_end(
                        scrutinee,
                        elem_pat,
                        absolute_idx_from_end,
                    );
                    self.lower_pattern_test(elem_local, elem_pat, next, failure);
                    if elem_idx + 1 < element_count {
                        self.builder.set_current_block(next);
                    }
                }

                if let Some(rest_pat) = rest_test_pat {
                    if let Some(rest_entry) = rest_entry {
                        self.builder.set_current_block(rest_entry);
                    }
                    let rest_local = self.project_array_pattern_rest(
                        scrutinee,
                        rest_pat,
                        prefix.len(),
                        suffix.len(),
                    );
                    self.lower_pattern_test(rest_local, rest_pat, success, failure);
                }
            }
        }
    }

    fn is_irrefutable_catch_all(&self, pat_id: AstPatId) -> bool {
        match &self.body.patterns[pat_id] {
            AstPattern::Wildcard => true,
            // `let x` is irrefutable; `let x: <pat>` is refutable iff
            // the inner sub-pattern is.
            AstPattern::Bind { subpat, .. } => match subpat {
                None => true,
                Some(sp) => self.is_irrefutable_catch_all(*sp),
            },
            AstPattern::Or(parts) => parts
                .iter()
                .any(|part| self.is_irrefutable_catch_all(*part)),
            AstPattern::Type(_)
            | AstPattern::Unreflect(_)
            | AstPattern::Class { .. }
            | AstPattern::Array { .. } => false,
        }
    }

    /// Type ascription on the pattern, if any. For `let x: T` (where the
    /// sub-pattern is a `Type`), returns `T`. For `[…]: T` (Array with
    /// ascription), returns `T`. Returns `None` for everything else
    /// (including `let x: <non-type-pattern>`).
    fn pattern_narrow_type(&self, pat_id: AstPatId) -> Option<AstTypeExpr> {
        match &self.body.patterns[pat_id] {
            AstPattern::Bind {
                subpat: Some(sp), ..
            } => match &self.body.patterns[*sp] {
                AstPattern::Type(t) => Some(t.clone()),
                _ => None,
            },
            AstPattern::Array {
                ascription: Some(t),
                ..
            } => Some(t.clone()),
            _ => None,
        }
    }

    fn bind_pattern(&mut self, scrutinee: Local, pat_id: AstPatId) {
        // Pass the root pat_id through recursion: HIR registers bindings
        // keyed by the OUTER pattern PatId (the let-stmt's pattern, the
        // match-arm's pattern, etc.), never by the inner Bind. To wire up
        // closure capture lookups correctly, we register the local against
        // that root.
        self.bind_pattern_inner(scrutinee, pat_id, pat_id, pat_id, false);
    }

    fn bind_pattern_with_fresh_cells(&mut self, scrutinee: Local, pat_id: AstPatId) {
        self.bind_pattern_inner(scrutinee, pat_id, pat_id, pat_id, true);
    }

    fn bind_pattern_inner(
        &mut self,
        scrutinee: Local,
        pat_id: AstPatId,
        root: AstPatId,
        narrow_root: AstPatId,
        fresh_cell: bool,
    ) {
        let scrutinee = self
            .tested_pattern_values
            .get(&self.pat_metadata_key(pat_id))
            .copied()
            .unwrap_or(scrutinee);
        match self.body.patterns[pat_id].clone() {
            AstPattern::Bind { name, subpat } => {
                let bound_scrutinee = subpat
                    .and_then(|subpat| {
                        self.tested_pattern_values
                            .get(&self.pat_metadata_key(subpat))
                            .copied()
                    })
                    .unwrap_or(scrutinee);
                // For Or-patterns we look up `pat_types` against the inner
                // bind's `pat_id`, not the outer `root`. That's safe because
                // TIR rejects Or-branches whose shared bindings disagree on
                // type (`OrPatternBindingTypeMismatch`), so by the time we
                // reach MIR every alternative's bind for `name` carries the
                // same type. If you ever loosen that TIR invariant, switch
                // this lookup to `root` so we don't over-narrow.
                let narrow = self.pattern_narrow_type(narrow_root);
                let ty = if let Some(narrow) = &narrow {
                    self.resolve_type_annotation(narrow)
                } else {
                    self.tir_pat_type(self.pat_metadata_key(pat_id))
                        .map(|ty| self.resolved_aliases.convert(ty))
                        .unwrap_or_else(|| self.builder.local_ty(scrutinee))
                };
                let local = self.builder.declare_local(Some(name.clone()), ty, None);
                if fresh_cell {
                    self.builder.fresh_cell(local);
                }
                self.builder.assign(
                    Place::local(local),
                    Rvalue::Use(Operand::Copy(Place::Local(bound_scrutinee))),
                );
                self.record_pattern_binding_local(root, &name, local);
                self.locals.insert(name, local);
                // Recurse into the sub-pattern so inner bindings (e.g.
                // `let x: let y` or `let x: Class { f }`) get emitted too.
                if let Some(sp) = subpat {
                    self.bind_pattern_inner(bound_scrutinee, sp, root, sp, fresh_cell);
                }
            }
            AstPattern::Or(parts) => {
                let mut bindings = Vec::new();
                self.collect_pattern_bindings(pat_id, &mut bindings);
                if bindings.is_empty() {
                    return;
                }
                self.declare_or_pattern_bindings(pat_id, root, fresh_cell);
                self.lower_or_pattern_assign_existing(scrutinee, &parts, root, narrow_root);
            }
            AstPattern::Class { fields, .. } => {
                for f in fields {
                    if let Some(field_local) =
                        self.project_class_pattern_field(scrutinee, pat_id, f.pat, &f.field)
                    {
                        self.bind_pattern_inner(field_local, f.pat, root, f.pat, fresh_cell);
                    }
                }
            }
            AstPattern::Array {
                prefix,
                rest,
                suffix,
                ascription: _,
            } => {
                for (idx, elem_pat) in prefix.iter().copied().enumerate() {
                    let elem_local =
                        self.project_array_pattern_element_from_start(scrutinee, elem_pat, idx);
                    self.bind_pattern_inner(elem_local, elem_pat, root, elem_pat, fresh_cell);
                }
                if let Some(rest) = rest
                    && let Some(rest_pat) = rest.pat
                    // Wildcard rests bind nothing; skip the slice copy.
                    && !matches!(self.body.patterns[rest_pat], AstPattern::Wildcard)
                {
                    let rest_local = self.project_array_pattern_rest(
                        scrutinee,
                        rest_pat,
                        prefix.len(),
                        suffix.len(),
                    );
                    self.bind_pattern_inner(rest_local, rest_pat, root, rest_pat, fresh_cell);
                }
                for (suffix_idx, elem_pat) in suffix.iter().copied().enumerate() {
                    let absolute_idx_from_end = suffix.len() - suffix_idx;
                    let elem_local = self.project_array_pattern_element_from_end(
                        scrutinee,
                        elem_pat,
                        absolute_idx_from_end,
                    );
                    self.bind_pattern_inner(elem_local, elem_pat, root, elem_pat, fresh_cell);
                }
            }
            AstPattern::Wildcard | AstPattern::Type(_) | AstPattern::Unreflect(_) => {}
        }
    }

    fn collect_pattern_bindings(&self, pat_id: AstPatId, out: &mut Vec<(Name, AstPatId)>) {
        match self.body.patterns[pat_id].clone() {
            AstPattern::Bind { name, subpat } => {
                out.push((name, pat_id));
                if let Some(sp) = subpat {
                    self.collect_pattern_bindings(sp, out);
                }
            }
            AstPattern::Or(parts) => {
                if let Some(first) = parts.first() {
                    self.collect_pattern_bindings(*first, out);
                }
            }
            AstPattern::Class { fields, .. } => {
                for field in fields {
                    self.collect_pattern_bindings(field.pat, out);
                }
            }
            AstPattern::Array {
                prefix,
                rest,
                suffix,
                ascription: _,
            } => {
                for part in prefix {
                    self.collect_pattern_bindings(part, out);
                }
                if let Some(rest) = rest
                    && let Some(rest_pat) = rest.pat
                {
                    self.collect_pattern_bindings(rest_pat, out);
                }
                for part in suffix {
                    self.collect_pattern_bindings(part, out);
                }
            }
            AstPattern::Wildcard | AstPattern::Type(_) | AstPattern::Unreflect(_) => {}
        }
    }

    fn declare_or_pattern_bindings(&mut self, pat_id: AstPatId, root: AstPatId, fresh_cell: bool) {
        let mut bindings = Vec::new();
        self.collect_pattern_bindings(pat_id, &mut bindings);
        for (name, bind_pat) in bindings {
            let local = self
                .builder
                .declare_local(Some(name.clone()), self.pat_ty(bind_pat), None);
            if fresh_cell {
                self.builder.fresh_cell(local);
            }
            self.record_pattern_binding_local(root, &name, local);
            self.locals.insert(name, local);
        }
    }

    fn lower_or_pattern_assign_existing(
        &mut self,
        scrutinee: Local,
        parts: &[AstPatId],
        root: AstPatId,
        narrow_root: AstPatId,
    ) {
        if parts.is_empty() {
            self.builder.unreachable();
            return;
        }

        let join = self.builder.create_block();
        let failure = self.builder.create_block();

        for (idx, part) in parts.iter().copied().enumerate() {
            let body = self.builder.create_block();
            let next = if idx + 1 == parts.len() {
                failure
            } else {
                self.builder.create_block()
            };
            self.lower_pattern_test(scrutinee, part, body, next);

            self.builder.set_current_block(body);
            self.assign_pattern_to_existing(scrutinee, part, root, narrow_root);
            if !self.builder.is_current_terminated() {
                self.builder.goto(join);
            }

            if idx + 1 < parts.len() {
                self.builder.set_current_block(next);
            }
        }

        self.builder.set_current_block(failure);
        self.builder.unreachable();
        self.builder.set_current_block(join);
    }

    fn assign_pattern_to_existing(
        &mut self,
        scrutinee: Local,
        pat_id: AstPatId,
        root: AstPatId,
        narrow_root: AstPatId,
    ) {
        match self.body.patterns[pat_id].clone() {
            AstPattern::Bind { name, .. } => {
                if let Some(&local) = self.locals.get(&name) {
                    self.builder.assign(
                        Place::local(local),
                        Rvalue::Use(Operand::Copy(Place::Local(scrutinee))),
                    );
                    self.record_pattern_binding_local(root, &name, local);
                }
            }
            AstPattern::Or(parts) => {
                self.lower_or_pattern_assign_existing(scrutinee, &parts, root, narrow_root);
            }
            AstPattern::Class { fields, .. } => {
                for field in fields {
                    if let Some(field_local) =
                        self.project_class_pattern_field(scrutinee, pat_id, field.pat, &field.field)
                    {
                        self.assign_pattern_to_existing(field_local, field.pat, root, field.pat);
                    }
                }
            }
            AstPattern::Array {
                prefix,
                rest,
                suffix,
                ascription: _,
            } => {
                for (idx, elem_pat) in prefix.iter().copied().enumerate() {
                    let elem_local =
                        self.project_array_pattern_element_from_start(scrutinee, elem_pat, idx);
                    self.assign_pattern_to_existing(elem_local, elem_pat, root, elem_pat);
                }
                if let Some(rest) = rest
                    && let Some(rest_pat) = rest.pat
                {
                    let rest_local = self.project_array_pattern_rest(
                        scrutinee,
                        rest_pat,
                        prefix.len(),
                        suffix.len(),
                    );
                    self.assign_pattern_to_existing(rest_local, rest_pat, root, rest_pat);
                }
                for (suffix_idx, elem_pat) in suffix.iter().copied().enumerate() {
                    let absolute_idx_from_end = suffix.len() - suffix_idx;
                    let elem_local = self.project_array_pattern_element_from_end(
                        scrutinee,
                        elem_pat,
                        absolute_idx_from_end,
                    );
                    self.assign_pattern_to_existing(elem_local, elem_pat, root, elem_pat);
                }
            }
            AstPattern::Wildcard | AstPattern::Type(_) | AstPattern::Unreflect(_) => {}
        }
    }
}

// ─── Type tag classification (shared by match/catch switch dispatch) ──────────

impl LoweringContext<'_> {
    /// Classify a pattern into type tag value(s) for switch dispatch.
    /// Classify a pattern as type-tag-eligible and return its tag(s).
    ///
    /// Shared by match and catch lowering. `scrutinee_static_ty` is the
    /// enclosing match scrutinee's resolved static type when known (`None` on
    /// the catch path, whose error value has no useful static type); the
    /// tag-sufficiency gate uses it to disqualify arms whose coarse tag would
    /// conflate values the scrutinee admits (see
    /// [`Self::ty_to_type_tags_for_switch`]).
    ///
    /// Returns `Some(tags)` for `TypedBinding` and Binding-with-TIR-type patterns
    /// that resolve to primitive or class types. Returns `None` for literals,
    /// wildcards, enum variants, and types without tag mappings.
    fn classify_pattern_type_tag(
        &self,
        pat_id: AstPatId,
        scrutinee_static_ty: Option<&RuntimeTy>,
    ) -> Option<Vec<i64>> {
        let pat = &self.body.patterns[pat_id];
        if self.pattern_contains_structural(pat_id) {
            return None;
        }
        // A generic-interface pattern (`Slot<int>`, or one nested in a union /
        // optional like `Slot<int> | Slot<string>`) cannot be discriminated by a
        // flat type-tag switch: every instantiation shares the bare interface's
        // implementor tags, so arms would collide and the first would wrongly
        // capture all of them. Disqualify the switch (recursively) so the
        // match-chain runtime test — which filters implementors by the specific
        // instantiation — is used instead.
        if self
            .tir_pat_type(self.pat_metadata_key(pat_id))
            .is_some_and(Self::tir_ty_needs_interface_shape_test)
        {
            return None;
        }
        // Bind/Array patterns may carry a `:T` type ascription; resolve
        // via the ascription's TypeExpr if present. For Bind, the
        // ascription is the sub-pattern when it's a `Type(...)` shape.
        let ascription_ty = match pat {
            AstPattern::Bind {
                subpat: Some(sp), ..
            } => match &self.body.patterns[*sp] {
                AstPattern::Type(t) => Some(t.clone()),
                _ => None,
            },
            AstPattern::Array {
                ascription: Some(t),
                ..
            } => Some(t.clone()),
            _ => None,
        };
        if let Some(ty_expr) = ascription_ty {
            if let Some(tir_ty) = self.tir_pat_type(self.pat_metadata_key(pat_id)) {
                let resolved = self.resolved_aliases.convert(tir_ty);
                if let Some(tags) = self.ty_to_type_tags_for_switch(&resolved, scrutinee_static_ty)
                {
                    return Some(tags);
                }
            }
            let resolved = self.resolve_type_annotation(&ty_expr);
            return self.ty_to_type_tags_for_switch(&resolved, scrutinee_static_ty);
        }
        match pat {
            AstPattern::Wildcard => None,
            AstPattern::Bind { .. } => {
                let tir_ty = self.tir_pat_type(self.pat_metadata_key(pat_id))?;
                let resolved = self.resolved_aliases.convert(tir_ty);
                self.ty_to_type_tags_for_switch(&resolved, scrutinee_static_ty)
            }
            AstPattern::Type(_) => {
                if let Some(tir_ty) = self.tir_pat_type(self.pat_metadata_key(pat_id)) {
                    let resolved = self.resolved_aliases.convert(tir_ty);
                    if let Some(tags) =
                        self.ty_to_type_tags_for_switch(&resolved, scrutinee_static_ty)
                    {
                        return Some(tags);
                    }
                }
                if let AstPattern::Type(ty_expr) = pat {
                    let resolved = self.resolve_type_annotation(ty_expr);
                    return self.ty_to_type_tags_for_switch(&resolved, scrutinee_static_ty);
                }
                None
            }
            _ => None,
        }
    }

    /// [`Self::ty_to_type_tags`], additionally requiring every flattened member
    /// of `ty` to be *tag-sufficient* for a switch keyed on those tags (see
    /// [`switch_member_tag_sufficient`]): a coarse tag may only key a jump-table
    /// arm when it cannot conflate values the scrutinee admits onto the wrong
    /// arm — `int[]` and `string[]` share the LIST tag, `Foo<int>` and
    /// `Foo<string>` share `Foo`'s tag, and the tag dedup would silently drop
    /// the second arm. An insufficient member disqualifies the pattern
    /// (→ `None`), which makes `try_lower_as_switch` fall back to the precise
    /// sequential chain.
    fn ty_to_type_tags_for_switch(
        &self,
        ty: &RuntimeTy,
        scrutinee_static_ty: Option<&RuntimeTy>,
    ) -> Option<Vec<i64>> {
        let tags = self.ty_to_type_tags(ty)?;
        let mut members = Vec::new();
        flatten_runtime_union(ty, &mut members);
        members
            .iter()
            .all(|m| switch_member_tag_sufficient(m, scrutinee_static_ty))
            .then_some(tags)
    }

    /// Convert a `RuntimeTy` to the list of type tag integers it corresponds to.
    /// Returns `None` if the type has no simple tag representation.
    ///
    /// Supports primitives (globally-stable tags) and class types (looked up
    /// from `class_type_tags`). Union types are flattened — all members must
    /// be tag-eligible.
    fn ty_to_type_tags(&self, ty: &RuntimeTy) -> Option<Vec<i64>> {
        match ty {
            RuntimeTy::Union(members, _) => {
                let mut tags = Vec::new();
                for m in members {
                    let member_tags = self.ty_to_type_tags(m)?;
                    tags.extend(member_tags);
                }
                Some(tags)
            }
            _ => self.type_tag_for_ty(ty).map(|tag| vec![tag]),
        }
    }
}

/// Format a type tag integer as a human-readable name for switch arm debug metadata.
fn format_type_tag_name(tag: i64) -> String {
    match tag {
        baml_type::typetag::INT => "int".to_string(),
        baml_type::typetag::BIGINT => "bigint".to_string(),
        baml_type::typetag::STRING => "string".to_string(),
        baml_type::typetag::BOOL => "bool".to_string(),
        baml_type::typetag::NULL => "null".to_string(),
        baml_type::typetag::FLOAT => "float".to_string(),
        baml_type::typetag::LIST => "list".to_string(),
        baml_type::typetag::MAP => "map".to_string(),
        baml_type::typetag::ENUM => "enum".to_string(),
        baml_type::typetag::FUNCTION => "function".to_string(),
        baml_type::typetag::FUTURE => "future".to_string(),
        baml_type::typetag::TYPE => "type".to_string(),
        baml_type::typetag::COLLECTOR => "collector".to_string(),
        baml_type::typetag::UINT8ARRAY => "uint8array".to_string(),
        tag if tag >= baml_type::typetag::CLASS_BASE => {
            format!("class#{}", tag - baml_type::typetag::CLASS_BASE)
        }
        _ => format!("tag#{tag}"),
    }
}

// ─── Catch lowering ───────────────────────────────────────────────────────────

impl LoweringContext<'_> {
    fn lower_catch(
        &mut self,
        _expr_id: AstExprId,
        base: AstExprId,
        clauses: &[baml_compiler2_ast::CatchClause],
        dest: &Place,
    ) {
        use baml_compiler2_ast::CatchClauseKind;

        #[derive(Clone)]
        struct ClauseLocals {
            binding_name: Option<Name>,
            binding_local: Option<Local>,
            binding_copy_local: Option<Local>,
            stack_trace_name: Option<Name>,
            stack_trace_payload: Option<Local>,
            stack_trace_copy_local: Option<Local>,
        }

        fn install_clause_locals(
            ctx: &mut LoweringContext<'_>,
            error_local: Local,
            clause: &ClauseLocals,
        ) {
            if let (Some(name), Some(local)) = (&clause.binding_name, clause.binding_local) {
                ctx.locals.insert(name.clone(), local);
            }
            if let Some(binding_copy_local) = clause.binding_copy_local {
                ctx.builder.assign(
                    Place::local(binding_copy_local),
                    Rvalue::Use(Operand::Copy(Place::Local(error_local))),
                );
            }
            if let (Some(name), Some(local)) =
                (&clause.stack_trace_name, clause.stack_trace_copy_local)
            {
                ctx.locals.insert(name.clone(), local);
            }
            if let (Some(payload), Some(copy_local)) =
                (clause.stack_trace_payload, clause.stack_trace_copy_local)
                && payload != copy_local
            {
                ctx.builder.assign(
                    Place::local(copy_local),
                    Rvalue::Use(Operand::Copy(Place::Local(payload))),
                );
            }
        }

        let saved_catch_outer_locals = self.locals.clone();
        let bb_join = self.builder.create_block();
        let bb_handler = self.builder.create_block();

        // Use the user-provided binding name (e.g. `e` from `catch (e)`) so it
        // shows up in bytecode instead of an anonymous `_N` temp. Only do this
        // for single-clause catches with a non-captured binding.
        let single_clause_binding_name = clauses.first().and_then(|c| {
            if clauses.len() == 1 && !self.catch_binding_is_captured(c.binding) {
                self.body.patterns[c.binding]
                    .binding_name(&self.body.patterns)
                    .cloned()
            } else {
                None
            }
        });
        let error_local = self.builder.declare_local(
            single_clause_binding_name,
            RuntimeTy::BuiltinUnknown {
                attr: TyAttr::default(),
            },
            None,
        );

        let stack_trace_local = clauses
            .iter()
            .any(|c| c.stack_trace_binding.is_some())
            .then(|| {
                self.builder.declare_local(
                    None,
                    RuntimeTy::BuiltinUnknown {
                        attr: TyAttr::default(),
                    },
                    None,
                )
            });

        let mut clause_locals = Vec::with_capacity(clauses.len());
        for clause in clauses {
            let binding_name = self.body.patterns[clause.binding]
                .binding_name(&self.body.patterns)
                .cloned();
            let binding_is_captured = self.catch_binding_is_captured(clause.binding);
            let (binding_local, binding_copy_local) = match binding_name.clone() {
                Some(name) if binding_is_captured => {
                    let local = self.builder.declare_local(
                        Some(name.clone()),
                        RuntimeTy::BuiltinUnknown {
                            attr: TyAttr::default(),
                        },
                        None,
                    );
                    self.record_catch_binding_local(clause.binding, &name, local);
                    (Some(local), Some(local))
                }
                Some(name) => {
                    self.record_catch_binding_local(clause.binding, &name, error_local);
                    (Some(error_local), None)
                }
                None => (None, None),
            };

            let (stack_trace_name, stack_trace_copy_local) = if let (Some(st_pat), Some(payload)) =
                (clause.stack_trace_binding, stack_trace_local)
            {
                let name = self.body.patterns[st_pat]
                    .binding_name(&self.body.patterns)
                    .cloned();
                let is_captured = self.catch_binding_is_captured(st_pat);
                match name.clone() {
                    Some(name) if is_captured => {
                        let local = self.builder.declare_local(
                            Some(name.clone()),
                            RuntimeTy::BuiltinUnknown {
                                attr: TyAttr::default(),
                            },
                            None,
                        );
                        self.record_catch_binding_local(st_pat, &name, local);
                        (Some(name), Some(local))
                    }
                    Some(name) => {
                        self.record_catch_binding_local(st_pat, &name, payload);
                        (Some(name), Some(payload))
                    }
                    None => (None, None),
                }
            } else {
                (None, None)
            };

            clause_locals.push(ClauseLocals {
                binding_name,
                binding_local,
                binding_copy_local,
                stack_trace_name,
                stack_trace_payload: stack_trace_local,
                stack_trace_copy_local,
            });
        }

        // Flatten all arms from all clauses (blocks created lazily below).
        let mut arms: Vec<(baml_compiler2_ast::CatchArm, bool, usize)> = Vec::new();
        for (clause_idx, clause) in clauses.iter().enumerate() {
            for &arm_id in &clause.arms {
                let arm = self.body.catch_arms[arm_id].clone();
                let is_wildcard = self.is_irrefutable_catch_all(arm.pattern);
                arms.push((arm, is_wildcard, clause_idx));
            }
        }

        let has_wildcard = arms.iter().any(|(_, is_wc, _)| *is_wc);
        let is_catch_all_panics = clauses
            .iter()
            .any(|clause| matches!(clause.kind, CatchClauseKind::CatchAllPanics));

        // Record the catch region (always one handler, one exception table entry).
        // `handler_body` is filled in after the arms are lowered (below): the
        // blocks created while lowering the arms ARE the handler body, and they
        // can be laid out non-contiguously, so `[handler, join)` is not enough.
        let body_entry = self.builder.current_block();
        let catch_region_idx = self.builder.catch_regions.len();
        self.builder.catch_regions.push(CatchRegion {
            body_entry,
            handler: bb_handler,
            handler_body: vec![bb_handler],
            error_local,
            stack_trace_local,
        });

        let prev_catch = self.catch_context.take();
        self.catch_context = Some(CatchContext {
            unwind_target: bb_handler,
            error_local,
        });

        // Lower the try body.
        self.lower_expr(base, dest.clone());
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        self.catch_context = prev_catch;

        // Before the wildcard arm (if any), insert a throw_if_panic guard to
        // prevent the wildcard from swallowing panics the programmer didn't
        // explicitly name. Skipped for catch_all_panics (user wants everything).
        let needs_throw_if_panic = has_wildcard && !is_catch_all_panics;

        // Attempt switch-style dispatch on type tags.
        // If all non-wildcard arms have pure type-test patterns, emit a single
        // Switch on Rvalue::TypeTag instead of a sequential is_type chain.
        let switch_arms: Vec<(AstPatId, AstExprId, Option<AstExprId>)> = arms
            .iter()
            .map(|(arm, _, _)| (arm.pattern, arm.body, None))
            .collect();
        // Everything created from here until the join belongs to the handler
        // body (the arms), captured into the catch region for the cause chain.
        let arm_blocks_lo = self.builder.num_blocks();
        self.builder.set_current_block(bb_handler);
        if clauses.len() == 1 {
            install_clause_locals(self, error_local, &clause_locals[0]);
        }
        let switch_rethrow_mark = self.catch_rethrow_locals.len();
        if clauses.len() == 1 {
            self.catch_rethrow_locals.push(error_local);
            if let Some(local) = clause_locals[0].binding_copy_local {
                self.catch_rethrow_locals.push(local);
            }
        }
        let lowered_as_switch = clauses.len() == 1
            && self.try_lower_as_switch(
                error_local,
                &switch_arms,
                dest.clone(),
                bb_join,
                SwitchOtherwise::Catch {
                    error_local,
                    needs_throw_if_panic,
                },
                None,
            );
        self.catch_rethrow_locals.truncate(switch_rethrow_mark);
        if lowered_as_switch {
            self.builder.catch_regions[catch_region_idx].handler_body = std::iter::once(bb_handler)
                .chain((arm_blocks_lo..self.builder.num_blocks()).map(BlockId))
                .collect();
            self.builder.set_current_block(bb_join);
            self.restore_active_locals(saved_catch_outer_locals);
            return;
        }

        // Fallback: sequential pattern-test chain.
        // Create body blocks now (not created earlier so the switch path
        // doesn't leave orphaned unterminated blocks).
        let arms_with_blocks: Vec<_> = arms
            .iter()
            .map(|(arm, is_wc, clause_idx)| {
                (
                    arm.clone(),
                    self.builder.create_block(),
                    *is_wc,
                    *clause_idx,
                )
            })
            .collect();

        for &(ref arm, body_block, is_wildcard, _) in &arms_with_blocks {
            if is_wildcard && needs_throw_if_panic {
                let bb_wildcard = self.builder.create_block();
                self.builder
                    .throw_if_panic(Operand::Copy(Place::Local(error_local)), bb_wildcard);
                self.builder.set_current_block(bb_wildcard);
            }

            let bb_arm_next = self.builder.create_block();
            self.lower_pattern_test(error_local, arm.pattern, body_block, bb_arm_next);
            self.builder.set_current_block(bb_arm_next);
        }

        // Rethrow if nothing matched.
        if !self.builder.is_current_terminated() {
            self.builder
                .rethrow(Operand::Copy(Place::Local(error_local)));
        }

        // Lower each arm body.
        for &(ref arm, body_block, _, clause_idx) in &arms_with_blocks {
            self.builder.set_current_block(body_block);
            let saved_locals = self.locals.clone();
            let clause = clause_locals[clause_idx].clone();
            install_clause_locals(self, error_local, &clause);
            self.bind_pattern(error_local, arm.pattern);
            let rethrow_mark = self.catch_rethrow_locals.len();
            self.catch_rethrow_locals.push(error_local);
            if let Some(local) = clause.binding_copy_local {
                self.catch_rethrow_locals.push(local);
            }
            self.lower_expr(arm.body, dest.clone());
            self.catch_rethrow_locals.truncate(rethrow_mark);
            if !self.builder.is_current_terminated() {
                self.builder.goto(bb_join);
            }
            self.restore_locals_after_scope(saved_locals);
        }

        self.builder.catch_regions[catch_region_idx].handler_body = std::iter::once(bb_handler)
            .chain((arm_blocks_lo..self.builder.num_blocks()).map(BlockId))
            .collect();
        self.builder.set_current_block(bb_join);
        self.restore_active_locals(saved_catch_outer_locals);
    }
}

// ─── 3.7: Entry points ────────────────────────────────────────────────────────

/// Lower a top-level let binding's initializer into a `MirFunctionBody`.
///
/// The body has arity 0 and contains only the initializer expression.
/// Used by `compile_init_function` in the emit crate to compile let initializers
/// into bytecode for the `$init` function.
pub fn lower_let_body<'db>(
    db: &'db dyn crate::Db,
    let_loc: LetLoc<'db>,
    opt: crate::OptLevel,
) -> Option<(MirFunctionBody, Vec<MirFunction>)> {
    lower_let_body_impl(db, let_loc, opt)
}

fn lower_let_body_impl<'db>(
    db: &'db dyn crate::Db,
    let_loc: LetLoc<'db>,
    opt: crate::OptLevel,
) -> Option<(MirFunctionBody, Vec<MirFunction>)> {
    let body = let_body(db, let_loc);
    let source_map = let_body_source_map(db, let_loc);

    match body.as_ref() {
        LetBody::Expr(expr_body) => {
            let mut ctx =
                LoweringContext::new_for_let(db, let_loc, expr_body.clone(), source_map, opt);
            let mir_body = ctx.lower_let_body_inner();
            let lambdas = std::mem::take(&mut ctx.pending_lambdas);
            Some((mir_body, lambdas))
        }
        LetBody::Missing => None,
    }
}

pub fn lower_function<'db>(
    db: &'db dyn crate::Db,
    func_loc: FunctionLoc<'db>,
    opt: crate::OptLevel,
) -> MirFunction {
    lower_function_impl(db, func_loc, opt)
}

fn lower_function_impl<'db>(
    db: &'db dyn crate::Db,
    func_loc: FunctionLoc<'db>,
    opt: crate::OptLevel,
) -> MirFunction {
    let body = baml_compiler2_ppir::function_body(db, func_loc);
    let source_map = baml_compiler2_ppir::function_body_source_map(db, func_loc);
    let item_ref = def_to_item_ref(
        db,
        baml_compiler2_hir::contributions::Definition::Function(func_loc),
    );
    let sig = baml_compiler2_ppir::function_signature(db, func_loc);
    let arity = sig.params.len();

    match body.as_ref() {
        FunctionBody::Expr(expr_body) => {
            let mut ctx = LoweringContext::new(db, func_loc, expr_body.clone(), source_map, opt);
            let mut mir = ctx.lower_function_body();
            mir.item_ref = item_ref;
            mir
        }
        FunctionBody::Builtin(kind) => {
            use baml_compiler2_ast::BuiltinKind;
            // For IO builtins (`$rust_io_function`), the compiler injects one
            // synthetic trailing value-arg slot for each generic type parameter
            // (e.g. `parse<T>` gets one extra `baml_type::RuntimeTy` slot after the
            // regular params).  We must include those synthetic slots in the
            // arity so that `ScheduleFuture` pops the correct number of args
            // from the stack.
            let extra_arity = if matches!(kind, BuiltinKind::Io) {
                // For IO builtins (`$rust_io_function`), the compiler injects
                // one synthetic trailing value-arg slot for each *function-level*
                // generic type parameter.  Class-level generics (from the
                // enclosing class definition) do NOT generate extra slots —
                // `baml_builtins2_codegen` only adds type-arg params for
                // function-level generics.  We therefore only count the
                // function's own generic_params here.
                baml_compiler2_ppir::item_data::function_data(db, func_loc)
                    .generic_params
                    .len()
            } else {
                0
            };
            MirFunction {
                arity: arity + extra_arity,
                span: None,
                item_ref,
                kind: MirFunctionKind::Builtin(*kind),
                lambdas: vec![],
                signature: None,
            }
        }
        FunctionBody::Missing => MirFunction {
            arity,
            span: None,
            item_ref,
            kind: MirFunctionKind::Bytecode(MirFunctionBody {
                blocks: vec![BasicBlock {
                    id: BlockId(0),
                    statements: vec![],
                    terminator: Some(Terminator::Unreachable),
                    span: None,
                    terminator_span: None,
                }],
                entry: BlockId(0),
                locals: (0..=arity)
                    .map(|_| LocalDecl {
                        name: None,
                        ty: baml_type::RuntimeTy::Void {
                            attr: baml_type::TyAttr::default(),
                        },
                        is_captured: false,
                        span: None,
                        scope_span: None,
                    })
                    .collect(),
                catch_regions: vec![],
                viz_nodes: vec![],
            }),
            lambdas: vec![],
            signature: None,
        },
    }
}
