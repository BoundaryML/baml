//! Cross-function throws propagation (S12): `callable_throws` is the
//! on-demand salsa query the design settles on - declared `throws` wins;
//! an omitted clause runs the callee's body inference and takes its
//! effect channel. Mutual recursion iterates to fixpoint from `never`
//! (`cycle_initial`), replacing TIR's eager package-wide pre-pass. The
//! crate's first tracked query - the S3 incremental work generalizes the
//! pattern to `infer_body` itself.

use baml_compiler2_hir::loc::FunctionLoc;

/// The stable, source-location-free identity of a callable exported across a
/// package boundary.  These names are exactly the material MIR needs to build
/// its linked item reference; consumers must never fabricate a `FunctionLoc`
/// for a source-less package.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum ExternalCallTarget {
    Free {
        package: baml_base::Name,
        namespace: Vec<baml_base::Name>,
        name: baml_base::Name,
    },
    Method {
        package: baml_base::Name,
        namespace: Vec<baml_base::Name>,
        class: baml_base::Name,
        name: baml_base::Name,
    },
    Interface {
        interface: baml_type::QualifiedTypeName,
        method: baml_base::Name,
    },
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
            ExternalCallTarget::Free { name, .. } | ExternalCallTarget::Method { name, .. } => name,
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
    if let Some(seeds) = db.seeded_callable_throws() {
        let by_path = seeds.by_path(db);
        if !by_path.is_empty() {
            let path = function.file(db).path(db).display().to_string();
            if let Some(ty) = by_path
                .get(&path)
                .and_then(|by_id| by_id.get(&function.id(db).as_u32()))
            {
                return CallableThrows(ty.clone());
            }
        }
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
    #[expect(
        deprecated,
        reason = "pre-D3: materializes interned inference state; moves to the finalized facts layer with the cutover"
    )]
    let throws = result.throws.to_plain();
    CallableThrows(throws)
}

/// Declaration-site resolved signature of a function or method, as the
/// tooling surface consumes it: plain types, OWN generic parameters only,
/// the declared throws kept separate from the inferred effect.
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
    /// The written `throws` clause; `None` when omitted (the effective
    /// contract is then [`callable_throws`]' inferred one).
    pub declared_throws: Option<baml_type::Ty>,
    /// The function's OWN generic parameters: its frame minus the enclosing
    /// type's prefix (class frame, interface frame incl. `Self` and the
    /// associated slots, or free-impl frame).
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
        Some(MethodOwner::FreeImpl(imp)) => crate::lower::impl_frame(db, imp).len(),
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
    #[expect(
        deprecated,
        reason = "pre-D3: materializes interned inference state; moves to the finalized facts layer with the cutover"
    )]
    let params = sig
        .params
        .iter()
        .map(|param| baml_type::FunctionParamTy {
            name: Some(param.name.clone()),
            ty: param.ty.to_plain(),
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
    #[expect(
        deprecated,
        reason = "pre-D3: materializes interned inference state; moves to the finalized facts layer with the cutover"
    )]
    let return_type = sig.ret.to_plain();
    #[expect(
        deprecated,
        reason = "pre-D3: materializes interned inference state; moves to the finalized facts layer with the cutover"
    )]
    let declared_throws = sig.throws_declared.then(|| sig.throws.to_plain());
    FunctionSignatureTy {
        params,
        return_type,
        declared_throws,
        generic_params: sig.generic_params[enclosing.min(sig.generic_params.len())..].to_vec(),
        builtin_kind,
    }
}
