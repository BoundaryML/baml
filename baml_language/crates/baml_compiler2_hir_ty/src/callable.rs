//! Cross-function throws propagation (S12): `callable_throws` is the
//! on-demand salsa query the design settles on - declared `throws` wins;
//! an omitted clause runs the callee's body inference and takes its
//! effect channel. Mutual recursion iterates to fixpoint from `never`
//! (`cycle_initial`), replacing TIR's eager package-wide pre-pass. The
//! crate's first tracked query - the S3 incremental work generalizes the
//! pattern to `infer_body` itself.

use baml_compiler2_hir::loc::FunctionLoc;

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
            return CallableThrows(crate::lower::reject_holes(&lowered).to_plain());
        }
    }
    let result = crate::infer::infer_body(
        db,
        baml_compiler2_hir::body::BodyOwnerId::Function(function),
    );
    CallableThrows(result.throws.to_plain())
}
