//! Cross-function throws propagation (S12): `callable_throws` is the
//! on-demand salsa query the design settles on - declared `throws` wins;
//! an omitted clause runs the callee's body inference and takes its
//! effect channel. Mutual recursion iterates to fixpoint from `never`
//! (`cycle_initial`), replacing TIR's eager package-wide pre-pass. The
//! crate's first tracked query - the S3 incremental work generalizes the
//! pattern to `infer_body` itself.

use baml_base::Name;
use baml_compiler2_hir::loc::{DeclRef, FunctionLoc};
use baml_type::{DeclName, ParamTy};

use crate::extern_loc::{
    ExternFunctionLoc, ExternRowAddr, FunctionRef, extern_function_row, extern_owner_generics,
    extern_signature_ty,
};

/// The stable, source-location-free identity of a callable exported across a
/// package boundary.  These names are exactly the material MIR needs to build
/// its linked item reference; consumers must never fabricate a `FunctionLoc`
/// for a source-less package.
#[derive(Debug, Clone, PartialEq, Eq, Hash, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum ExternalCallTarget<N: baml_type::Head = baml_type::DeclName> {
    /// A free function, named as an item: its package (the head's root at
    /// compile time, a spelling on the wire), namespace, and name.
    Free { function: N },
    /// A class-inherent method: the owning class as a head, and the method.
    Method { class: N, name: baml_base::Name },
    /// An interface method: the interface as a head, and the method.
    Interface {
        interface: N,
        method: baml_base::Name,
    },
}

impl<N: baml_type::Head> ExternalCallTarget<N> {
    /// This target with every head replaced by what `f` resolves it to: the
    /// one operation that moves a target between the database's root-headed
    /// form and the wire's name-headed form.
    pub fn try_map_heads<M: baml_type::Head, E>(
        &self,
        f: &mut impl FnMut(&N) -> Result<M, E>,
    ) -> Result<ExternalCallTarget<M>, E> {
        Ok(match self {
            Self::Free { function } => ExternalCallTarget::Free {
                function: f(function)?,
            },
            Self::Method { class, name } => ExternalCallTarget::Method {
                class: f(class)?,
                name: name.clone(),
            },
            Self::Interface { interface, method } => ExternalCallTarget::Interface {
                interface: f(interface)?,
                method: method.clone(),
            },
        })
    }
}

/// Whether an exported callable has a symbol that a source-less consumer may
/// link. Builtin bodies currently require source-owned lowering and therefore
/// remain deliberately reserved until a builtin supplies an explicit ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum ExternalLinkability {
    Linkable,
    ReservedBuiltin,
}

/// Owned call-site facts for a source-less dependency callable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalCallable {
    pub target: ExternalCallTarget,
    pub linkability: ExternalLinkability,
    /// Preserved so source-less consumers can lower compiler intrinsics and
    /// await/sys-op call shapes through the same road as source functions.
    pub builtin_kind: Option<baml_compiler2_ast::BuiltinKind>,
    pub takes_self: bool,
    pub owner_generic_params: Vec<baml_type::ParamTy>,
    pub owner_generic_param_bounds: Vec<Vec<baml_type::Interface>>,
    pub generic_params: Vec<baml_type::ParamTy>,
    pub generic_param_bounds: Vec<Vec<baml_type::Interface>>,
}

impl ExternalCallable {
    /// Call-site-suppliable generics. Synthetic callback-effect parameters are
    /// inference-only and never participate in written arity.
    pub fn user_generic_params(
        &self,
    ) -> impl Iterator<Item = (&baml_type::ParamTy, &[baml_type::Interface])> {
        self.generic_params
            .iter()
            .zip(self.generic_param_bounds.iter())
            .filter(|(param, _)| !baml_type::is_synthetic_effect_param(param.name()))
            .map(|(param, bounds)| (param, bounds.as_slice()))
    }

    pub fn display_name(&self) -> &baml_base::Name {
        match &self.target {
            ExternalCallTarget::Free { function } => function.name(),
            ExternalCallTarget::Method { name, .. } => name,
            ExternalCallTarget::Interface { method, .. } => method,
        }
    }
}

/// A function's effect: plain (ground - inference never leaks variables,
/// finalize defaults unconstrained effects to `never`). Wrapped for the
/// manual `salsa::Update` impl (`baml_type` has no salsa dependency).
#[derive(Debug, Clone, PartialEq)]
pub struct CallableThrows(pub baml_type::Ty);

// SAFETY: `maybe_update` transfers ownership of `new_value` into
// `old_pointer` and reports change via `PartialEq` for early cutoff -
// the `ResolvedTypeAlias`/`ScopeInference` precedent.
#[allow(unsafe_code)]
unsafe impl salsa::Update for CallableThrows {
    #[allow(unsafe_code)]
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        // SAFETY: `old_pointer` is valid and initialized, per the trait
        // contract.
        #[allow(unsafe_code)]
        unsafe {
            let changed = *old_pointer != new_value;
            if changed {
                std::ptr::drop_in_place(old_pointer);
                std::ptr::write(old_pointer, new_value);
            }
            changed
        }
    }
}

fn callable_throws_cycle_initial<'db>(
    _db: &'db dyn baml_compiler2_ppir::Db,
    _id: salsa::Id,
    _function: FunctionLoc<'db>,
) -> CallableThrows {
    // The fixpoint seed: a recursive call contributes nothing until an
    // iteration proves otherwise.
    CallableThrows(baml_type::Ty::Never {
        attr: baml_type::TyAttr::default(),
    })
}

/// What `function` throws: the declared clause when written, else the
/// union its body's effect channel infers.
#[salsa::tracked(cycle_initial = callable_throws_cycle_initial)]
pub fn callable_throws<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    function: FunctionLoc<'db>,
) -> CallableThrows {
    // A seeded value from a previous compile short-circuits body inference
    // for a clean function (the bytecode cache seeds only functions its
    // reuse plan proved unchanged). `by_path(db)` is a tracked read of the
    // `SeededCallableThrows` input, so a later seed invalidates this memo;
    // the lookup is skipped when no seeds were injected (LSP, cold CLI).
    // Seeds are wire data: their heads are spelled, and resolve through the
    // seeded function's own root. A seed naming a package that root cannot
    // reach is not this compile's fact and is inferred honestly below.
    if let Some(seeds) = db.seeded_callable_throws() {
        let by_path = seeds.by_path(db);
        if !by_path.is_empty() {
            let file = function.file(db);
            let path = file.path(db).display().to_string();
            if let Some(ty) = by_path
                .get(&path)
                .and_then(|by_id| by_id.get(&function.id(db).as_u32()))
            {
                let root = baml_compiler2_hir::file_package::file_package(db, file).root;
                let spelling = baml_compiler2_hir::package::spelling(db);
                if let Ok(ty) = ty.try_map_heads::<_, (), _>(&mut |name| {
                    spelling.resolve(db, root, name).ok_or(())
                }) {
                    return CallableThrows(ty);
                }
            }
        }
    }
    // An interface method's contract is what its signature declares, never
    // what a default body does: that body is an implementation kept alongside
    // the interface, and no implementor is bound by it. Deferring to the one
    // signature road also keeps this out of the fixpoint - `function_signature`
    // does not call back here for an interface contract (same predicate).
    if crate::lower::signature_is_interface_contract(db, function) {
        return CallableThrows(
            crate::lower::function_signature(db, function)
                .throws
                .clone(),
        );
    }
    let data = baml_compiler2_ppir::item_data::elaborated_function_data(db, function);
    if let Some(throws_ref) = data.throws {
        let frame = crate::lower::function_generic_frame(db, function);
        let ctx = crate::lower::lower_ctx_for_file(db, function.file(db)).with_frame(frame);
        let lowered = ctx.lower_type_ref(&data.type_refs, throws_ref);
        // A PARTIAL clause (`throws T | _`, spec Functions rule 3) keeps
        // inferring: fall through to the body run, whose finalize unions
        // the named members with the inferred set.
        if !crate::lower::throws_clause_parts(&lowered).1 {
            return CallableThrows(crate::lower::reject_holes(&lowered));
        }
    }
    let result = crate::infer::infer_body(
        db,
        baml_compiler2_hir::body::BodyOwnerId::Function(function),
    );
    CallableThrows(result.throws.clone())
}

/// Declaration-site resolved signature of a function or method, as the
/// tooling surface consumes it: plain types, OWN generic parameters only
/// (the effective error type is [`callable_throws`]' - total, whatever its
/// provenance).
///
/// A view over [`crate::lower::function_signature`] (the one signature
/// road), so `Self`, projections, and bounds resolve exactly as the type
/// provider sees them - for class methods, interface default methods, and
/// free-impl methods alike (`owner_self_ty` / `owner_impl_target` bind
/// `Self` in every owner position).
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionSignatureTy {
    /// Every parameter, `self` included; a defaulted parameter is
    /// [`baml_type::FunctionParamMode::Optional`].
    pub params: Vec<baml_type::FunctionParamTy>,
    /// The declared return type; `Ty::Error` when unwritten.
    pub return_type: baml_type::Ty,
    /// The function's OWN generic parameters: its frame minus the enclosing
    /// type's prefix (class frame, `[Self, iface params..]` for an
    /// interface, or the impl block's frame).
    pub generic_params: Vec<baml_type::ParamTy>,
    /// `Some` for builtin-bodied functions.
    pub builtin_kind: Option<baml_compiler2_ast::BuiltinKind>,
}

// SAFETY: `maybe_update` transfers ownership of `new_value` into
// `old_pointer` and reports change via `PartialEq` for early cutoff -
// the `CallableThrows` precedent.
#[allow(unsafe_code)]
unsafe impl salsa::Update for FunctionSignatureTy {
    #[allow(unsafe_code)]
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        // SAFETY: `old_pointer` is valid and initialized, per the trait
        // contract.
        #[allow(unsafe_code)]
        unsafe {
            let changed = *old_pointer != new_value;
            if changed {
                std::ptr::drop_in_place(old_pointer);
                std::ptr::write(old_pointer, new_value);
            }
            changed
        }
    }
}

/// The enclosing type's generic-frame prefix length for a method's frame;
/// 0 for a free function.
fn enclosing_param_count<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    function: FunctionLoc<'db>,
) -> usize {
    use baml_compiler2_ppir::item_data::MethodOwner;
    match baml_compiler2_ppir::item_data::method_owner(db, function) {
        Some(MethodOwner::Class(class)) => crate::lower::class_generic_frame(db, class).len(),
        Some(MethodOwner::Interface(iface)) => crate::lower::interface_frame(db, iface).len(),
        Some(MethodOwner::Impl(imp)) => crate::lower::impl_frame(db, imp).len(),
        None => 0,
    }
}

#[salsa::tracked(returns(ref))]
pub fn function_signature_ty<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    function: FunctionLoc<'db>,
) -> FunctionSignatureTy {
    let sig = crate::lower::function_signature(db, function);
    let enclosing = enclosing_param_count(db, function);
    let params = sig
        .params
        .iter()
        .map(|param| baml_type::FunctionParamTy {
            name: Some(param.name.clone()),
            ty: param.ty.clone(),
            mode: if param.has_default {
                baml_type::FunctionParamMode::Optional
            } else {
                baml_type::FunctionParamMode::Required
            },
        })
        .collect();
    let builtin_kind = match baml_compiler2_ppir::function_body(db, function).as_ref() {
        baml_compiler2_hir::body::FunctionBody::Builtin(kind) => Some(*kind),
        _ => None,
    };
    let return_type = sig.ret.clone();
    FunctionSignatureTy {
        params,
        return_type,
        generic_params: sig.generic_params[enclosing.min(sig.generic_params.len())..].to_vec(),
        builtin_kind,
    }
}

// ── The one callable surface over provenance ─────────────────────────────────
//
// Every question a consumer asks of a callable is answered here for BOTH
// lanes of [`FunctionRef`] by one function with one return type — a source
// item through its salsa queries, an exported row through
// [`crate::extern_loc`]'s — so no consumer keeps a twin helper per lane.

/// The declaration-site resolved signature: own generics, every parameter
/// (`self` included), the declared return type.
pub fn callable_signature<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    callable: FunctionRef<'db>,
) -> &'db FunctionSignatureTy {
    match callable {
        DeclRef::Source(function) => function_signature_ty(db, function),
        DeclRef::External(function) => extern_signature_ty(db, function),
    }
}

/// The effective error type: the signature's total `throws` for a source
/// item (written when closed, body-inferred when open — the same value
/// `function_signature` pairs with the parameters), the exported
/// `callable_throws` for a row.
pub fn callable_throws_of<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    callable: FunctionRef<'db>,
) -> &'db baml_type::Ty {
    match callable {
        DeclRef::Source(function) => &crate::lower::function_signature(db, function).throws,
        DeclRef::External(function) => &extern_function_row(db, function).callable_throws,
    }
}

/// Whether the callable declares a `self` receiver — an instance method
/// rather than a static one. The receiver is an ordinary first parameter
/// named `self` (there is no separate receiver slot), so this is the whole
/// test, in both lanes.
pub fn callable_takes_self(db: &dyn baml_compiler2_ppir::Db, callable: FunctionRef<'_>) -> bool {
    callable_signature(db, callable)
        .params
        .first()
        .and_then(|param| param.name.as_ref())
        .is_some_and(|name| name.as_str() == "self")
}

/// `Some` for a builtin-bodied callable.
pub fn callable_builtin_kind(
    db: &dyn baml_compiler2_ppir::Db,
    callable: FunctionRef<'_>,
) -> Option<baml_compiler2_ast::BuiltinKind> {
    callable_signature(db, callable).builtin_kind
}

/// The callable's own short name, for diagnostics.
pub fn callable_display_name<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    callable: FunctionRef<'db>,
) -> &'db Name {
    match callable {
        DeclRef::Source(function) => {
            &baml_compiler2_ppir::item_data::function_data(db, function).name
        }
        DeclRef::External(function) => &extern_function_row(db, function).name,
    }
}

/// Call-site-suppliable generics with their bounds: the callable's OWN
/// parameters minus the synthetic callback-effect parameters, which are
/// inference-only and never participate in written arity.
pub fn callable_user_generic_params(
    db: &dyn baml_compiler2_ppir::Db,
    callable: FunctionRef<'_>,
) -> Vec<(ParamTy, Vec<baml_type::Interface>)> {
    let own: Vec<(ParamTy, Vec<baml_type::Interface>)> = match callable {
        DeclRef::Source(function) => {
            let bounds = crate::lower::function_generic_bounds(db, function);
            function_signature_ty(db, function)
                .generic_params
                .iter()
                .map(|param| {
                    (
                        param.clone(),
                        bounds.get(param).cloned().unwrap_or_default(),
                    )
                })
                .collect()
        }
        DeclRef::External(function) => {
            let row = extern_function_row(db, function);
            row.generic_params
                .iter()
                .cloned()
                .zip(row.generic_param_bounds.iter().cloned())
                .collect()
        }
    };
    own.into_iter()
        .filter(|(param, _)| !baml_type::is_synthetic_effect_param(param.name()))
        .collect()
}

/// A callable's full generic frame: the owner's prefix, then its own
/// parameters, with the declared bound conjunction of every slot.
#[derive(Debug, Clone, PartialEq)]
pub struct CallableFrame {
    pub params: Vec<ParamTy>,
    /// Per frame slot, that parameter's declared bounds (empty when none).
    pub bounds: Vec<Vec<baml_type::Interface>>,
    /// The first slot the CALL SITE supplies (turbofish or inferred). Slots
    /// before it belong to the receiver: `Self` legitimately binds an
    /// existential for virtual dispatch, and class/interface args were
    /// judged at the receiver's own annotation.
    pub own_start: usize,
}

/// The frame a call instantiates.
///
/// `own_start` is computed per lane exactly as the two bound-registration
/// roads did before they shared this surface, and the formulas DIVERGE: a
/// source item's own count is its DECLARED parameters (`function_data`), so
/// a synthetic effect parameter elaborated onto the signature counts as
/// part of the prefix; an exported row's own count is its whole `generic_params`
/// (effect parameters included), so `own_start` is the owner frame's length.
/// The difference only feeds the concreteness rule on effect slots, which
/// no bound constrains; preserved verbatim rather than unified here, so the
/// unification is its own reviewable change.
pub fn callable_generic_frame(
    db: &dyn baml_compiler2_ppir::Db,
    callable: FunctionRef<'_>,
) -> CallableFrame {
    match callable {
        DeclRef::Source(function) => {
            let params = crate::lower::function_generic_frame(db, function);
            let bounds = crate::package_interface::plain_bounds(
                &params,
                &crate::lower::function_generic_bounds(db, function),
            );
            let own = baml_compiler2_ppir::item_data::function_data(db, function)
                .generic_params
                .len();
            CallableFrame {
                own_start: params.len().saturating_sub(own),
                params,
                bounds,
            }
        }
        DeclRef::External(function) => {
            let (owner_params, owner_bounds) = extern_owner_generics(db, function);
            let row = extern_function_row(db, function);
            let mut params = owner_params.to_vec();
            params.extend(row.generic_params.iter().cloned());
            let mut bounds = owner_bounds.to_vec();
            bounds.extend(row.generic_param_bounds.iter().cloned());
            CallableFrame {
                params,
                bounds,
                own_start: owner_params.len(),
            }
        }
    }
}

/// The callable as a value of function type, instantiated at `instantiation`
/// (one type per frame slot): parameters, return type, and effective throws
/// with every frame variable substituted.
pub fn instantiate_callable_signature<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    callable: FunctionRef<'db>,
    instantiation: &[baml_type::interned::Ty],
) -> baml_type::interned::Ty {
    use baml_type::interned::{InferFunctionParamTy, InferTy, Ty};
    let signature = callable_signature(db, callable);
    let substitute =
        |ty: &baml_type::Ty| crate::lower::substitute_params(&Ty::from_plain(ty), instantiation);
    let params: Box<[InferFunctionParamTy]> = signature
        .params
        .iter()
        .map(|param| InferFunctionParamTy {
            name: param.name.clone(),
            ty: substitute(&param.ty),
            mode: param.mode,
        })
        .collect();
    Ty::intern(InferTy::Function {
        params,
        ret: substitute(&signature.return_type),
        throws: substitute(callable_throws_of(db, callable)),
        attr: baml_type::TyAttr::default(),
    })
}

/// The one-`Self` rule (spec: object safety for existential receivers): a
/// NON-self parameter containing bare `Self`, or `Self` nested inside an
/// invariant constructor in the return/throws, makes the method uncallable
/// through an existential (a bare top-level `-> Self` collapses covariantly
/// and stays legal). `Self.Assoc` projections are exempt — the
/// existential's pins make them one concrete type. `Self` is frame slot 0
/// in both lanes (the export asserts it), so one test serves both.
pub fn callable_breaks_one_self(
    db: &dyn baml_compiler2_ppir::Db,
    callable: FunctionRef<'_>,
) -> bool {
    let signature = callable_signature(db, callable);
    let self_in = |ty: &baml_type::Ty, top_ok: bool| {
        crate::method_resolution::self_occurs(&baml_type::interned::Ty::from_plain(ty), top_ok)
    };
    signature
        .params
        .iter()
        .skip(1)
        .any(|param| self_in(&param.ty, false))
        || self_in(&signature.return_type, true)
        || self_in(callable_throws_of(db, callable), true)
}

/// What declares a callable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallableOwnerKind {
    /// A top-level function.
    Free,
    /// A class-inherent method.
    Class,
    /// A method an interface declares (required or default).
    Interface,
    /// A method an `implements` block provides.
    Impl,
}

pub fn callable_owner_kind(
    db: &dyn baml_compiler2_ppir::Db,
    callable: FunctionRef<'_>,
) -> CallableOwnerKind {
    use baml_compiler2_ppir::item_data::MethodOwner;
    match callable {
        DeclRef::Source(function) => {
            match baml_compiler2_ppir::item_data::method_owner(db, function) {
                None => CallableOwnerKind::Free,
                Some(MethodOwner::Class(_)) => CallableOwnerKind::Class,
                Some(MethodOwner::Interface(_)) => CallableOwnerKind::Interface,
                Some(MethodOwner::Impl(_)) => CallableOwnerKind::Impl,
            }
        }
        DeclRef::External(function) => match function.addr(db) {
            ExternRowAddr::Declared(ExternalCallTarget::Free { .. }) => CallableOwnerKind::Free,
            ExternRowAddr::Declared(ExternalCallTarget::Method { .. }) => CallableOwnerKind::Class,
            ExternRowAddr::Declared(ExternalCallTarget::Interface { .. }) => {
                CallableOwnerKind::Interface
            }
            ExternRowAddr::ImplProvided { .. } => CallableOwnerKind::Impl,
        },
    }
}

/// The head of the declaration that owns a callable: the class for a class
/// method, the interface for an interface method, the IMPLEMENTED interface
/// for an impl-provided method; `None` for a free function (and for an
/// impl whose header did not resolve).
pub fn callable_owner_type(
    db: &dyn baml_compiler2_ppir::Db,
    callable: FunctionRef<'_>,
) -> Option<DeclName> {
    use baml_compiler2_ppir::item_data::MethodOwner;
    match callable {
        DeclRef::Source(function) => {
            match baml_compiler2_ppir::item_data::method_owner(db, function)? {
                MethodOwner::Class(class) => Some(crate::lower::class_qualified_name(db, class)),
                MethodOwner::Interface(interface) => {
                    Some(crate::lower::interface_qualified_name(db, interface))
                }
                MethodOwner::Impl(block) => Some(
                    crate::impls::impl_facts(db, block)
                        .resolved()?
                        .interface
                        .name
                        .clone(),
                ),
            }
        }
        DeclRef::External(function) => match function.addr(db) {
            ExternRowAddr::Declared(ExternalCallTarget::Free { .. }) => None,
            ExternRowAddr::Declared(ExternalCallTarget::Method { class, .. }) => {
                Some(class.clone())
            }
            ExternRowAddr::Declared(ExternalCallTarget::Interface { interface, .. }) => {
                Some(interface.clone())
            }
            ExternRowAddr::ImplProvided { identity, .. } => Some(identity.interface.clone()),
        },
    }
}

/// Every `ExternFunctionLoc` is a `FunctionRef::External`; the conversion
/// exists so a consumer holding a row identity can ask the shared surface
/// without naming the variant.
impl<'db> From<ExternFunctionLoc<'db>> for FunctionRef<'db> {
    fn from(function: ExternFunctionLoc<'db>) -> Self {
        DeclRef::External(function)
    }
}
