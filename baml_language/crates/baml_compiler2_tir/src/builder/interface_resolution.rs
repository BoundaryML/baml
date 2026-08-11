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
use baml_type::normalize::TypeContext;

use super::{
    Definition, ExprId, InterfaceBound, MemberAccess, Name, PackageItems, SelfReceiver,
    TirTypeError, Ty, TyAttr, TypeInferenceBuilder, lower_generic_param_bound_refs,
};
use crate::{
    infer_context::SelfCallPosition,
    signature::{DeclaredSignature, SigTypeRef, lower_signature},
};

/// One realized interface instantiation to resolve a member against: `loc` reads the declared
/// members (fields, method signatures and bodies); `realized` is the interface at its applied
/// generic args and associated bindings — how the receiver reached it. The interface's package
/// items, namespace, and item-tree data all derive from `loc`.
#[derive(Clone)]
struct InterfaceView<'db> {
    loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    realized: baml_type::Interface,
}

/// A member declarer may have a source loc or be represented solely by a
/// mounted interface row. Keeping both in one candidate set is essential for
/// conjunction ambiguity: source and mounted bounds are sibling constraints.
enum MemberDeclarer<'db> {
    Source(InterfaceView<'db>),
    Mounted(baml_type::Interface),
}

impl MemberDeclarer<'_> {
    fn realized(&self) -> &baml_type::Interface {
        match self {
            Self::Source(view) => &view.realized,
            Self::Mounted(realized) => realized,
        }
    }
}

impl<'db> InterfaceView<'db> {
    /// The namespace the interface is declared in — the scope its member type expressions
    /// resolve unqualified names against.
    fn namespace(&self, db: &dyn crate::Db) -> Vec<Name> {
        baml_compiler2_hir::file_package::file_package(db, self.loc.file(db)).namespace_path
    }

    /// The interface's own package items, for lowering its declared member types.
    fn pkg_items(&self, db: &'db dyn crate::Db) -> &'db PackageItems<'db> {
        let package = baml_compiler2_hir::file_package::file_package(db, self.loc.file(db)).package;
        baml_compiler2_ppir::package_items(
            db,
            baml_compiler2_hir::package::PackageId::new(db, package),
        )
    }
}

/// The lowering environment for one interface member (field or method) resolved on a
/// receiver, built by [`TypeInferenceBuilder::interface_member_lowering_env`]: the scope a
/// member's declared type expression(s) lower in, and the substitution that specializes the
/// result to the receiver.
struct MemberLoweringEnv {
    /// Type-variable names in scope for the lowering: the interface's params, the caller's
    /// extra names (method generics), `Self`'s bound name (when symbolic), and the
    /// associated-type names.
    all_generic_params: Vec<crate::ty::ParamTy>,
    /// The lowering scope's bounds — the enclosing scope's (shadowed by this member's own
    /// names), the interface's declared param bounds, and `Self`'s interface bound.
    bounds: crate::lower_type_expr::TypeVarBoundsMap,
    /// What `Self` lowers to for this receiver.
    self_ty: Ty,
    /// The realized interface with only its *explicit* associated bindings — the constraint
    /// recorded for a fresh receiver generic's call-site enforcement.
    iface_ty: Ty,
    /// Substitution applied to the lowered type: interface generics at their realized args,
    /// extra generics to themselves, each associated type to its pin or symbolic projection.
    bindings: rustc_hash::FxHashMap<crate::ty::ParamTy, Ty>,
    /// Diagnostics produced while resolving pins/projections; the caller reports them.
    diags: Vec<TirTypeError>,
}

/// A positional or keyword interface-method parameter. The `self` receiver is an ordinary
/// positional `arg`, desugared to `self: Self` (its `ty` is [`SigTypeRef::SelfReceiver`]).
struct InterfaceMethodParam {
    name: Name,
    ty: SigTypeRef,
}

/// A normalized interface-method signature: default and required methods both reduce to
/// this so one builder produces the `Ty::Function`. Interfaces must label their full
/// contract, so `return_type`/`throws` are required (sourced from the declaration, never
/// inferred from a body).
pub(crate) struct InterfaceMethodSpec<'db> {
    /// The arena every signature slot (`args`/`kwargs`/`return_type`/`throws`) indexes: the
    /// elaborated store for a default method, the interface's own store for a required one.
    sig_refs: &'db baml_compiler2_hir::type_ref::TypeRefStore,
    /// The arena the `generics` bound ids index. Bounds are never elaborated, so for a
    /// default method this is the *declaration* store (`FunctionData::type_refs`), distinct
    /// from `sig_refs`; for a required method the two are one store.
    bound_refs: &'db baml_compiler2_hir::type_ref::TypeRefStore,
    /// Positional (required) parameters, in declaration order.
    args: Vec<InterfaceMethodParam>,
    /// Keyword/optional parameters (those with a default).
    kwargs: Vec<InterfaceMethodParam>,
    return_type: SigTypeRef,
    throws: SigTypeRef,
    /// Method generic params with their interface-bound *conjunction* (`T extends A & B`),
    /// unified — every conjunct is kept, so an override's bounds compare as sets.
    generics: Vec<baml_compiler2_ppir::item_data::GenericParamData>,
}

impl<'db> InterfaceMethodSpec<'db> {
    pub(crate) fn from_default(
        db: &'db dyn crate::Db,
        func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
    ) -> Self {
        let sig = baml_compiler2_ppir::item_data::elaborated_function_data(db, func_loc);
        let func_data = baml_compiler2_ppir::item_data::function_data(db, func_loc);
        let (args, kwargs) = split_params(sig.params.iter().map(|p| {
            // The implicit `self` receiver: name "self" with no declared type
            // (elaboration synthesizes an `Unknown` node for it).
            let is_self = p.name.as_str() == "self"
                && matches!(
                    sig.type_refs[p.type_ref].kind,
                    baml_compiler2_hir::type_ref::TypeRefKind::Unknown
                );
            (
                is_self,
                p.has_default,
                p.name.clone(),
                SigTypeRef::Id(p.type_ref),
            )
        }));
        // `user_generic_params` is the elaborated view of the same declaration
        // list, so the declaration's params carry the bounds in the same order.
        let generics = func_data.generic_params.clone();
        Self {
            sig_refs: &sig.type_refs,
            bound_refs: &func_data.type_refs,
            args,
            kwargs,
            return_type: sig.return_type.map_or(SigTypeRef::Missing, SigTypeRef::Id),
            throws: sig.throws.map_or(SigTypeRef::Missing, SigTypeRef::Id),
            generics,
        }
    }

    pub(crate) fn from_required(
        iface_data: &'db baml_compiler2_ppir::item_data::InterfaceData<'db>,
        sig: &baml_compiler2_ppir::item_data::InterfaceMethodSigData,
    ) -> Self {
        let (args, kwargs) = split_params(sig.params.iter().map(|p| {
            let is_self = p.name.as_str() == "self" && p.type_ref.is_none();
            let ty = p.type_ref.map_or(SigTypeRef::Missing, SigTypeRef::Id);
            (is_self, p.has_default, p.name.clone(), ty)
        }));
        let generics = sig.generic_params.clone();
        Self {
            sig_refs: &iface_data.type_refs,
            bound_refs: &iface_data.type_refs,
            args,
            kwargs,
            return_type: sig.return_type.map_or(SigTypeRef::Missing, SigTypeRef::Id),
            throws: sig.throws.map_or(SigTypeRef::Missing, SigTypeRef::Id),
            generics,
        }
    }

    /// The method's own generic parameter names — in scope (as free type variables) when
    /// lowering this spec's signature through a context.
    pub(crate) fn generic_param_names(&self) -> Vec<Name> {
        self.generics
            .iter()
            .map(|param| param.name.clone())
            .collect()
    }

    /// The method's generic parameters paired with their interface-bound conjunction
    /// (declaration order, ids into [`Self::bound_store`]) — so an override's bounds can be
    /// checked against the interface method's (an implementation may not add a requirement
    /// the interface does not declare).
    pub(crate) fn generic_bounds(&self) -> &[baml_compiler2_ppir::item_data::GenericParamData] {
        &self.generics
    }

    /// The arena [`Self::generic_bounds`]' ids index.
    pub(crate) fn bound_store(&self) -> &'db baml_compiler2_hir::type_ref::TypeRefStore {
        self.bound_refs
    }

    /// Lower this normalized signature to a `Ty::Function` through `ctx` (which resolves `Self`,
    /// `Self.Assoc`, and the in-scope generics). Still a template over its free type variables;
    /// the caller specializes it (binding interface/method generics via `substitute_ty`).
    pub(crate) fn to_function_ty(
        &self,
        ctx: &dyn crate::lower_type_expr::TypeExprContext<'_>,
        diags: &mut Vec<TirTypeError>,
    ) -> Ty {
        let signature = DeclaredSignature {
            type_refs: self.sig_refs,
            positional: self
                .args
                .iter()
                .map(|p| (Some(p.name.clone()), p.ty))
                .collect(),
            keyword: self
                .kwargs
                .iter()
                .map(|p| (Some(p.name.clone()), p.ty))
                .collect(),
            return_type: self.return_type,
            throws: self.throws,
        };
        lower_signature(&signature, ctx, diags).into_ty()
    }
}

/// Split `(is_self, has_default, name, ty)` tuples into positional args (no default) and
/// keyword/optional kwargs (with a default). The `self` receiver desugars to `self: Self`
/// and stays a positional arg (its declared type is replaced by
/// [`SigTypeRef::SelfReceiver`]).
fn split_params(
    params: impl Iterator<Item = (bool, bool, Name, SigTypeRef)>,
) -> (Vec<InterfaceMethodParam>, Vec<InterfaceMethodParam>) {
    let mut args = Vec::new();
    let mut kwargs = Vec::new();
    for (is_self, has_default, name, ty) in params {
        let ty = if is_self {
            SigTypeRef::SelfReceiver
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
    /// A namespace-qualified display of `iface` for an ambiguity / projection diagnostic —
    /// spelled so the suggested `recv.as<…>` fix resolves from the *current call site's*
    /// namespace. A dependency-package interface, or a local one named from the package root,
    /// uses the plain user-facing path (`zoo.Animal`, resolvable as written); a local interface
    /// named from inside a sub-namespace is forced root-relative (`root.zoo.Animal`) so a bare
    /// `zoo` segment is not misread there as a sibling namespace or a package.
    fn qualified_interface_display(&self, iface: &baml_type::Interface) -> String {
        let qtn = &iface.name;
        let base = if qtn.is_local() && !self.ns_context.is_empty() {
            match qtn.namespace().as_slice() {
                [] => format!("root.{}", qtn.name()),
                ns => format!(
                    "root.{}.{}",
                    ns.iter().map(Name::as_str).collect::<Vec<_>>().join("."),
                    qtn.name()
                ),
            }
        } else {
            qtn.render_user_facing()
        };
        if iface.generics.is_empty() {
            base
        } else {
            let args = iface
                .generics
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{base}<{args}>")
        }
    }

    /// Resolve `access.member` on an existential / type-variable receiver whose interface
    /// constraint is `bound`, by walking `bound`'s `requires`-closure and resolving the
    /// member on the first interface that declares it. A default method (with a body) is
    /// resolved via its `FunctionLoc` like a class method; a required method is lowered
    /// straight from its `InterfaceMethodSig` (no function loc). Returns `None` when no
    /// interface in the chain declares the member (the caller emits the `UnresolvedMember`
    /// diagnostic).
    ///
    /// `recv` describes how the receiver pins `Self`, which decides whether
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
        // A MOUNTED (source-less) interface has no loc: its declaration
        // surface is the exported row, resolved by pure substitution
        // (BEP-066 mounted-package linking).
        if crate::package_interface::mounted_type_row(self.context.db(), bound.name).is_some() {
            return self.resolve_member_on_mounted_interface(bound, recv, access);
        }
        let declarers = self.member_declarers_for_bound(bound, access.member);
        self.arbitrate_member_declarers(&declarers, recv, access)
    }

    /// Resolve `access.member` on a receiver bounded by a **conjunction** of interfaces
    /// (`T extends A & B`, `type Assoc extends J & K`).
    ///
    /// Unlike a single bound's `requires` closure, the conjuncts are siblings: none
    /// shadows another, so a member declared by two of them is ambiguous exactly as two
    /// incomparable interfaces within one closure are. Collecting every conjunct's
    /// declarers before arbitrating is what makes that visible — resolving conjunct by
    /// conjunct and taking the first hit silently picks one.
    ///
    /// Each conjunct still applies its own root-wins tiering first, and the union is
    /// deduped by realized identity, so overlaps that denote the *same* interface (`B`
    /// requiring `A` with the member on `A`) stay unambiguous.
    pub(super) fn resolve_interface_member_over_conjunction(
        &mut self,
        bounds: &[baml_type::Interface],
        recv: SelfReceiver<'_>,
        access: MemberAccess<'_>,
    ) -> Option<Ty> {
        let mut declarers: Vec<MemberDeclarer<'db>> = Vec::new();
        for iface in bounds {
            for view in self.member_declarers_for_bound(
                InterfaceBound {
                    name: &iface.name,
                    type_args: &iface.generics,
                    associated_bindings: &iface.associated_types,
                },
                access.member,
            ) {
                if !declarers.iter().any(|v| v.realized() == &view.realized) {
                    declarers.push(MemberDeclarer::Source(view));
                }
            }
            for declarer in self.mounted_method_declarers_for_bound(
                InterfaceBound {
                    name: &iface.name,
                    type_args: &iface.generics,
                    associated_bindings: &iface.associated_types,
                },
                access.member,
            ) {
                if !declarers
                    .iter()
                    .any(|v| v.realized() == declarer.realized())
                {
                    declarers.push(declarer);
                }
            }
        }
        match declarers.as_slice() {
            [] => None,
            [MemberDeclarer::Source(view)] => {
                self.resolve_member_on_one_interface(view, recv, &access)
            }
            [MemberDeclarer::Mounted(realized)] => self.resolve_member_on_mounted_interface(
                InterfaceBound {
                    name: &realized.name,
                    type_args: &realized.generics,
                    associated_bindings: &realized.associated_types,
                },
                recv,
                access,
            ),
            _ => {
                let is_field = declarers.iter().any(|declarer| match declarer {
                    MemberDeclarer::Source(view) => {
                        self.interface_member_kind(view.loc, access.member)
                            == Some(UnionMemberKind::Field)
                    }
                    MemberDeclarer::Mounted(_) => false,
                });
                let receiver = match &recv {
                    SelfReceiver::RigidVar(name) => name.to_string(),
                    SelfReceiver::ExactTy(ty)
                    | SelfReceiver::Existential(ty)
                    | SelfReceiver::Union(ty) => ty.render_user_facing(),
                };
                let sources: Vec<String> = declarers
                    .iter()
                    .map(|v| self.qualified_interface_display(v.realized()))
                    .collect();
                let error = if is_field {
                    TirTypeError::AmbiguousInterfaceField {
                        class_name: Name::new(&receiver),
                        field_name: access.member.clone(),
                        sources: sources
                            .into_iter()
                            .map(|source| Name::new(&source))
                            .collect(),
                    }
                } else {
                    TirTypeError::AmbiguousInterfaceMethod {
                        class_name: Name::new(&receiver),
                        method_name: access.member.clone(),
                        sources,
                    }
                };
                self.context.report_at_member_simple(error, access.at);
                Some(Ty::Unknown {
                    attr: TyAttr::default(),
                })
            }
        }
    }

    /// Loc-free declarers for one mounted bound, with root-wins tiering and a
    /// mixed source/mounted `requires` closure. Mounted fields are intentionally
    /// omitted; this helper enumerates methods only.
    fn mounted_method_declarers_for_bound(
        &self,
        bound: InterfaceBound<'_>,
        member: &Name,
    ) -> Vec<MemberDeclarer<'db>> {
        use crate::package_interface::ExportedType;

        let db = self.context.db();
        let Some(ExportedType::Interface {
            qtn,
            generic_params,
            required_methods,
            default_methods,
            requires,
            ..
        }) = crate::package_interface::mounted_type_row(db, bound.name)
        else {
            return Vec::new();
        };
        let root = baml_type::Interface::new(
            qtn.clone(),
            bound.type_args.to_vec(),
            bound.associated_bindings.to_vec(),
        );
        if required_methods
            .iter()
            .chain(default_methods)
            .any(|method| method.name == *member)
        {
            return vec![MemberDeclarer::Mounted(root)];
        }

        let bindings = crate::generics::bind_type_vars(generic_params, bound.type_args);
        let mut out = Vec::new();
        for required in requires {
            let realized = required.map_tys(|ty| crate::generics::substitute_ty(ty, &bindings));
            let pkg =
                baml_compiler2_hir::package::PackageId::new(db, realized.name.package().clone());
            match baml_compiler2_ppir::package_items(db, pkg)
                .lookup_type(realized.name.namespace(), realized.name.name())
            {
                Some(Definition::Interface(loc))
                    if self.interface_member_kind(loc, member) == Some(UnionMemberKind::Method) =>
                {
                    out.push(MemberDeclarer::Source(InterfaceView { loc, realized }));
                }
                _ => {
                    if let Some(ExportedType::Interface {
                        required_methods,
                        default_methods,
                        ..
                    }) = crate::package_interface::mounted_type_row(db, &realized.name)
                        && required_methods
                            .iter()
                            .chain(default_methods)
                            .any(|method| method.name == *member)
                    {
                        out.push(MemberDeclarer::Mounted(realized));
                    }
                }
            }
        }
        out
    }

    /// Every interface in `bound`'s `requires` closure that declares `member`.
    ///
    /// Existential / type-var receiver: the concrete type is unknown, so a member may
    /// come from `bound.name` OR any interface it transitively `requires`. Resolution
    /// is *tiered*, matching the associated-type resolver `resolve_through_roots`: the
    /// directly-named interface shadows the ones it requires (root-wins), but two
    /// *incomparable* interfaces declaring the same member are ambiguous and must be
    /// qualified (`recv.as<I>.member`). So resolve through the root when it declares
    /// `member`; otherwise collect the closure interfaces that declare it.
    fn member_declarers_for_bound(
        &mut self,
        bound: InterfaceBound<'_>,
        member: &Name,
    ) -> Vec<InterfaceView<'db>> {
        let Some(pkg_items) = self.resolve_class_pkg_items(bound.name.package()) else {
            return Vec::new();
        };
        let Some(def) = pkg_items.lookup_type(bound.name.namespace(), bound.name.name()) else {
            return Vec::new();
        };
        let Definition::Interface(root_loc) = def else {
            return Vec::new();
        };
        let db = self.context.db();

        // A rigid `Self` receiver resolves associated types symbolically — do NOT fill
        // an unbound associated type with its interface default (the implementor may
        // override it); the default is applied for concrete/existential receivers by
        // `associated_type_pin`. Existential/exact receivers reach the same closure but
        // get their defaults there, so passing `false` here is correct for all receivers.
        let closure = crate::interfaces::interface_closure_locs_with_args_and_assoc(
            db,
            root_loc,
            bound.type_args,
            bound.associated_bindings,
            false,
        );
        let mut declarers: Vec<InterfaceView<'db>> = Vec::new();
        if self.interface_member_kind(root_loc, member).is_some() {
            // Direct tier: the root shadows everything it transitively requires.
            if let Some((loc, args, assoc)) = closure.iter().find(|(loc, _, _)| *loc == root_loc)
                && let Some(qtn) = crate::interfaces::interface_loc_qtn(db, *loc)
            {
                declarers.push(InterfaceView {
                    loc: *loc,
                    realized: baml_type::Interface::new(qtn, args.clone(), assoc.clone()),
                });
            }
        } else {
            // Transitive tier: closure interfaces that declare it, deduped by realized
            // identity so a diamond (`D: A, B` both requiring `C`) counts `C` once.
            for (loc, args, assoc) in &closure {
                if *loc == root_loc {
                    continue;
                }
                if self.interface_member_kind(*loc, member).is_some()
                    && let Some(qtn) = crate::interfaces::interface_loc_qtn(db, *loc)
                {
                    let realized = baml_type::Interface::new(qtn, args.clone(), assoc.clone());
                    if !declarers.iter().any(|v| v.realized == realized) {
                        declarers.push(InterfaceView {
                            loc: *loc,
                            realized,
                        });
                    }
                }
            }
        }
        declarers
    }

    /// One declarer resolves; ≥2 are ambiguous; none is unresolved (`None`, so the
    /// caller can fall through to its own not-found handling).
    fn arbitrate_member_declarers(
        &mut self,
        declarers: &[InterfaceView<'db>],
        recv: SelfReceiver<'_>,
        access: MemberAccess<'_>,
    ) -> Option<Ty> {
        match declarers {
            [] => None,
            [one] => self.resolve_member_on_one_interface(one, recv, &access),
            // ≥2 incomparable interfaces declare `member`: resolving it would silently pick
            // one. Report the ambiguity (as for a union receiver) — the `recv.as<I>.member`
            // qualification hint in the diagnostic applies.
            _ => {
                let is_field = self.interface_member_kind(declarers[0].loc, access.member)
                    == Some(UnionMemberKind::Field);
                let receiver = match &recv {
                    SelfReceiver::RigidVar(name) => name.to_string(),
                    SelfReceiver::ExactTy(ty)
                    | SelfReceiver::Existential(ty)
                    | SelfReceiver::Union(ty) => ty.render_user_facing(),
                };
                let sources: Vec<String> = declarers
                    .iter()
                    .map(|v| self.qualified_interface_display(&v.realized))
                    .collect();
                let err = if is_field {
                    TirTypeError::AmbiguousInterfaceField {
                        class_name: Name::new(&receiver),
                        field_name: access.member.clone(),
                        sources: sources.into_iter().map(|s| Name::new(&s)).collect(),
                    }
                } else {
                    TirTypeError::AmbiguousInterfaceMethod {
                        class_name: Name::new(&receiver),
                        method_name: access.member.clone(),
                        sources,
                    }
                };
                self.context.report_at_member_simple(err, access.at);
                Some(Ty::Unknown {
                    attr: TyAttr::default(),
                })
            }
        }
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

        // Partition the type's own impls by *realized* interface — the impl's head with its
        // matched bindings substituted (`implements Iterator<T, never>` on `Repeat<T>` at a
        // `Repeat<int>` receiver is `Iterator<int, never>`, never the raw pattern form): those
        // declaring `member` as a method, and (separately) those declaring it as a field.
        // Coherence forbids two impls of the same realized interface, so dedup defensively.
        let mut method_candidates: Vec<(baml_type::Interface, _, _, _, _)> = Vec::new();
        for resolved_impl in &impls {
            let Some(resolved_method) = resolved_impl.get_method(db, member) else {
                continue;
            };
            let Some(data) = resolved_impl.data(db) else {
                continue;
            };
            let realized = resolved_impl.implemented_interface(db);
            if !method_candidates.iter().any(|(r, ..)| *r == realized) {
                method_candidates.push((
                    realized,
                    resolved_impl.impl_loc(),
                    data.interface_loc(),
                    data,
                    resolved_method,
                ));
            }
        }
        let field_sources = self.concrete_interface_field_sources(&impls, member);

        // Interface fields are reachable only through an explicit projection.
        if field_sources.len() >= 2 {
            let sources = field_sources
                .iter()
                .map(|s| Name::new(self.qualified_interface_display(&s.interface)))
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
            let interface_name = Name::new(self.qualified_interface_display(&source.interface));
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
                .map(|(realized, ..)| self.qualified_interface_display(realized))
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
        let (realized, impl_loc, iface_loc, data, resolved_method) =
            method_candidates.into_iter().next()?;

        // The fully source-backed case preserves its concrete-resolution locs.
        // Any mounted side is loc-free and routes through the interface slot;
        // obtain the same symbolic declaration surface from the package export
        // and let `build_foreign_interface_method_ty` realize it at `base_ty`.
        let Some(func_loc) = resolved_method.method_loc() else {
            return self.resolve_member_from_external_impl(base_ty, member, at, bound, &realized);
        };
        let (Some(impl_loc), Some(iface_loc)) = (impl_loc, iface_loc) else {
            return self.resolve_member_from_external_impl(base_ty, member, at, bound, &realized);
        };

        // The realized interface declares the member; build its `Ty::Function` with `Self`
        // pinned to the concrete receiver (the impl conforms, so the signature is the
        // interface's). The view lowers the interface's declared types in its own package.
        let access = MemberAccess { member, at, bound };
        let view = InterfaceView {
            loc: iface_loc,
            realized,
        };
        let ty =
            self.resolve_member_on_one_interface(&view, SelfReceiver::ExactTy(base_ty), &access)?;

        // The helper recorded a *virtual* call; overwrite it — the impl is statically known,
        // so dispatch is concrete. For an override, also replace the interface-framed generic
        // seed with the impl's frame (the override body is keyed by the impl's own params).
        self.resolutions.insert(
            at,
            crate::inference::MemberResolution::InterfaceConcreteMethod { impl_loc, func_loc },
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

    /// Resolve a concrete member supplied by a loc-free impl through the
    /// implemented interface's exported declaration row. The row may itself
    /// be mounted or source-backed in this database; cloning it before the
    /// builder mutation keeps the Salsa borrow boundary explicit.
    fn resolve_member_from_external_impl(
        &mut self,
        base_ty: &Ty,
        member: &Name,
        at: ExprId,
        bound: bool,
        realized: &baml_type::Interface,
    ) -> Option<Ty> {
        use crate::package_interface::ExportedType;

        let db = self.context.db();
        let exported = crate::package_interface::mounted_type_row(db, &realized.name)
            .cloned()
            .or_else(|| {
                let pkg = baml_compiler2_hir::package::PackageId::new(
                    db,
                    realized.name.package().clone(),
                );
                crate::package_interface::package_interface(db, pkg)
                    .lookup_type(realized.name.namespace(), realized.name.name())
                    .cloned()
            })?;
        let ExportedType::Interface {
            qtn,
            self_param,
            generic_params,
            required_methods,
            default_methods,
            ..
        } = exported
        else {
            return None;
        };
        let method = required_methods
            .iter()
            .chain(&default_methods)
            .find(|method| method.name == *member)?;
        self.build_foreign_interface_method_ty(
            &qtn,
            &self_param,
            &generic_params,
            realized,
            method,
            SelfReceiver::ExactTy(base_ty),
            &MemberAccess { member, at, bound },
        )
        .into()
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
            self.aliases,
            |a, b| self.is_subtype(a, b),
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
            let Some(data) = resolved_impl.data(db) else {
                continue;
            };
            // Mounted interfaces export no field declarations reachable here;
            // interface-field views through mounted impls land with the
            // call-lowering PR.
            let Some(iface_loc) = data.interface_loc() else {
                continue;
            };
            let declares_field = baml_compiler2_ppir::item_data::interface_data(db, iface_loc)
                .fields
                .iter()
                .any(|f| &f.name == field);
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
                iface_loc,
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
            // A union member is reachable only through an interface every arm shares. When
            // none does, the arms may each declare `member` via *different* interfaces —
            // those are distinct methods, so the union has no single one. Say so, rather
            // than the bare "has no member" (which wrongly implies no arm declares it).
            UnionMemberResolution::Unresolved(TirTypeError::UnionMemberNoCommonInterface {
                union: union_ty.clone(),
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
                    .map(|(ai, _)| self.qualified_interface_display(&ai.interface))
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
                crate::interfaces::interface_closure_locs_with_args_and_assoc(
                    db, root_loc, args, assoc, true,
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
            Ty::TypeVar(name, _) => {
                // A bounded type variable's arm interfaces are those of every interface in
                // its bound conjunction (`T: A & B`).
                let bounds = self
                    .generic_param_bounds
                    .get(name)
                    .cloned()
                    .unwrap_or_default();
                bounds
                    .iter()
                    .flat_map(|iface| self.union_arm_interfaces(&iface.to_ty()))
                    .collect()
            }
            // A concrete arm's interfaces come from its own impls, with each impl's bindings
            // substituted into the interface head — so a blanket `impl<U> I<U> for Box<U>` at
            // `Box<int>` yields the *realized* `I<int>`, not the impl-space `I<U>`.
            _ => self
                .type_impls(arm)
                .iter()
                .filter_map(|resolved_impl| {
                    // Mounted rows without a source-backed interface loc are
                    // skipped: the union-member view reads the interface's
                    // declared members through its loc (call-lowering PR).
                    let iface_loc = resolved_impl.data(db)?.interface_loc()?;
                    Some(ArmInterface {
                        iface_loc,
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
        let iface_data = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
        if iface_data.fields.iter().any(|field| &field.name == member) {
            return Some(UnionMemberKind::Field);
        }
        if iface_data.default_methods.iter().any(|&fn_loc| {
            baml_compiler2_ppir::item_data::function_data(db, fn_loc).name == *member
        }) || iface_data
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
        let view = InterfaceView {
            loc: ai.iface_loc,
            realized: ai.interface.clone(),
        };
        self.resolve_member_on_one_interface(
            &view,
            SelfReceiver::Union(union_ty),
            &MemberAccess { member, at, bound },
        )
    }

    /// Resolve `member` on **one** interface instantiation — its own fields and methods,
    /// with **no** `requires`-closure walk. [`Self::resolve_interface_member`] calls this
    /// per interface in an existential/type-var receiver's bound closure.
    fn resolve_member_on_one_interface(
        &mut self,
        view: &InterfaceView<'db>,
        recv: SelfReceiver<'_>,
        access: &MemberAccess<'_>,
    ) -> Option<Ty> {
        let db = self.context.db();
        let iface_data = baml_compiler2_ppir::item_data::interface_data(db, view.loc);

        // Field lookup: this interface's own fields (no closure). The enumeration index
        // *is* the field's dispatch index — see `MemberResolution::InterfaceVirtualField`.
        for (field_index, field) in iface_data.fields.iter().enumerate() {
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
            let ty = {
                // A field lowers in the same environment as a method signature — so
                // `key: Self.Key` resolves through the receiver's pins/impls exactly
                // as a `-> Self.Key` return would. Fields require a bound receiver
                // (checked above), so no fresh receiver generic is ever needed.
                let env = self.interface_member_lowering_env(view, recv, &[], false);
                let mut diags = env.diags;
                let ns = view.namespace(db);
                let ty = crate::generics::substitute_ty(
                    &crate::lower_type_expr::lower_type_ref(
                        &iface_data.type_refs,
                        field.type_ref,
                        &crate::lower_type_expr::ScopeCtx {
                            db,
                            package_items: view.pkg_items(db),
                            ns_context: &ns,
                            generic_params: &env.all_generic_params,
                            bounds: &env.bounds,
                            self_ty: Some(env.self_ty.clone()),
                        },
                        &mut diags,
                    ),
                    &env.bindings,
                );
                let span = baml_compiler2_ppir::item_data::interface_source_map(db, view.loc)
                    .type_refs
                    .span(field.type_ref);
                for diag in diags {
                    self.context.report_at_span(diag, span);
                }
                ty
            };
            self.resolutions.insert(
                access.at,
                crate::inference::MemberResolution::InterfaceVirtualField {
                    iface_loc: view.loc,
                    interface: view.realized.to_ty(),
                    field_index: u32::try_from(field_index)
                        .expect("interface field count fits u32"),
                    field: access.member.clone(),
                },
            );
            return Some(ty);
        }

        // Method lookup: the interface's default body, else its required signature.
        // Both normalize to one `InterfaceMethodSpec` and one builder.
        if let Some(&func_loc) = iface_data.default_methods.iter().find(|&&fn_loc| {
            baml_compiler2_ppir::item_data::function_data(db, fn_loc).name == *access.member
        }) {
            let spec = InterfaceMethodSpec::from_default(db, func_loc);
            return Some(self.build_interface_method_ty(view, recv, &spec, access));
        }
        if let Some(sig) = iface_data
            .required_methods
            .iter()
            .find(|sig| sig.name == *access.member)
        {
            let spec = InterfaceMethodSpec::from_required(iface_data, sig);
            return Some(self.build_interface_method_ty(view, recv, &spec, access));
        }
        None
    }

    /// Interface field `(name, type)` pairs in requires-closure order, each field lowered
    /// in the same environment as the member-access path (`Self` = the receiver
    /// existential, so a `Self.Assoc` field type collapses through the receiver's pins).
    /// Side-effect-free for matrix construction: lowering diagnostics are dropped — the
    /// decl-validation pass reports a field type's own errors, and the member-access path
    /// reports receiver-specific ones.
    pub(super) fn interface_field_infos_ordered_for_ty(&self, iface_ty: &Ty) -> Vec<(Name, Ty)> {
        let mut out = Vec::new();
        let mut seen = rustc_hash::FxHashSet::default();
        let Ty::Interface(iface_name, iface_type_args, associated_bindings, _) = iface_ty else {
            return out;
        };
        let Some(pkg_items) = self.resolve_class_pkg_items(iface_name.package()) else {
            return out;
        };
        let Some(baml_compiler2_hir::contributions::Definition::Interface(root_loc)) =
            pkg_items.lookup_type(iface_name.namespace(), iface_name.name())
        else {
            return out;
        };

        let db = self.context.db();
        for (iface_loc, closure_args, closure_assoc) in
            crate::interfaces::interface_closure_locs_with_args_and_assoc(
                db,
                root_loc,
                iface_type_args,
                associated_bindings,
                true,
            )
        {
            let iface_data = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
            if iface_data.fields.is_empty() {
                continue;
            }
            let Some(qtn) = crate::interfaces::interface_loc_qtn(db, iface_loc) else {
                continue;
            };
            // `Self` stays the receiver existential (`iface_ty`, the closure root) for
            // every closure entry — a required interface's `Self.Assoc` field resolves
            // through the root's closure exactly as it would on a member access.
            let view = InterfaceView {
                loc: iface_loc,
                realized: baml_type::Interface::new(qtn, closure_args, closure_assoc),
            };
            let env = self.interface_member_lowering_env(
                &view,
                SelfReceiver::Existential(iface_ty),
                &[],
                false,
            );
            let ns = view.namespace(db);
            let mut diags = env.diags;
            for field in &iface_data.fields {
                if !seen.insert(field.name.clone()) {
                    continue;
                }
                let ty = crate::generics::substitute_ty(
                    &crate::lower_type_expr::lower_type_ref(
                        &iface_data.type_refs,
                        field.type_ref,
                        &crate::lower_type_expr::ScopeCtx {
                            db,
                            package_items: view.pkg_items(db),
                            ns_context: &ns,
                            generic_params: &env.all_generic_params,
                            bounds: &env.bounds,
                            self_ty: Some(env.self_ty.clone()),
                        },
                        &mut diags,
                    ),
                    &env.bindings,
                );
                out.push((field.name.clone(), ty));
            }
        }

        out
    }

    /// Build the lowering environment for one interface member (field or method) resolved on
    /// a receiver — everything the member's declared type expression(s) need to lower: the
    /// in-scope type-variable names, their bounds, what `Self` lowers to, and the final
    /// substitution map. One construction shared by the field and method paths, so a field's
    /// `Self.Assoc` type resolves exactly as a method signature's.
    fn interface_member_lowering_env(
        &self,
        view: &InterfaceView<'db>,
        recv: SelfReceiver<'_>,
        extra_generic_names: &[Name],
        symbolic_receiver: bool,
    ) -> MemberLoweringEnv {
        let db = self.context.db();
        let data = baml_compiler2_ppir::item_data::interface_data(db, view.loc);
        let interface_env = crate::generic_env::interface_generic_env(db, view.loc);
        let (self_param, interface_params) = interface_env.interface_param_parts();
        let self_param = self_param.clone();
        let mut all_generic_params = interface_env.params().to_vec();
        let inherited_count = all_generic_params.len();
        crate::ty::ParamTy::extend_frame(&mut all_generic_params, extra_generic_names);
        let method_params = &all_generic_params[inherited_count..];
        let prefer_symbolic = matches!(recv, SelfReceiver::RigidVar(_));
        let mut diags = Vec::new();

        // Interface generic args bound to their params; an unbound param stays its own type
        // variable. Extra (method) generics likewise. Associated types are resolved below.
        let iface_args: Vec<Ty> = if view.realized.generics.is_empty() {
            interface_params
                .iter()
                .map(|gp| Ty::TypeVar(gp.clone(), TyAttr::default()))
                .collect()
        } else {
            view.realized.generics.clone()
        };
        let mut base_bindings = crate::generics::bind_type_vars(interface_params, &iface_args);
        for generic_param in interface_params.iter().chain(method_params) {
            base_bindings
                .entry(generic_param.clone())
                .or_insert_with(|| Ty::TypeVar(generic_param.clone(), TyAttr::default()));
        }

        // Resolve each associated type to a pin (explicit binding or usable default); unpinned
        // associated types have no pin and lower to a symbolic projection below. The interface
        // bound carries the pins so a pinned `Self.Assoc` collapses to its concrete type.
        let mut pins: Vec<(Name, Ty)> = Vec::new();
        for assoc in &data.associated_types {
            let mut prior = base_bindings.clone();
            for (name, ty) in &pins {
                let param = interface_env
                    .resolve_any_param(name)
                    .expect("associated type parameter is in its interface environment")
                    .clone();
                prior.insert(param, ty.clone());
            }
            if let Some(ty) = self.associated_type_pin(view, assoc, prefer_symbolic, &prior) {
                pins.push((assoc.name.clone(), ty));
            }
        }
        // The realized interface with only its *explicit* associated bindings — the constraint
        // recorded for the receiver generic's call-site enforcement. Defaults are deliberately
        // omitted: `Self: Iterator` must admit implementors that override an associated type, so
        // pinning `Error = never` (a default) here would wrongly reject them.
        let iface_ty = baml_type::Interface::new(
            view.realized.name.clone(),
            iface_args.clone(),
            view.realized.associated_types.clone(),
        )
        .to_ty();
        // The interface *constraint* that `Self.Assoc` projections resolve through — carrying
        // the resolved pins (explicit bindings plus usable defaults) so a pinned projection
        // collapses to its concrete type. A `baml_type::Interface` constraint, never a
        // `Ty::Interface` existential: it pins only what's known, which the all-or-nothing
        // existential could not represent.
        let iface_bound =
            baml_type::Interface::new(view.realized.name.clone(), iface_args, pins.clone());

        // `Self` for this receiver, plus the type-variable name (if any) whose bound is the
        // interface: a rigid or unbound-existential `Self` is a type variable resolving
        // `Self.Assoc` through its bound; an exact or union `Self` is the receiver type itself;
        // a bound-existential `Self` is the receiver's own (complete) interface existential.
        let (self_ty, self_bound_name) = match recv {
            SelfReceiver::RigidVar(param) => (
                Ty::TypeVar(param.clone(), TyAttr::default()),
                Some(param.clone()),
            ),
            SelfReceiver::ExactTy(ty) | SelfReceiver::Union(ty) => (ty.clone(), None),
            SelfReceiver::Existential(_) if symbolic_receiver => (
                Ty::TypeVar(self_param.clone(), TyAttr::default()),
                Some(self_param),
            ),
            SelfReceiver::Existential(existential_ty) => (existential_ty.clone(), None),
        };

        // The lowering scope's bounds, innermost binding wins:
        //   1. the *enclosing* scope's — the receiver type may carry the caller's rigid type
        //      variables (`us: U[]` with `U extends HasErr`), whose bounds must be visible to
        //      discharge a blanket impl's constraint (`implement<T extends HasErr> Wrap for
        //      T[]`) when a `Self.Assoc` projection resolves through the receiver's impls;
        //   2. shadowed by the member's own scope names (the signature is written in the
        //      interface's scope — a caller variable sharing a name must not leak a bound in);
        //   3. the interface's own declared param bounds, and `Self`'s interface bound.
        let mut bounds = self.generic_param_bounds.clone();
        for param in &all_generic_params {
            bounds.remove(param);
        }
        for (param, param_bounds) in
            crate::lower_type_expr::interface_generic_param_bounds(db, view.loc)
        {
            bounds.insert(param.clone(), param_bounds.clone());
        }
        if let Some(param) = &self_bound_name {
            bounds.insert(param.clone(), vec![iface_bound]);
        }

        // Final substitution map: interface args, extra generics, and each associated type
        // resolved to its pin or its symbolic `Self.Assoc` projection — so a bare `Assoc`
        // reference specializes identically to `Self.Assoc`.
        let mut bindings = base_bindings;
        {
            let ns = view.namespace(db);
            let ctx = crate::lower_type_expr::ScopeCtx {
                db,
                package_items: view.pkg_items(db),
                ns_context: &ns,
                generic_params: &all_generic_params,
                bounds: &bounds,
                self_ty: Some(self_ty.clone()),
            };
            for assoc in &data.associated_types {
                let value = if let Some((_, ty)) = pins.iter().find(|(name, _)| name == &assoc.name)
                {
                    ty.clone()
                } else {
                    let lowered = crate::builder::associated_projection::lower_projection(
                        &ctx,
                        self_ty.clone(),
                        None,
                        assoc.name.clone(),
                    );
                    diags.extend(lowered.diagnostics);
                    lowered.ty
                };
                let param = interface_env
                    .resolve_any_param(&assoc.name)
                    .expect("associated type parameter is in its interface environment")
                    .clone();
                bindings.insert(param, value);
            }
        }

        MemberLoweringEnv {
            all_generic_params,
            bounds,
            self_ty,
            iface_ty,
            bindings,
            diags,
        }
    }

    /// The pinned type for associated type `assoc` on `view`: its explicit binding (from the
    /// realized interface), or its default — lowered against `prior` (already-resolved generics
    /// and earlier associated types) — unless the receiver keeps `Self.Assoc` symbolic (a rigid
    /// `Self` in an interface's own default body must stay polymorphic over implementors that
    /// override it). `None` = unpinned: the caller supplies the symbolic projection (member
    /// lowering) or an error placeholder (field lowering).
    fn associated_type_pin(
        &self,
        view: &InterfaceView<'db>,
        assoc: &baml_compiler2_ppir::item_data::AssociatedTypeData,
        prefer_symbolic: bool,
        prior: &rustc_hash::FxHashMap<crate::ty::ParamTy, Ty>,
    ) -> Option<Ty> {
        if let Some((_, ty)) = view
            .realized
            .associated_types
            .iter()
            .find(|(name, _)| name == &assoc.name)
        {
            return Some(ty.clone());
        }
        // A rigid `Self` (an interface's own default body) keeps `Self.Assoc` symbolic — an
        // implementor may override the default — so the caller supplies the projection.
        if prefer_symbolic {
            return None;
        }
        let db = self.context.db();
        let (default, _diags) =
            crate::interfaces::interface_associated_type_default(db, view.loc, assoc.name.clone())?;
        // Fill the default eagerly at the receiver: `Self` is this interface realized on it (its
        // pins resolved so far), so a Self-referencing default (`type Items = Self.Item[]`)
        // reduces against them. The default was lowered once (symbolic `Self`) by the shared
        // query; realize substitutes this receiver for `Self` and the realized generic args.
        let interface_env = crate::generic_env::interface_generic_env(db, view.loc);
        let (self_param, iface_generic_params) = interface_env.interface_param_parts();
        let self_ty = Ty::Interface(
            view.realized.name.clone(),
            view.realized.generics.clone(),
            view.realized.associated_types.clone(),
            TyAttr::default(),
        );
        let realized = crate::interfaces::realize_associated_default(
            &default,
            iface_generic_params,
            &view.realized.generics,
            self_param,
            &self_ty,
        );
        Some(crate::generics::substitute_ty(&realized, prior))
    }

    /// Build the `Ty::Function` for an interface method from its normalized
    /// [`InterfaceMethodSpec`], resolved on one interface instantiation. Handles the
    /// `Self` placeholder (rigid var / exact receiver / existential), associated-type
    /// bindings, generic binding and bound recording, and the `bound`-receiver `self`
    /// strip — and records the `InterfaceVirtualMethod` slot resolution (the concrete path
    /// overwrites it with `InterfaceConcreteMethod` once the impl is known).
    fn build_interface_method_ty(
        &mut self,
        view: &InterfaceView<'db>,
        recv: SelfReceiver<'_>,
        spec: &InterfaceMethodSpec,
        access: &MemberAccess<'_>,
    ) -> Ty {
        let db = self.context.db();
        let data = baml_compiler2_ppir::item_data::interface_data(db, view.loc);
        let ns = view.namespace(db);
        let pkg_items = view.pkg_items(db);

        let rigid_pin: Option<crate::ty::ParamTy> = match recv {
            SelfReceiver::RigidVar(pin) => Some(pin.clone()),
            _ => None,
        };
        let generic_names: Vec<Name> = spec.generic_param_names();

        let symbolic_receiver =
            !access.bound && !matches!(recv, SelfReceiver::ExactTy(_) | SelfReceiver::Union(_));
        let interface_env = crate::generic_env::interface_generic_env(db, view.loc);
        let receiver_generic =
            symbolic_receiver.then(|| interface_env.interface_param_parts().0.clone());

        let MemberLoweringEnv {
            all_generic_params,
            bounds,
            self_ty,
            iface_ty,
            bindings,
            mut diags,
        } = self.interface_member_lowering_env(view, recv, &generic_names, symbolic_receiver);
        let method_generic_params = generic_names
            .iter()
            .map(|name| {
                all_generic_params
                    .iter()
                    .rev()
                    .find(|param| param.name() == name)
                    .expect("method generic parameter is in its lowering environment")
                    .clone()
            })
            .collect::<Vec<_>>();

        let ctx = crate::lower_type_expr::ScopeCtx {
            db,
            package_items: pkg_items,
            ns_context: &ns,
            generic_params: &all_generic_params,
            bounds: &bounds,
            self_ty: Some(self_ty),
        };

        // Object safety only applies to a bare existential / union receiver (a pinned or
        // rigid `Self` is a single concrete type, so all `Self` usage is sound there). A
        // method is unsafe to dispatch through such a receiver when it uses bare `Self`
        // outside the receiver position:
        //   - a non-`self` parameter typed with `Self` (the concrete implementor is unknown
        //     for those arguments — the multi-`Self` problem), OR
        //   - `Self` nested inside an invariant constructor in the return/throws type
        //     (`-> Self[]`, `-> Box<Self>`): the impl returns a concretely-tagged container
        //     that is not a subtype of the existential-tagged one. A bare top-level `-> Self`
        //     stays legal (it collapses covariantly to the receiver).
        // A `Self.Assoc` projection is exempt in both positions: the existential's pins
        // (or the assoc default) make it one concrete type for every member.
        if matches!(recv, SelfReceiver::Existential(_) | SelfReceiver::Union(_)) && access.bound {
            let contains_bare_self = |slot: SigTypeRef| match slot {
                SigTypeRef::Id(id) => Self::type_ref_contains_bare_self(spec.sig_refs, id),
                // The desugared receiver IS bare `Self` (excluded below by name).
                SigTypeRef::SelfReceiver => true,
                SigTypeRef::Missing => false,
            };
            let self_in_invariant_position = |slot: SigTypeRef| match slot {
                SigTypeRef::Id(id) => Self::type_ref_self_in_invariant_position(spec.sig_refs, id),
                SigTypeRef::SelfReceiver | SigTypeRef::Missing => false,
            };
            let param_self = spec
                .args
                .iter()
                .chain(&spec.kwargs)
                .filter(|p| p.name.as_str() != "self")
                .any(|p| contains_bare_self(p.ty));
            let position = if param_self {
                Some(SelfCallPosition::Parameter)
            } else if self_in_invariant_position(spec.return_type)
                || self_in_invariant_position(spec.throws)
            {
                Some(SelfCallPosition::NestedInReturn)
            } else {
                None
            };
            if let Some(position) = position {
                self.context.report_simple(
                    TirTypeError::InvalidSelfCallThroughInterface {
                        interface_name: data.name.clone(),
                        method_name: access.member.clone(),
                        position,
                    },
                    access.at,
                );
            }
        }

        // Lower the declared signature to a type-constructor template through `ctx` (which
        // resolves `Self` and `Self.Assoc`), then specialize it — binding the interface/method
        // generics and associated types. The `bound`-receiver `self` strip happens after.
        let mut fn_ty =
            crate::generics::substitute_ty(&spec.to_function_ty(&ctx, &mut diags), &bindings);

        // Track the method's generic params *and* their bounds so call-site bound
        // enforcement works without the function type carrying them: the receiver generic
        // is bounded by the interface, the method's own params by their declared bounds.
        let mut function_generic_params = Vec::new();
        let mut function_generic_param_bounds: Vec<Vec<Ty>> = Vec::new();
        if let Some(receiver_generic) = &receiver_generic {
            function_generic_params.push(receiver_generic.clone());
            function_generic_param_bounds.push(vec![iface_ty.clone()]);
            // The synthetic receiver bound is an interface; store it as the conjunction.
            self.generic_param_bounds.insert(
                receiver_generic.clone(),
                iface_ty.as_interface().into_iter().collect(),
            );
        }
        function_generic_params.extend(method_generic_params);
        function_generic_param_bounds.extend(lower_generic_param_bound_refs(
            db,
            spec.bound_store(),
            spec.generic_bounds(),
            pkg_items,
            &ns,
            &all_generic_params,
            Some(&bindings),
            &mut diags,
        ));
        for diag in diags {
            self.context.report(diag, access.at, Vec::new());
        }

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
                iface_loc: view.loc,
                method: access.member.clone(),
            },
        );
        let owner_type_arg_bindings = data
            .generic_params
            .iter()
            .filter_map(|declared| {
                let param = interface_env.resolve_param(&declared.name)?;
                bindings.get(param).cloned().map(|ty| (param.clone(), ty))
            })
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

    /// Resolve `access.member` on a receiver whose bound names a MOUNTED
    /// (source-less) interface — the loc-free foreign twin of
    /// [`Self::resolve_interface_member`] (BEP-066 mounted-package linking). The exported row
    /// carries pre-lowered symbolic-`Self` signatures and the pre-flattened
    /// `requires` closure, so root-wins tiering runs over rows and realization
    /// at this receiver is pure substitution. A `requires` parent that is
    /// source-backed in THIS database (a mounted interface requiring a stdlib
    /// one) delegates to the ordinary loc view.
    ///
    /// Mounted interface FIELDS deliberately stay unresolved here (fail-closed
    /// residue: the virtual-field view's loc-free lowering lands in a
    /// follow-up), and unbound (`I.method`) value accesses never reach this
    /// path (they stay call-reserved upstream).
    fn resolve_member_on_mounted_interface(
        &mut self,
        bound: InterfaceBound<'_>,
        recv: SelfReceiver<'_>,
        access: MemberAccess<'_>,
    ) -> Option<Ty> {
        use crate::package_interface::ExportedType;
        let db = self.context.db();
        let ExportedType::Interface {
            qtn,
            self_param,
            generic_params,
            fields,
            required_methods,
            default_methods,
            requires,
            ..
        } = crate::package_interface::mounted_type_row(db, bound.name)?
        else {
            return None;
        };

        // Root-wins tier: the mounted root's own declaration surface shadows
        // everything it transitively requires.
        if let Some(method) = required_methods
            .iter()
            .chain(default_methods)
            .find(|m| m.name == *access.member)
        {
            let realized = baml_type::Interface::new(
                qtn.clone(),
                bound.type_args.to_vec(),
                bound.associated_bindings.to_vec(),
            );
            return Some(self.build_foreign_interface_method_ty(
                qtn,
                self_param,
                generic_params,
                &realized,
                method,
                recv,
                &access,
            ));
        }
        if fields.iter().any(|(name, _, _)| name == access.member) {
            // Field residue: unresolved (never a fabricated view).
            return None;
        }

        // Transitive tier: the pre-flattened `requires` closure realized at
        // the bound's args, deduped by realized identity; declarers split by
        // which declaration surface they have HERE.
        let bindings = crate::generics::bind_type_vars(generic_params, bound.type_args);
        let mut source_declarers: Vec<InterfaceView<'db>> = Vec::new();
        let mut foreign_declarers: Vec<baml_type::Interface> = Vec::new();
        for required in requires {
            let realized = required.map_tys(|ty| crate::generics::substitute_ty(ty, &bindings));
            let req_pkg =
                baml_compiler2_hir::package::PackageId::new(db, realized.name.package().clone());
            match baml_compiler2_ppir::package_items(db, req_pkg)
                .lookup_type(realized.name.namespace(), realized.name.name())
            {
                Some(Definition::Interface(loc)) => {
                    if self.interface_member_kind(loc, access.member).is_some()
                        && !source_declarers.iter().any(|v| v.realized == realized)
                    {
                        source_declarers.push(InterfaceView { loc, realized });
                    }
                }
                _ => {
                    if let Some(ExportedType::Interface {
                        required_methods,
                        default_methods,
                        ..
                    }) = crate::package_interface::mounted_type_row(db, &realized.name)
                        && required_methods
                            .iter()
                            .chain(default_methods)
                            .any(|m| m.name == *access.member)
                        && !foreign_declarers.contains(&realized)
                    {
                        foreign_declarers.push(realized);
                    }
                }
            }
        }
        match (source_declarers.as_slice(), foreign_declarers.as_slice()) {
            ([], []) => None,
            ([one], []) => {
                let view = one.clone();
                self.resolve_member_on_one_interface(&view, recv, &access)
            }
            ([], [_one]) => {
                let realized = foreign_declarers.into_iter().next().expect("one declarer");
                let ExportedType::Interface {
                    qtn,
                    self_param,
                    generic_params,
                    required_methods,
                    default_methods,
                    ..
                } = crate::package_interface::mounted_type_row(db, &realized.name)?
                else {
                    return None;
                };
                let method = required_methods
                    .iter()
                    .chain(default_methods)
                    .find(|m| m.name == *access.member)?;
                Some(self.build_foreign_interface_method_ty(
                    qtn,
                    self_param,
                    generic_params,
                    &realized,
                    method,
                    recv,
                    &access,
                ))
            }
            // ≥2 incomparable declarers (any mix of surfaces): ambiguous,
            // exactly as `arbitrate_member_declarers` reports it.
            _ => {
                let receiver = match &recv {
                    SelfReceiver::RigidVar(name) => name.to_string(),
                    SelfReceiver::ExactTy(ty)
                    | SelfReceiver::Existential(ty)
                    | SelfReceiver::Union(ty) => ty.render_user_facing(),
                };
                let sources: Vec<String> = source_declarers
                    .iter()
                    .map(|v| self.qualified_interface_display(&v.realized))
                    .chain(
                        foreign_declarers
                            .iter()
                            .map(|r| self.qualified_interface_display(r)),
                    )
                    .collect();
                self.context.report_at_member_simple(
                    TirTypeError::AmbiguousInterfaceMethod {
                        class_name: Name::new(&receiver),
                        method_name: access.member.clone(),
                        sources,
                    },
                    access.at,
                );
                Some(Ty::Unknown {
                    attr: TyAttr::default(),
                })
            }
        }
    }

    /// Build the `Ty::Function` for a MOUNTED interface's method resolved on a
    /// receiver — the substitution twin of [`Self::build_interface_method_ty`]
    /// (BEP-066 mounted-package linking). The exported signature keeps `Self` symbolic (and
    /// associated types as `Self.<name>` projections on it), so realization is
    /// one substitution: interface params to the realized args, `Self` to the
    /// receiver; residual projections reduce under `normalize` through the
    /// receiver's pins/impls. Records the loc-free `External` resolution
    /// (interface-slot routing — the VM resolves the receiver's registered
    /// impl) plus the same call-site seeds the source path records.
    #[expect(
        clippy::too_many_arguments,
        reason = "the exported row's decomposed fields plus the receiver/access pair"
    )]
    fn build_foreign_interface_method_ty(
        &mut self,
        iface_qtn: &crate::ty::QualifiedTypeName,
        self_param: &crate::ty::ParamTy,
        iface_generic_params: &[crate::ty::ParamTy],
        realized: &baml_type::Interface,
        method: &crate::package_interface::ExportedFunction,
        recv: SelfReceiver<'_>,
        access: &MemberAccess<'_>,
    ) -> Ty {
        // `Self` for this receiver. The foreign path serves bound accesses and
        // rigid receivers; an exact/union/existential receiver pins `Self` to
        // its own type (the existential's pins travel on the type itself).
        let (self_ty, rigid_pin) = match recv {
            SelfReceiver::RigidVar(param) => (
                Ty::TypeVar(param.clone(), TyAttr::default()),
                Some(param.clone()),
            ),
            SelfReceiver::ExactTy(ty) | SelfReceiver::Union(ty) | SelfReceiver::Existential(ty) => {
                (ty.clone(), None)
            }
        };

        // Object safety on a bare existential/union receiver, on the exported
        // symbolic-`Self` surface: a non-receiver parameter mentioning bare
        // `Self` (projections exempt) is uncallable; `Self` nested in an
        // invariant constructor in return/throws likewise (a bare top-level
        // `-> Self` collapses covariantly). Mirrors the source arm.
        if matches!(recv, SelfReceiver::Existential(_) | SelfReceiver::Union(_)) && access.bound {
            let param_self = method
                .params
                .iter()
                .filter(|p| p.name.as_ref().map(Name::as_str) != Some("self"))
                .any(|p| mentions_bare_self(&p.ty, self_param));
            let position = if param_self {
                Some(SelfCallPosition::Parameter)
            } else if self_nested_in_invariant_position(&method.return_type, self_param)
                || self_nested_in_invariant_position(&method.callable_throws, self_param)
            {
                Some(SelfCallPosition::NestedInReturn)
            } else {
                None
            };
            if let Some(position) = position {
                self.context.report_simple(
                    TirTypeError::InvalidSelfCallThroughInterface {
                        interface_name: iface_qtn.name().clone(),
                        method_name: access.member.clone(),
                        position,
                    },
                    access.at,
                );
            }
        }

        let mut bindings =
            crate::generics::bind_type_vars(iface_generic_params, &realized.generics);
        bindings.insert(self_param.clone(), self_ty);
        for gp in &method.generic_params {
            bindings
                .entry(gp.clone())
                .or_insert_with(|| Ty::TypeVar(gp.clone(), TyAttr::default()));
        }
        let substitute = |ty: &Ty| crate::generics::substitute_ty(ty, &bindings);

        let takes_self = method
            .params
            .first()
            .and_then(|p| p.name.as_ref())
            .is_some_and(|n| n.as_str() == "self");
        let mut params: Vec<crate::ty::FunctionParamTy> = method
            .params
            .iter()
            .map(|p| crate::ty::FunctionParamTy {
                name: p.name.clone(),
                ty: substitute(&p.ty),
                mode: p.mode,
            })
            .collect();
        if access.bound {
            params = crate::generics::skip_self_param(&params).to_vec();
        }
        let fn_ty = Ty::Function {
            params,
            ret: Box::new(substitute(&method.return_type)),
            throws: Box::new(substitute(&method.callable_throws)),
            attr: TyAttr::default(),
        };

        // The call-site facts, realized at this receiver: the loc-free
        // `External` resolution (the interface's method slot — virtual
        // dispatch), the interface args seed for bound checks that reference
        // owner params, and the rigid `Self` pin.
        let realized_bounds: Vec<Vec<baml_type::Interface>> = method
            .generic_param_bounds
            .iter()
            .map(|conjunction| {
                conjunction
                    .iter()
                    .map(|bound| bound.map_tys(&substitute))
                    .collect()
            })
            .collect();
        self.resolutions.insert(
            access.at,
            crate::inference::MemberResolution::External(std::sync::Arc::new(
                crate::inference::ExternalCallable {
                    target: crate::inference::ExternalCallTarget::Interface {
                        iface: iface_qtn.clone(),
                        method: method.name.clone(),
                    },
                    takes_self,
                    owner_generic_params: Vec::new(),
                    generic_params: method.generic_params.clone(),
                    generic_param_bounds: realized_bounds,
                },
            )),
        );
        let owner_bindings: Vec<(crate::ty::ParamTy, Ty)> = iface_generic_params
            .iter()
            .cloned()
            .zip(realized.generics.iter().cloned())
            .collect();
        if !owner_bindings.is_empty() {
            self.owner_type_arg_binding_seed
                .insert(access.at, owner_bindings);
        }
        if let Some(pin) = rigid_pin {
            self.self_pinned_rigid_var.insert(access.at, pin);
        }
        fn_ty
    }
}

/// Whether `ty` mentions the symbolic `Self` parameter OUTSIDE an
/// associated-type projection base (`Self` is bare; `Self.Item` is exempt —
/// the receiver's pins make it one concrete type). The exported-row twin of
/// `type_ref_contains_bare_self`, walking the already-lowered `Ty`.
fn mentions_bare_self(ty: &Ty, self_param: &crate::ty::ParamTy) -> bool {
    match ty {
        Ty::TypeVar(param, _) => param == self_param,
        // The projection base is exempt; its qualifying interface's own types
        // are still walked (a `(Self as I<Self>)`-shaped qualifier is bare use).
        Ty::AssociatedTypeProjection { interface, .. } => {
            interface.tys().any(|t| mentions_bare_self(t, self_param))
        }
        Ty::List(inner, _) | Ty::EvolvingList(inner, _) => mentions_bare_self(inner, self_param),
        Ty::Map {
            key: k, value: v, ..
        }
        | Ty::EvolvingMap(k, v, _) => {
            mentions_bare_self(k, self_param) || mentions_bare_self(v, self_param)
        }
        Ty::Union(tys, _) => tys.iter().any(|t| mentions_bare_self(t, self_param)),
        Ty::Future(value, error, _) => {
            mentions_bare_self(value, self_param) || mentions_bare_self(error, self_param)
        }
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            params
                .iter()
                .any(|param| mentions_bare_self(&param.ty, self_param))
                || mentions_bare_self(ret, self_param)
                || mentions_bare_self(throws, self_param)
        }
        Ty::Class(_, type_args, _) => type_args.iter().any(|t| mentions_bare_self(t, self_param)),
        Ty::Interface(_, type_args, associated_bindings, _) => {
            type_args.iter().any(|t| mentions_bare_self(t, self_param))
                || associated_bindings
                    .iter()
                    .any(|(_, ty)| mentions_bare_self(ty, self_param))
        }
        _ => false,
    }
}

/// Whether bare `Self` appears NESTED inside `ty` — anywhere except as the
/// whole type — the invariant-constructor half of the object-safety rule for
/// return/throws positions (`-> Self` collapses covariantly; `-> Self[]` /
/// `-> Box<Self>` do not).
fn self_nested_in_invariant_position(ty: &Ty, self_param: &crate::ty::ParamTy) -> bool {
    if matches!(ty, Ty::TypeVar(param, _) if param == self_param) {
        return false;
    }
    mentions_bare_self(ty, self_param)
}
