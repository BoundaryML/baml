//! Interface member/method resolution during type inference.
//!
//! Resolves a `member` access on an **existential / type-var** receiver: it walks the
//! interface bound's `requires` closure and resolves the member on each interface in it
//! (concrete receivers resolve via their own impls — handled elsewhere). The entry point
//! is [`TypeInferenceBuilder::resolve_interface_member`]; the rest of this module is its
//! supporting machinery (the per-interface resolver, the normalized method spec, and the
//! `Ty::Function` builder).
//!
//! This is an `impl TypeInferenceBuilder` continuation split out of `builder.rs` for size;
//! it draws the inference vocabulary it needs from the parent module.
use super::{
    Definition, ExprId, FunctionParamMode, FunctionParamTy, InterfaceBindingInputs, InterfaceBound,
    MemberAccess, Name, NormalizeCtx, PackageItems, SelfReceiver, TirTypeError, Ty, TyAttr,
    TypeInferenceBuilder, format_interface_display, function_generic_param_bounds_exprs,
    lower_generic_param_bounds,
};

/// One interface instantiation being resolved against: its loc, the applied generic args
/// and associated bindings, and the projection-qualification flags. Computed once per
/// interface in [`TypeInferenceBuilder::resolve_member_on_one_interface`].
struct OneInterface<'a, 'db> {
    iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    iface_data: &'a baml_compiler2_hir::item_tree::Interface,
    iface_type_args: &'a [Ty],
    iface_associated_bindings: &'a [(Name, Ty)],
    current_iface_qtn: &'a crate::ty::QualifiedTypeName,
    iface_ns: &'a [Name],
    pkg_items: &'db PackageItems<'db>,
    prefer_symbolic_projections: bool,
}

impl<'a, 'db> OneInterface<'a, 'db> {
    fn binding_inputs(&self) -> InterfaceBindingInputs<'a, 'db> {
        InterfaceBindingInputs {
            iface_data: self.iface_data,
            associated_bindings: self.iface_associated_bindings,
            pkg_items: self.pkg_items,
            iface_ns: self.iface_ns,
            prefer_symbolic_projections: self.prefer_symbolic_projections,
        }
    }
}

/// A positional or keyword interface-method parameter. The `self` receiver is an ordinary
/// positional `arg`, desugared to `self: Self` (its `ty` is the `Self` path).
struct InterfaceMethodParam {
    name: Name,
    ty: baml_compiler2_ast::TypeExpr,
}

/// A normalized interface-method signature: default and required methods both reduce to
/// this so one builder produces the `Ty::Function`. Interfaces must label their full
/// contract, so `return_type`/`throws` are required (sourced from the declaration, never
/// inferred from a body).
struct InterfaceMethodSpec {
    /// Positional (required) parameters, in declaration order.
    args: Vec<InterfaceMethodParam>,
    /// Keyword/optional parameters (those with a default).
    kwargs: Vec<InterfaceMethodParam>,
    return_type: baml_compiler2_ast::TypeExpr,
    throws: baml_compiler2_ast::TypeExpr,
    /// Method generic params with their optional interface bound, unified.
    generics: Vec<(Name, Option<baml_compiler2_ast::TypeExpr>)>,
}

impl InterfaceMethodSpec {
    fn from_default<'db>(
        db: &'db dyn crate::Db,
        func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
    ) -> Self {
        let sig = baml_compiler2_ppir::elaborated_function_signature(db, func_loc);
        let (args, kwargs) = split_params(sig.params.iter().map(|p| {
            // The implicit `self` receiver: name "self" with no declared type.
            let is_self = p.name.as_str() == "self"
                && matches!(p.ty, baml_compiler2_ast::TypeExpr::Unknown { .. });
            (is_self, p.has_default, p.name.clone(), p.ty.clone())
        }));
        let generics = sig
            .user_generic_params
            .iter()
            .cloned()
            .zip(function_generic_param_bounds_exprs(db, func_loc))
            .collect();
        Self {
            args,
            kwargs,
            return_type: sig.return_type.clone().unwrap_or_else(unknown_type_expr),
            throws: sig.throws.clone().unwrap_or_else(unknown_type_expr),
            generics,
        }
    }

    fn from_required(sig: &baml_compiler2_hir::item_tree::InterfaceMethodSig) -> Self {
        let (args, kwargs) = split_params(sig.params.iter().map(|p| {
            let is_self = p.name.as_str() == "self" && p.type_expr.is_none();
            let ty = p
                .type_expr
                .as_ref()
                .map(|te| te.expr.clone())
                .unwrap_or_else(unknown_type_expr);
            (is_self, p.default.is_some(), p.name.clone(), ty)
        }));
        let generics = sig
            .generic_params
            .iter()
            .cloned()
            .zip(sig.generic_param_bounds.iter().cloned())
            .collect();
        Self {
            args,
            kwargs,
            return_type: sig
                .return_type
                .as_ref()
                .map(|te| te.expr.clone())
                .unwrap_or_else(unknown_type_expr),
            throws: sig
                .throws
                .as_ref()
                .map(|te| te.expr.clone())
                .unwrap_or_else(unknown_type_expr),
            generics,
        }
    }
}

fn unknown_type_expr() -> baml_compiler2_ast::TypeExpr {
    baml_compiler2_ast::TypeExpr::Unknown { attrs: vec![] }
}

/// Split `(is_self, has_default, name, ty)` tuples into positional args (no default) and
/// keyword/optional kwargs (with a default). The `self` receiver desugars to `self: Self`
/// and stays a positional arg (its declared type is replaced by the `Self` path).
fn split_params(
    params: impl Iterator<Item = (bool, bool, Name, baml_compiler2_ast::TypeExpr)>,
) -> (Vec<InterfaceMethodParam>, Vec<InterfaceMethodParam>) {
    let mut args = Vec::new();
    let mut kwargs = Vec::new();
    for (is_self, has_default, name, ty) in params {
        // `self` is syntax sugar for `self: Self` — until `TypeExpr::Self` exists, desugar
        // to the `Self` path so the receiver flows through normal param lowering.
        let ty = if is_self {
            crate::lower_type_expr::type_expr_for_name(Name::new("Self"))
        } else {
            ty
        };
        let param = InterfaceMethodParam { name, ty };
        if has_default {
            kwargs.push(param);
        } else {
            args.push(param);
        }
    }
    (args, kwargs)
}

/// An interface a concrete receiver implements that declares a queried field, paired with
/// the class field it links to. Returned by
/// [`TypeInferenceBuilder::concrete_interface_field_sources`].
pub(super) struct ConcreteFieldSource<'db> {
    pub(super) iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    pub(super) interface: baml_type::Interface,
    pub(super) class_field: Name,
}

/// One interface a union arm provides (implements or `requires`): its decl loc (the HIR handle
/// for member reads) plus the realized [`baml_type::Interface`] constraint — the interface head
/// with the providing impl's bindings substituted in. A union's member surface is the
/// *intersection* of these across all arms — an existential `dyn (I1 + I2 + ...)` — so an
/// interface is shared only when every arm provides the *same* realized `Interface` (its
/// derived `Eq` keys on name + generics + associated bindings — the existential uniformity
/// constraint — with binding order canonicalized by `Interface::new`).
struct ArmInterface<'db> {
    iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    interface: baml_type::Interface,
}

/// Whether a shared interface declares the queried member as a field or a method — selects
/// the field vs method ambiguity diagnostic when two shared interfaces both declare it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum UnionMemberKind {
    Field,
    Method,
}

/// The outcome of resolving a member on a union receiver. `Resolved` already recorded the
/// virtual access/call; the caller reports the prebuilt `Unresolved`/`Ambiguous` diagnostic
/// against its own span (expression vs path segment).
pub(super) enum UnionMemberResolution {
    Resolved(Ty),
    Unresolved(TirTypeError),
    Ambiguous(TirTypeError),
}

impl<'db> TypeInferenceBuilder<'db> {
    /// a matching field or method. Default methods (with bodies) are
    /// resolved via `FunctionLoc` like class methods. Required methods are
    /// lowered straight from the `InterfaceMethodSig` since they don't have
    /// function locs. Returns `None` when the member isn't found anywhere
    /// in the chain (the caller emits the `UnresolvedMember` diagnostic).
    /// Resolve `member` on interface `iface_name`.
    ///
    /// `self_recv` describes how the receiver pins `Self`, which decides whether
    /// the object-safety restriction (`InvalidSelfCallThroughInterface`) applies:
    ///
    /// - [`SelfReceiver::RigidVar`]: the call reaches the interface through a
    ///   *type variable* bound by it — `self` inside the interface's own default
    ///   method, or a generic `T extends Equals`. `Self` is that rigid variable;
    ///   a `Self`-typed argument is checked against it by identity.
    /// - [`SelfReceiver::ExactTy`]: the receiver is a single known type. This
    ///   includes concrete values and rigid projections such as `H.Item`;
    ///   `Self` resolves to that type.
    /// - [`SelfReceiver::Existential`]: a bare `Ty::Interface` ("dyn") receiver.
    ///   A method is callable if and only if `Self` appears in exactly one
    ///   parameter — the `self` receiver itself. Any *additional* `Self`-typed
    ///   parameter (e.g. `other: Self`) makes the method uncallable on the
    ///   existential, because the second `Self` would have to be the same hidden
    ///   concrete type as the receiver, which a "dyn" value cannot guarantee.
    ///   (Return and `throws` positions don't count — they collapse to the
    ///   interface.) This mirrors Rust's `Self`-vs-`dyn Trait` object-safety
    ///   split and Swift's `Self`-vs-`any Protocol`. Interface methods are
    ///   instance-only (every call goes through a receiver), so "the one `Self`
    ///   parameter" is always the `self` receiver; if static interface methods
    ///   are ever added, a non-receiver `Self` parameter is still caught here.
    pub(super) fn resolve_interface_member(
        &mut self,
        bound: InterfaceBound<'_>,
        recv: SelfReceiver<'_>,
        access: MemberAccess<'_>,
    ) -> Option<Ty> {
        let pkg_items = self.resolve_class_pkg_items(bound.name.package())?;
        let def = pkg_items.lookup_type(bound.name.namespace(), bound.name.name())?;
        let Definition::Interface(root_loc) = def else {
            return None;
        };
        let db = self.context.db();
        let root_pkg = baml_compiler2_hir::file_package::file_package(db, root_loc.file(db));
        let pkg_ns = root_pkg.namespace_path;

        // Existential / type-var receiver: the concrete type is unknown, so a member may
        // come from `bound.name` OR any interface it transitively `requires`. Walk that
        // closure and resolve on each interface — `requires` is the bound here.
        for (iface_loc, iface_type_args, iface_associated_bindings) in
            crate::interfaces::interface_closure_locs_with_args_and_assoc(
                db,
                root_loc,
                bound.type_args,
                bound.associated_bindings,
                pkg_items,
                &pkg_ns,
            )
        {
            if let Some(ty) = self.resolve_member_on_one_interface(
                iface_loc,
                &iface_type_args,
                &iface_associated_bindings,
                pkg_items,
                recv,
                &access,
            ) {
                return Some(ty);
            }
        }
        None
    }

    /// Resolve `member` on a **concrete** receiver `base_ty` through the type's own impl
    /// blocks (`impls_for_type` — no `requires`-closure walk; coherence forces a separate
    /// impl per interface). Called by `resolve_member` only after inherent resolution (class
    /// field/method, builtins) misses; `receiver_name` names the receiver for diagnostics.
    ///
    /// A method resolves to its `Ty::Function` (recording `InterfaceConcreteMethod`), and is
    /// ambiguous (`E0121`) when ≥2 distinct interfaces declare it. Interface *fields* are
    /// projection-only: a single source needs `obj.as<I>.field`, ≥2 are ambiguous. Returns
    /// `None` when no impl provides `member` (the caller emits the unknown-member error).
    pub(super) fn resolve_member_from_impls(
        &mut self,
        base_ty: &Ty,
        receiver_name: Name,
        member: &Name,
        at: ExprId,
        bound: bool,
    ) -> Option<Ty> {
        let db = self.context.db();
        let impls = self.type_impls(base_ty);

        // Partition the type's own impls by realized interface `(qtn, args)`: those declaring
        // `member` as a method, and (separately) those declaring it as a field. Coherence
        // forbids two impls of the same realized interface, so dedup defensively.
        let mut method_candidates: Vec<((crate::ty::QualifiedTypeName, Vec<Ty>), _, _, _)> =
            Vec::new();
        for resolved_impl in &impls {
            let Some(resolved_method) = resolved_impl.get_method(db, member) else {
                continue;
            };
            let Ok(data) = crate::interfaces::impl_data(db, resolved_impl.impl_loc) else {
                continue;
            };
            let Some(iface_qtn) = crate::interfaces::interface_loc_qtn(db, data.interface) else {
                continue;
            };
            let realized = (iface_qtn, data.interface_args.clone());
            if !method_candidates.iter().any(|(r, ..)| *r == realized) {
                method_candidates.push((realized, resolved_impl.impl_loc, data, resolved_method));
            }
        }
        let field_sources = self.concrete_interface_field_sources(&impls, member);

        // Interface fields are reachable only through an explicit projection.
        if field_sources.len() >= 2 {
            let sources = field_sources
                .iter()
                .map(|s| {
                    Name::new(format_interface_display(
                        s.interface.name.name(),
                        &s.interface.generics,
                    ))
                })
                .collect();
            self.context.report_at_member(
                TirTypeError::AmbiguousInterfaceField {
                    class_name: receiver_name,
                    field_name: member.clone(),
                    sources,
                },
                at,
                Vec::new(),
            );
            return Some(Ty::Unknown {
                attr: TyAttr::default(),
            });
        }
        if let Some(source) = field_sources.into_iter().next() {
            let interface_name = Name::new(format_interface_display(
                source.interface.name.name(),
                &source.interface.generics,
            ));
            self.context.report_at_member(
                TirTypeError::InterfaceFieldRequiresProjection {
                    class_name: receiver_name,
                    field_name: member.clone(),
                    interface_name,
                },
                at,
                Vec::new(),
            );
            return Some(Ty::Unknown {
                attr: TyAttr::default(),
            });
        }

        // Interface methods. Two or more distinct interfaces declaring `member` is ambiguous.
        if method_candidates.len() >= 2 {
            let sources = method_candidates
                .iter()
                .map(|((qtn, args), ..)| format_interface_display(qtn.name(), args))
                .collect();
            self.context.report_at_member(
                TirTypeError::AmbiguousInterfaceMethod {
                    class_name: receiver_name,
                    method_name: member.clone(),
                    sources,
                },
                at,
                Vec::new(),
            );
            return Some(Ty::Unknown {
                attr: TyAttr::default(),
            });
        }
        let (realized, impl_loc, data, resolved_method) = method_candidates.into_iter().next()?;

        // The realized interface declares the member; build its `Ty::Function` with `Self`
        // pinned to the concrete receiver (the impl conforms, so the signature is the
        // interface's). `pkg_items` is the interface's, for lowering its declared types.
        let pkg_items = self.resolve_class_pkg_items(realized.0.package())?;
        let access = MemberAccess { member, at, bound };
        let ty = self.resolve_member_on_one_interface(
            data.interface,
            &data.interface_args,
            &data.associated_types,
            pkg_items,
            SelfReceiver::ExactTy(base_ty),
            &access,
        )?;

        // The helper recorded a *virtual* call; overwrite it — the impl is statically known,
        // so dispatch is concrete. For an override, also replace the interface-framed generic
        // seed with the impl's frame (the override body is keyed by the impl's own params).
        self.resolutions.insert(
            at,
            crate::inference::MemberResolution::InterfaceConcreteMethod {
                impl_loc,
                func_loc: resolved_method.method,
            },
        );
        if !resolved_method.from_interface_default {
            let frame = data
                .generic_params
                .iter()
                .map(|(name, _)| name.clone())
                .zip(resolved_method.frame_type_args)
                .collect();
            self.owner_type_arg_binding_seed.insert(at, frame);
        }
        Some(ty)
    }

    /// All impl blocks the concrete `base_ty` satisfies — `impls_for_type` with the builder's
    /// package, aliases, and canonical subtyping wired in. `impls_for_type` is *not* a salsa
    /// query (it takes a closure), so callers compute this once and reuse the `Vec` rather
    /// than re-enumerating per member kind.
    pub(super) fn type_impls(&self, base_ty: &Ty) -> Vec<crate::interfaces::ResolvedImpl<'db>> {
        crate::interfaces::impls_for_type(
            self.context.db(),
            self.package_id,
            base_ty,
            &self.aliases,
            |a, b| baml_type::normalize::is_subtype(a, b, &NormalizeCtx(self)),
        )
    }

    /// Among `impls` (from [`Self::type_impls`]), the interfaces that declare `field`, each
    /// paired with the class field it links to (`ImplData.field_links`, default same name),
    /// deduped by realized interface. The shared concrete-field path: member access uses it
    /// for the projection/ambiguity diagnostics, construction for the `field_links` mapping.
    pub(super) fn concrete_interface_field_sources(
        &self,
        impls: &[crate::interfaces::ResolvedImpl<'db>],
        field: &Name,
    ) -> Vec<ConcreteFieldSource<'db>> {
        let db = self.context.db();
        let mut sources: Vec<ConcreteFieldSource<'db>> = Vec::new();
        for resolved_impl in impls {
            let Ok(data) = crate::interfaces::impl_data(db, resolved_impl.impl_loc) else {
                continue;
            };
            let declares_field = baml_compiler2_hir::file_item_tree(db, data.interface.file(db))
                .interfaces
                .get(&data.interface.id(db))
                .is_some_and(|iface| iface.fields.iter().any(|f| &f.name == field));
            if !declares_field {
                continue;
            }
            // The realized interface (the impl's bindings substituted into its head), keyed for
            // dedup — coherence forbids two impls of the same realized interface anyway.
            let interface = resolved_impl.implemented_interface(db);
            if sources.iter().any(|s| s.interface == interface) {
                continue;
            }
            let class_field = data
                .field_links
                .iter()
                .find(|(interface_field, _)| interface_field == field)
                .map(|(_, class_field)| class_field.clone())
                .unwrap_or_else(|| field.clone());
            sources.push(ConcreteFieldSource {
                iface_loc: data.interface,
                interface,
                class_field,
            });
        }
        sources
    }

    /// Resolve `member` on a **union** receiver. A union behaves as the intersection
    /// existential `dyn (I1 + I2 + ...)` over the interfaces every arm provides: `member`
    /// resolves through the *single* shared interface that declares it — inference sugar for
    /// `union.as<I>.member`, recorded as a virtual access/call (the union upcast to `I`, valid
    /// because `I` is implemented by every arm). Zero shared declarers → `Unresolved`; two or
    /// more → `Ambiguous`. Only interface members are reachable on a union — never an arm's
    /// own class fields/methods, which need not agree across arms.
    pub(super) fn resolve_union_member(
        &mut self,
        union_ty: &Ty,
        members: &[Ty],
        member: &Name,
        at: ExprId,
        bound: bool,
    ) -> UnionMemberResolution {
        let declaring: Vec<(ArmInterface<'db>, UnionMemberKind)> = self
            .union_shared_interfaces(members)
            .into_iter()
            .filter_map(|ai| {
                self.interface_member_kind(ai.iface_loc, member)
                    .map(|kind| (ai, kind))
            })
            .collect();
        let unresolved = || {
            UnionMemberResolution::Unresolved(TirTypeError::UnresolvedMember {
                base_type: union_ty.clone(),
                member: member.clone(),
            })
        };
        match declaring.as_slice() {
            [] => unresolved(),
            [(ai, _)] => match self
                .resolve_member_through_shared_interface(ai, union_ty, member, at, bound)
            {
                Some(ty) => UnionMemberResolution::Resolved(ty),
                // `interface_member_kind` already confirmed this interface declares `member`,
                // and per-interface resolution reads the same `iface_data`, so this is
                // unreachable — fall back to unresolved rather than panic in release.
                None => {
                    debug_assert!(
                        false,
                        "a shared interface declares `{member}` (per interface_member_kind) but \
                         resolving it through that interface failed",
                    );
                    unresolved()
                }
            },
            // ≥2 shared interfaces declare `member`: resolving it would silently pick one. The
            // receiver renders as the union; the `as<I>` projection hint in the diagnostic
            // still applies (`union.as<I>.member`).
            _ => {
                let is_field = declaring[0].1 == UnionMemberKind::Field;
                let receiver = Name::new(union_ty.render_user_facing());
                let sources: Vec<String> = declaring
                    .iter()
                    .map(|(ai, _)| {
                        format_interface_display(ai.interface.name.name(), &ai.interface.generics)
                    })
                    .collect();
                let err = if is_field {
                    TirTypeError::AmbiguousInterfaceField {
                        class_name: receiver,
                        field_name: member.clone(),
                        sources: sources.into_iter().map(Name::new).collect(),
                    }
                } else {
                    TirTypeError::AmbiguousInterfaceMethod {
                        class_name: receiver,
                        method_name: member.clone(),
                        sources,
                    }
                };
                UnionMemberResolution::Ambiguous(err)
            }
        }
    }

    /// The interfaces shared by **every** arm of `members` — the union's member surface. An
    /// interface counts only when each arm provides it at equivalent generic args AND
    /// associated bindings (see [`ArmInterface`]).
    fn union_shared_interfaces(&self, members: &[Ty]) -> Vec<ArmInterface<'db>> {
        let mut arms = members.iter();
        let Some(first) = arms.next() else {
            return Vec::new();
        };
        let mut shared = self.union_arm_interfaces(first);
        for arm in arms {
            if shared.is_empty() {
                break;
            }
            let arm_ifaces = self.union_arm_interfaces(arm);
            shared.retain(|s| {
                arm_ifaces
                    .iter()
                    .any(|other| s.interface == other.interface)
            });
        }
        shared
    }

    /// The realized interfaces a single union arm provides. A concrete arm contributes its
    /// own impl blocks (coherence has already materialized the `requires` closure as separate
    /// impls); an interface-existential or type-var arm contributes the bound interface plus
    /// its transitive `requires` closure.
    fn union_arm_interfaces(&self, arm: &Ty) -> Vec<ArmInterface<'db>> {
        let db = self.context.db();
        match arm {
            Ty::Interface(qtn, args, assoc, _) => {
                let Some(pkg_items) = self.resolve_class_pkg_items(qtn.package()) else {
                    return Vec::new();
                };
                let Some(Definition::Interface(root_loc)) =
                    pkg_items.lookup_type(qtn.namespace(), qtn.name())
                else {
                    return Vec::new();
                };
                let pkg_ns = baml_compiler2_hir::file_package::file_package(db, root_loc.file(db))
                    .namespace_path;
                crate::interfaces::interface_closure_locs_with_args_and_assoc(
                    db, root_loc, args, assoc, pkg_items, &pkg_ns,
                )
                .into_iter()
                .filter_map(|(iface_loc, type_args, associated_bindings)| {
                    let name = crate::interfaces::interface_loc_qtn(db, iface_loc)?;
                    Some(ArmInterface {
                        iface_loc,
                        interface: baml_type::Interface::new(name, type_args, associated_bindings),
                    })
                })
                .collect()
            }
            Ty::TypeVar(name, _) => match self.generic_param_bounds.get(name) {
                Some(bound) => self.union_arm_interfaces(&bound.clone()),
                None => Vec::new(),
            },
            // A concrete arm's interfaces come from its own impls, with each impl's bindings
            // substituted into the interface head — so a blanket `impl<U> I<U> for Box<U>` at
            // `Box<int>` yields the *realized* `I<int>`, not the impl-space `I<U>`.
            _ => self
                .type_impls(arm)
                .iter()
                .filter_map(|resolved_impl| {
                    let Ok(data) = crate::interfaces::impl_data(db, resolved_impl.impl_loc) else {
                        return None;
                    };
                    Some(ArmInterface {
                        iface_loc: data.interface,
                        interface: resolved_impl.implemented_interface(db),
                    })
                })
                .collect(),
        }
    }

    /// Whether interface `iface_loc` declares `member`, and as which kind. Side-effect-free —
    /// used to count shared declarers before committing to a virtual resolution.
    fn interface_member_kind(
        &self,
        iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
        member: &Name,
    ) -> Option<UnionMemberKind> {
        let db = self.context.db();
        let iface_tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
        let iface_data = iface_tree.interfaces.get(&iface_loc.id(db))?;
        if iface_data.fields.iter().any(|field| &field.name == member) {
            return Some(UnionMemberKind::Field);
        }
        if iface_data
            .default_methods
            .iter()
            .any(|&fn_id| iface_tree[fn_id].name == *member)
            || iface_data
                .required_methods
                .iter()
                .any(|sig| &sig.name == member)
        {
            return Some(UnionMemberKind::Method);
        }
        None
    }

    /// Resolve `member` through one shared interface `ai`. The virtual dispatch goes through
    /// `ai` (the `union.as<I>` view — recording the interface slot), but `Self` binds to
    /// `union_ty` itself ([`SelfReceiver::Union`]), the subtype of the interface existential,
    /// so a `Self`-returning method yields the union rather than the erased interface.
    fn resolve_member_through_shared_interface(
        &mut self,
        ai: &ArmInterface<'db>,
        union_ty: &Ty,
        member: &Name,
        at: ExprId,
        bound: bool,
    ) -> Option<Ty> {
        // The loc reads the members; the `interface` supplies the realized args/qtn. They are
        // derived from the same interface at construction, but nothing in the type forbids a
        // future divergence that would read one interface's members and bind another's args.
        debug_assert_eq!(
            crate::interfaces::interface_loc_qtn(self.context.db(), ai.iface_loc).as_ref(),
            Some(&ai.interface.name),
            "ArmInterface.iface_loc and .interface must name the same interface",
        );
        let pkg_items = self.resolve_class_pkg_items(ai.interface.name.package())?;
        self.resolve_member_on_one_interface(
            ai.iface_loc,
            &ai.interface.generics,
            &ai.interface.associated_types,
            pkg_items,
            SelfReceiver::Union(union_ty),
            &MemberAccess { member, at, bound },
        )
    }

    /// Resolve `member` on **one** interface instantiation — its own fields and methods,
    /// with **no** `requires`-closure walk. [`Self::resolve_interface_member`] calls this
    /// per interface in an existential/type-var receiver's bound closure.
    fn resolve_member_on_one_interface(
        &mut self,
        iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
        iface_type_args: &[Ty],
        iface_associated_bindings: &[(Name, Ty)],
        pkg_items: &'db PackageItems<'db>,
        recv: SelfReceiver<'_>,
        access: &MemberAccess<'_>,
    ) -> Option<Ty> {
        let db = self.context.db();
        let file = iface_loc.file(db);
        let iface_tree = baml_compiler2_hir::file_item_tree(db, file);
        let iface_data = iface_tree.interfaces.get(&iface_loc.id(db))?;
        let iface_ns = baml_compiler2_hir::file_package::file_package(db, file).namespace_path;
        let current_iface_qtn = crate::lower_type_expr::qualify_def(
            db,
            Definition::Interface(iface_loc),
            &iface_data.name,
        );
        let iface = OneInterface {
            iface_loc,
            iface_data,
            iface_type_args,
            iface_associated_bindings,
            current_iface_qtn: &current_iface_qtn,
            iface_ns: &iface_ns,
            pkg_items,
            prefer_symbolic_projections: matches!(recv, SelfReceiver::RigidVar(_)),
        };

        // Field lookup: this interface's own fields (no closure).
        for field in &iface_data.fields {
            if &field.name != access.member {
                continue;
            }
            if !access.bound {
                self.context.report_simple(
                    TirTypeError::InterfaceMemberRequiresReceiver {
                        interface_name: iface_data.name.clone(),
                        member_name: access.member.clone(),
                    },
                    access.at,
                );
                return Some(Ty::Error {
                    attr: TyAttr::default(),
                });
            }
            let ty = field
                .type_expr
                .as_ref()
                .map(|te| {
                    let mut diags = Vec::new();
                    let mut bindings = crate::generics::bind_type_vars(
                        &iface_data.generic_params,
                        iface_type_args,
                    );
                    for generic_param in &iface_data.generic_params {
                        bindings.entry(generic_param.clone()).or_insert_with(|| {
                            Ty::TypeVar(generic_param.clone(), TyAttr::default())
                        });
                    }
                    self.add_interface_associated_type_bindings(
                        &iface.binding_inputs(),
                        &mut bindings,
                        &mut diags,
                    );
                    let ty = crate::generics::lower_type_expr_with_generics(
                        db, &te.expr, pkg_items, &iface_ns, &bindings, &mut diags,
                    );
                    for diag in diags {
                        self.context.report_at_span(diag, te.span);
                    }
                    ty
                })
                .unwrap_or(Ty::Unknown {
                    attr: TyAttr::default(),
                });
            self.resolutions.insert(
                access.at,
                crate::inference::MemberResolution::InterfaceVirtualField {
                    iface_loc: iface.iface_loc,
                    field: access.member.clone(),
                },
            );
            return Some(ty);
        }

        // Method lookup: the interface's default body, else its required signature.
        // Both normalize to one `InterfaceMethodSpec` and one builder.
        if let Some(&fn_id) = iface_data
            .default_methods
            .iter()
            .find(|&&fn_id| iface_tree[fn_id].name == *access.member)
        {
            let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(db, file, fn_id);
            let spec = InterfaceMethodSpec::from_default(db, func_loc);
            return Some(self.build_interface_method_ty(&spec, &iface, recv, access));
        }
        if let Some(sig) = iface_data
            .required_methods
            .iter()
            .find(|sig| sig.name == *access.member)
        {
            let spec = InterfaceMethodSpec::from_required(sig);
            return Some(self.build_interface_method_ty(&spec, &iface, recv, access));
        }
        None
    }

    /// Build the `Ty::Function` for an interface method from its normalized
    /// [`InterfaceMethodSpec`], resolved on one interface instantiation. Handles the
    /// `Self` placeholder (rigid var / exact receiver / existential), associated-type
    /// bindings, generic binding and bound recording, and the `bound`-receiver `self`
    /// strip — and records the `InterfaceVirtualMethod` slot resolution (the concrete path
    /// overwrites it with `InterfaceConcreteMethod` once the impl is known).
    fn build_interface_method_ty(
        &mut self,
        spec: &InterfaceMethodSpec,
        iface: &OneInterface<'_, 'db>,
        recv: SelfReceiver<'_>,
        access: &MemberAccess<'_>,
    ) -> Ty {
        let db = self.context.db();
        let rigid_pin: Option<Name> = match recv {
            SelfReceiver::RigidVar(pin) => Some(pin.clone()),
            _ => None,
        };
        let preserve_self_associated_projections =
            matches!(recv, SelfReceiver::Existential(_)) && access.bound;
        let generic_names: Vec<Name> = spec.generics.iter().map(|(name, _)| name.clone()).collect();

        // An exact receiver pins `Self` to its own type, not to a fresh method generic,
        // so suppress the unbound-reference generic there.
        let receiver_generic = (!access.bound
            && !matches!(recv, SelfReceiver::ExactTy(_) | SelfReceiver::Union(_)))
        .then(|| self.fresh_interface_method_receiver_generic(iface.iface_data, &generic_names));

        let mut diags = Vec::new();
        let mut bindings = if iface.iface_type_args.is_empty() {
            rustc_hash::FxHashMap::default()
        } else {
            crate::generics::bind_type_vars(&iface.iface_data.generic_params, iface.iface_type_args)
        };
        for generic_param in &iface.iface_data.generic_params {
            bindings
                .entry(generic_param.clone())
                .or_insert_with(|| Ty::TypeVar(generic_param.clone(), TyAttr::default()));
        }
        self.add_interface_associated_type_bindings(
            &iface.binding_inputs(),
            &mut bindings,
            &mut diags,
        );
        for generic_param in &generic_names {
            bindings
                .entry(generic_param.clone())
                .or_insert_with(|| Ty::TypeVar(generic_param.clone(), TyAttr::default()));
        }

        // A pinned `Self` (rigid type variable for `self`/`T extends I`, or an exact
        // receiver type) takes precedence over the fresh existential generic; it's
        // registered as a known type variable without clobbering an identically-named
        // interface generic, and an exact receiver binds it to its own type.
        let mut all_generic_params = iface.iface_data.generic_params.clone();
        let (self_ty_var, self_exact) = self.self_substitution(
            recv,
            iface.iface_data,
            &generic_names,
            receiver_generic.as_ref(),
        );
        if let Some(name) = &self_ty_var {
            let binding =
                self_exact.unwrap_or_else(|| Ty::TypeVar(name.clone(), TyAttr::default()));
            bindings.entry(name.clone()).or_insert(binding);
            if !all_generic_params.contains(name) {
                all_generic_params.push(name.clone());
            }
        }
        all_generic_params.extend(generic_names.iter().cloned());

        let iface_ty = Ty::Interface(
            iface.current_iface_qtn.clone(),
            if iface.iface_type_args.is_empty() {
                iface
                    .iface_data
                    .generic_params
                    .iter()
                    .map(|gp| Ty::TypeVar(gp.clone(), TyAttr::default()))
                    .collect()
            } else {
                iface.iface_type_args.to_vec()
            },
            iface.iface_associated_bindings.to_vec(),
            TyAttr::default(),
        );
        let self_replacement = self_ty_var
            .as_ref()
            .map(|name| crate::lower_type_expr::type_expr_for_name(name.clone()))
            .unwrap_or_else(|| Self::interface_self_type_expr(iface.iface_data));

        // A pinned `Self` is a single type, so `Self`-typed parameters are sound; the
        // object-safety restriction only applies to a bare existential receiver. The `self`
        // receiver itself is `self: Self` by construction — exclude it, only *other* params
        // referencing `Self` break object safety.
        if matches!(recv, SelfReceiver::Existential(_) | SelfReceiver::Union(_))
            && access.bound
            && spec
                .args
                .iter()
                .chain(&spec.kwargs)
                .filter(|p| p.name.as_str() != "self")
                .any(|p| Self::type_expr_contains_self(&p.ty))
        {
            self.context.report_simple(
                TirTypeError::InvalidSelfCallThroughInterface {
                    interface_name: iface.iface_data.name.clone(),
                    method_name: access.member.clone(),
                },
                access.at,
            );
        }

        // `self` is an ordinary positional arg (`self: Self`), lowered here like any other:
        // its `Self` is substituted to the receiver and the `bound`-receiver strip below
        // removes it for method-value references.
        let mut params: Vec<FunctionParamTy> =
            Vec::with_capacity(spec.args.len() + spec.kwargs.len());
        for (param, mode) in spec
            .args
            .iter()
            .map(|p| (p, FunctionParamMode::Required))
            .chain(spec.kwargs.iter().map(|p| (p, FunctionParamMode::Optional)))
        {
            let ty_expr = Self::substitute_interface_self_in(
                &param.ty,
                &self_replacement,
                preserve_self_associated_projections,
            );
            let param_ty = if bindings.is_empty() {
                crate::lower_type_expr::lower_type_expr_in_ns(
                    db,
                    &ty_expr,
                    iface.pkg_items,
                    iface.iface_ns,
                    &all_generic_params,
                    &mut diags,
                )
            } else {
                crate::generics::lower_type_expr_with_generics(
                    db,
                    &ty_expr,
                    iface.pkg_items,
                    iface.iface_ns,
                    &bindings,
                    &mut diags,
                )
            };
            params.push(FunctionParamTy {
                name: Some(param.name.clone()),
                ty: param_ty,
                mode,
            });
        }

        let lower_signature_ty =
            |_this: &mut Self, te: &baml_compiler2_ast::TypeExpr, diags: &mut Vec<_>| {
                let ty_expr = Self::substitute_interface_self_in(
                    te,
                    &self_replacement,
                    preserve_self_associated_projections,
                );
                if bindings.is_empty() {
                    crate::lower_type_expr::lower_type_expr_in_ns(
                        db,
                        &ty_expr,
                        iface.pkg_items,
                        iface.iface_ns,
                        &all_generic_params,
                        diags,
                    )
                } else {
                    crate::generics::lower_type_expr_with_generics(
                        db,
                        &ty_expr,
                        iface.pkg_items,
                        iface.iface_ns,
                        &bindings,
                        diags,
                    )
                }
            };
        let ret_ty = lower_signature_ty(self, &spec.return_type, &mut diags);
        let throws_ty = lower_signature_ty(self, &spec.throws, &mut diags);

        // Track the method's generic params *and* their bounds so call-site bound
        // enforcement works without the function type carrying them: the receiver generic
        // is bounded by the interface existential, the method's own params by their
        // declared bounds.
        let mut function_generic_params = Vec::new();
        let mut function_generic_param_bounds: Vec<Option<Ty>> = Vec::new();
        if let Some(receiver_generic) = &receiver_generic {
            function_generic_params.push(receiver_generic.clone());
            function_generic_param_bounds.push(Some(iface_ty.clone()));
            self.generic_param_bounds
                .insert(receiver_generic.clone(), iface_ty);
        }
        function_generic_params.extend(generic_names.iter().cloned());
        let bound_exprs: Vec<Option<baml_compiler2_ast::TypeExpr>> = spec
            .generics
            .iter()
            .map(|(_, bound)| bound.clone())
            .collect();
        function_generic_param_bounds.extend(lower_generic_param_bounds(
            db,
            &bound_exprs,
            iface.pkg_items,
            iface.iface_ns,
            &all_generic_params,
            Some(&bindings),
            &mut diags,
        ));
        for diag in diags {
            self.context.report(diag, access.at, Vec::new());
        }

        let mut fn_ty = Ty::Function {
            params,
            ret: Box::new(ret_ty),
            throws: Box::new(throws_ty),
            attr: TyAttr::default(),
        };
        if access.bound
            && let Ty::Function {
                params,
                ret,
                throws,
                attr,
            } = fn_ty
        {
            let stripped = crate::generics::skip_self_param(&params).to_vec();
            fn_ty = Ty::Function {
                params: stripped,
                ret,
                throws,
                attr,
            };
        }

        // The slot is what's known for a virtual call — the interface plus the member name,
        // not a body. Recorded uniformly for default and required methods; the owner's
        // (interface) generic args are seeded so the call's type-args bind them.
        self.resolutions.insert(
            access.at,
            crate::inference::MemberResolution::InterfaceVirtualMethod {
                iface_loc: iface.iface_loc,
                method: access.member.clone(),
            },
        );
        let owner_type_arg_bindings = iface
            .iface_data
            .generic_params
            .iter()
            .filter_map(|param| bindings.get(param).cloned().map(|ty| (param.clone(), ty)))
            .collect::<Vec<_>>();
        if !owner_type_arg_bindings.is_empty() {
            self.owner_type_arg_binding_seed
                .insert(access.at, owner_type_arg_bindings);
        }
        if let Some(pin) = &rigid_pin {
            self.self_pinned_rigid_var.insert(access.at, pin.clone());
        }
        self.interface_method_generic_params.insert(
            access.at,
            (
                access.member.clone(),
                function_generic_params,
                function_generic_param_bounds,
            ),
        );
        fn_ty
    }
}
