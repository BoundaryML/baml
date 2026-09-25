//! Cross-function throws propagation (S12): `callable_throws` is the
//! on-demand salsa query the design settles on - declared `throws` wins;
//! an omitted clause runs the callee's body inference and takes its
//! effect channel. Mutual recursion iterates to fixpoint from `never`
//! (`cycle_initial`), replacing TIR's eager package-wide pre-pass. The
//! crate's first tracked query - the S3 incremental work generalizes the
//! pattern to `infer_body` itself.

use std::borrow::Cow;

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
    _db: &'db dyn baml_compiler2_hir::Db,
    _id: salsa::Id,
    _function: FunctionLoc<'db>,
) -> CallableThrows {
    // The fixpoint seed: a recursive call contributes nothing until an
    // iteration proves otherwise.
    CallableThrows(baml_type::Ty::Never)
}

/// What `function` throws: the declared clause when written, else the
/// union its body's effect channel infers.
#[salsa::tracked(cycle_initial = callable_throws_cycle_initial)]
pub fn callable_throws<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
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
    let data = baml_compiler2_hir::item_data::elaborated_function_data(db, function);
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
    db: &'db dyn baml_compiler2_hir::Db,
    function: FunctionLoc<'db>,
) -> usize {
    use baml_compiler2_hir::item_data::MethodOwner;
    match baml_compiler2_hir::item_data::method_owner(db, function) {
        Some(MethodOwner::Class(class)) => crate::lower::class_generic_frame(db, class).len(),
        Some(MethodOwner::Interface(iface)) => crate::lower::interface_frame(db, iface).len(),
        Some(MethodOwner::Impl(imp)) => crate::lower::impl_frame(db, imp).len(),
        None => 0,
    }
}

fn function_signature_ty_cycle_initial<'db>(
    _db: &'db dyn baml_compiler2_hir::Db,
    _id: salsa::Id,
    _function: FunctionLoc<'db>,
) -> FunctionSignatureTy {
    // A projection of `function_signature`, so it sits inside the same
    // signature/throws/inference fixpoint (an omitted throws clause runs
    // body inference, which resolves callees through this view): the same
    // degenerate seed as `function_signature_cycle_initial`; iteration
    // converges with the signature it projects.
    FunctionSignatureTy {
        params: Vec::new(),
        return_type: baml_type::Ty::error(),
        generic_params: Vec::new(),
        builtin_kind: None,
    }
}

#[salsa::tracked(returns(ref), cycle_initial = function_signature_ty_cycle_initial)]
pub fn function_signature_ty<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
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
    let builtin_kind = match baml_compiler2_hir::body::function_body(db, function).as_ref() {
        baml_compiler2_hir::body::FunctionBody::Builtin(kind) => Some(*kind),
        baml_compiler2_hir::body::FunctionBody::Expr(_)
        | baml_compiler2_hir::body::FunctionBody::Missing => None,
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
    db: &'db dyn baml_compiler2_hir::Db,
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
    db: &'db dyn baml_compiler2_hir::Db,
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
/// test, in both lanes. This is the SUGAR question only — whether
/// `a.m(..)` may stand for `I.m(a, ..)`; a static is not a member of an
/// instance. Whether an erased `Self` can be dispatched is
/// [`callable_self_dispatch`], which treats the receiver as the parameter
/// it is.
pub fn callable_takes_self(db: &dyn baml_compiler2_hir::Db, callable: FunctionRef<'_>) -> bool {
    callable_signature(db, callable)
        .params
        .first()
        .and_then(|param| param.name.as_ref())
        .is_some_and(|name| name.as_str() == "self")
}

/// `Some` for a builtin-bodied callable.
pub fn callable_builtin_kind(
    db: &dyn baml_compiler2_hir::Db,
    callable: FunctionRef<'_>,
) -> Option<baml_compiler2_ast::BuiltinKind> {
    callable_signature(db, callable).builtin_kind
}

/// The function a language package declares at `namespace.name`, whichever
/// lane serves the package: the declaration when the package's source is
/// present, its exported row when it is served from its interface. `None`
/// when the package is not installed or declares no such function.
pub fn lang_function<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    package: baml_base::LangPackage,
    namespace: &[&str],
    name: &str,
) -> Option<FunctionRef<'db>> {
    use baml_compiler2_hir::contributions::Definition;

    let root = baml_compiler2_hir::package::lang_roots(db).get(package)?;
    let namespace: Vec<Name> = namespace.iter().copied().map(Name::new).collect();
    let name = Name::new(name);
    if baml_compiler2_hir::package::is_served_from_interface(db, root) {
        return crate::extern_loc::extern_function_named(db, root, &namespace, &name)
            .map(DeclRef::External);
    }
    match baml_compiler2_hir::package::package_items(db, root).lookup_value(&namespace, &name)? {
        Definition::Function(function) => Some(DeclRef::Source(function)),
        Definition::Class(_)
        | Definition::Enum(_)
        | Definition::Interface(_)
        | Definition::TypeAlias(_)
        | Definition::Let(_) => None,
    }
}

/// The class-inherent method `class.method` a language package declares at
/// its root, whichever lane serves the package (see [`lang_function`]).
pub fn lang_class_method<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    package: baml_base::LangPackage,
    class: &str,
    method: &str,
) -> Option<FunctionRef<'db>> {
    let root = baml_compiler2_hir::package::lang_roots(db).get(package)?;
    let method = Name::new(method);
    if baml_compiler2_hir::package::is_served_from_interface(db, root) {
        let head = baml_type::DeclName::in_root(root, Vec::new(), Name::new(class));
        return crate::extern_loc::extern_class_method(db, &head, &method).map(DeclRef::External);
    }
    let baml_compiler2_hir::contributions::Definition::Class(class) =
        baml_compiler2_hir::package::package_items(db, root).lookup_type(&[], &Name::new(class))?
    else {
        return None;
    };
    baml_compiler2_hir::item_data::class_data(db, class)
        .methods
        .iter()
        .copied()
        .find(|&candidate| {
            baml_compiler2_hir::item_data::function_data(db, candidate).name == method
        })
        .map(DeclRef::Source)
}

/// The callable's own short name, for diagnostics and display.
pub fn callable_display_name<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    callable: FunctionRef<'db>,
) -> &'db Name {
    match callable {
        DeclRef::Source(function) => {
            &baml_compiler2_hir::item_data::function_data(db, function).name
        }
        DeclRef::External(function) => &extern_function_row(db, function).name,
    }
}

/// Call-site-suppliable generics with their bounds: the callable's OWN
/// parameters minus the synthetic callback-effect parameters, which are
/// inference-only and never participate in written arity.
pub fn callable_user_generic_params(
    db: &dyn baml_compiler2_hir::Db,
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
/// parameters, with the declared bound conjunction of every slot. Borrowed
/// from the declaration's own storage whenever one side of the frame is
/// empty and the other is held whole there; built only for a concatenation.
#[derive(Debug, Clone, PartialEq)]
pub struct CallableFrame<'db> {
    pub params: Cow<'db, [ParamTy]>,
    /// Per frame slot, that parameter's declared bounds (empty when none).
    pub bounds: Cow<'db, [Vec<baml_type::Interface>]>,
    /// The first slot the CALL SITE supplies (turbofish or inferred). Slots
    /// before it belong to the receiver: `Self` legitimately binds an
    /// existential for virtual dispatch, and class/interface args were
    /// judged at the receiver's own annotation.
    pub own_start: usize,
}

/// The frame a call instantiates.
///
/// `own_start` is the OWNER frame's length in both lanes: everything after
/// it is the callable's own — its declared parameters, then the synthetic
/// effect parameters elaborated onto the signature — which is also exactly
/// what an exported row lists as its `generic_params`.
pub fn callable_generic_frame<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    callable: FunctionRef<'db>,
) -> CallableFrame<'db> {
    match callable {
        DeclRef::Source(function) => {
            let params = crate::lower::function_generic_frame(db, function);
            let bounds = crate::package_interface::plain_bounds(
                &params,
                &crate::lower::function_generic_bounds(db, function),
            );
            // The frame is `owner ++ declared ++ synthetic effect`, so the
            // owner's length is what the callable's own two groups leave.
            let own = baml_compiler2_hir::item_data::elaborated_function_data(db, function);
            let own = own.user_generic_params.len() + own.synthetic_effect_params.len();
            debug_assert!(own <= params.len());
            CallableFrame {
                own_start: params.len() - own,
                params: Cow::Owned(params),
                bounds: Cow::Owned(bounds),
            }
        }
        DeclRef::External(function) => {
            let (owner_params, owner_bounds) = extern_owner_generics(db, function);
            let row = extern_function_row(db, function);
            CallableFrame {
                own_start: owner_params.len(),
                params: concat_frame(owner_params, &row.generic_params),
                bounds: concat_frame(owner_bounds, &row.generic_param_bounds),
            }
        }
    }
}

/// `owner ++ own`, borrowing whichever side is the whole frame; a copy is
/// made only when both contribute.
fn concat_frame<'db, T: Clone>(owner: Cow<'db, [T]>, own: &'db [T]) -> Cow<'db, [T]> {
    match (owner.is_empty(), own.is_empty()) {
        (_, true) => owner,
        (true, false) => Cow::Borrowed(own),
        (false, false) => {
            let mut frame = owner.into_owned();
            frame.extend(own.iter().cloned());
            Cow::Owned(frame)
        }
    }
}

/// The callable as a value of function type, instantiated at `instantiation`
/// (one type per frame slot): parameters, return type, and effective throws
/// with every frame variable substituted.
pub fn instantiate_callable_signature<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
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
    })
}

/// Where an erased `Self` (an interface-existential or a union) is
/// dispatched from — the one-`Self` rule (spec, `Self`: "exactly one
/// `Self`-typed parameter (including the `self` receiver)"), with the
/// receiver treated as what it is: a parameter named `self` whose type is
/// `Self`. A method is object-safe when exactly one parameter is typed
/// bare `Self` and `Self` occurs nowhere else — not in a second parameter,
/// not nested in any parameter, not nested inside an invariant constructor
/// in the return/throws type (a bare top-level `-> Self` collapses
/// covariantly and stays legal). `Self.Assoc` projections are exempt: the
/// existential's pins make them one concrete type.
///
/// A union is NOT a relaxation in argument position: `other: Self?` and
/// `other: Self | int` each name the implementor, so a caller holding two
/// existentials could pass a second, DIFFERENT concrete type into a callee
/// compiled for one. Covariance makes a top-level `Self` harmless only
/// where the value flows OUT (the return and throws types), which is why
/// those two are the only positions tested permissively.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfDispatch {
    /// Object-safe: the one bare-`Self` parameter is at this index, and an
    /// erased `Self` is dispatched from that argument's runtime type.
    OnParam(usize),
    /// No parameter is typed `Self` (and `Self` occurs nowhere the rule
    /// forbids): there is no value to consult, so a caller must name a
    /// concrete `Self`.
    NoSelfParam,
    /// `Self` occurs where the rule forbids it: an erased `Self` names no
    /// single implementation for the call.
    Breaks(crate::diagnostics::SelfCallPosition),
}

/// [`SelfDispatch`] for a callable, in either lane. `Self` is frame slot 0
/// in both — the export asserts it and the import refuses a row that frames
/// it otherwise — so one test serves both.
pub fn callable_self_dispatch(
    db: &dyn baml_compiler2_hir::Db,
    callable: FunctionRef<'_>,
) -> SelfDispatch {
    use crate::diagnostics::SelfCallPosition;
    let signature = callable_signature(db, callable);
    // `top_ok` is the covariance switch: with it set, a bare top-level
    // `Self` is permitted and only NESTED occurrences are flagged. Only
    // the dispatch parameter and the outward-flowing types earn it.
    let self_occurs = |ty: &baml_type::Ty, top_ok: bool| {
        crate::method_resolution::self_occurs(&baml_type::interned::Ty::from_plain(ty), top_ok)
    };
    let bare_self = |ty: &baml_type::Ty| {
        matches!(
            baml_type::interned::Ty::from_plain(ty).kind(),
            baml_type::interned::InferTy::TypeVar(param)
                if param.index() == 0 && param.as_str() == "Self"
        )
    };
    let mut dispatch_params = signature
        .params
        .iter()
        .enumerate()
        .filter(|(_, param)| bare_self(&param.ty))
        .map(|(index, _)| index);
    let first = dispatch_params.next();
    let second = dispatch_params.next();
    // Every parameter but the one dispatch source must be free of `Self`
    // ENTIRELY, unions included — one runtime value answers the question
    // "which implementor?", and a second mention is a second answer.
    let other_param_mentions_self = signature
        .params
        .iter()
        .enumerate()
        .any(|(index, param)| Some(index) != first && self_occurs(&param.ty, false));
    if second.is_some() || other_param_mentions_self {
        return SelfDispatch::Breaks(SelfCallPosition::Parameter);
    }
    if self_occurs(&signature.return_type, true)
        || self_occurs(callable_throws_of(db, callable), true)
    {
        return SelfDispatch::Breaks(SelfCallPosition::NestedInReturn);
    }
    first.map_or(SelfDispatch::NoSelfParam, SelfDispatch::OnParam)
}

/// Whether the one-`Self` rule forbids dispatching this callable on an
/// erased `Self` — [`SelfDispatch::Breaks`]. A method with no `Self`
/// parameter does not break the rule; it merely has nothing to dispatch on.
pub fn callable_breaks_one_self(
    db: &dyn baml_compiler2_hir::Db,
    callable: FunctionRef<'_>,
) -> bool {
    matches!(
        callable_self_dispatch(db, callable),
        SelfDispatch::Breaks(_)
    )
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
    db: &dyn baml_compiler2_hir::Db,
    callable: FunctionRef<'_>,
) -> CallableOwnerKind {
    use baml_compiler2_hir::item_data::MethodOwner;
    match callable {
        DeclRef::Source(function) => {
            match baml_compiler2_hir::item_data::method_owner(db, function) {
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
    db: &dyn baml_compiler2_hir::Db,
    callable: FunctionRef<'_>,
) -> Option<DeclName> {
    use baml_compiler2_hir::item_data::MethodOwner;
    match callable {
        DeclRef::Source(function) => {
            match baml_compiler2_hir::item_data::method_owner(db, function)? {
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

// Every `ExternFunctionLoc` is a `FunctionRef::External`; the conversion
// exists so a consumer holding a row identity can ask the shared surface
// without naming the variant.
impl<'db> From<ExternFunctionLoc<'db>> for FunctionRef<'db> {
    fn from(function: ExternFunctionLoc<'db>) -> Self {
        DeclRef::External(function)
    }
}
