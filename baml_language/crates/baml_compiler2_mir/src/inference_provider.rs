//! MIR's view of inference: the `hir_ty` [`InferenceResult`] of the body
//! being lowered and, for a function, of its parameter defaults (their own
//! inference root), borrowed from the tracked queries. MIR reads `hir_ty`'s
//! vocabulary directly — [`MemberResolution`], [`CallPlan`],
//! [`ScopedTypeBinding`], … — through the `tir_*` accessors on
//! `LoweringContext`; nothing is re-encoded at the seam. Keying is shared
//! by construction: `hir_ty` types lambdas in their owner's arena and
//! parameter defaults as their own body owner, so the per-scope dispatch
//! reduces to body-vs-defaults.

use baml_compiler2_hir::{
    body::BodyOwnerId,
    loc::{FunctionLoc, LetLoc},
    semantic_index::ExprMetadataScope,
};
use baml_compiler2_hir_ty::infer::infer_body;
pub(crate) use baml_compiler2_hir_ty::infer::{
    CallPlan, InferenceResult, MemberResolution, MethodCallee, ParamBinding, Receiver,
    ResolvedPath, ScopedTypeBinding, ScopedTypeSource,
};

/// The inference results behind the `tir_*` accessors, one per metadata
/// scope.
pub(crate) struct InferenceTables<'db> {
    body: &'db InferenceResult<'db>,
    /// A function's parameter defaults; a `let` has no parameters.
    defaults: Option<&'db InferenceResult<'db>>,
}

impl<'db> InferenceTables<'db> {
    pub(crate) fn for_function(db: &'db dyn crate::Db, function: FunctionLoc<'db>) -> Self {
        InferenceTables {
            body: infer_body(db, BodyOwnerId::Function(function)),
            defaults: Some(infer_body(db, BodyOwnerId::ParameterDefaults(function))),
        }
    }

    pub(crate) fn for_let(db: &'db dyn crate::Db, let_binding: LetLoc<'db>) -> Self {
        InferenceTables {
            body: infer_body(db, BodyOwnerId::Let(let_binding)),
            defaults: None,
        }
    }

    /// The results `scope` reads: a body's own, or its parameter defaults'.
    pub(crate) fn for_scope(&self, scope: ExprMetadataScope) -> Option<&'db InferenceResult<'db>> {
        match scope {
            ExprMetadataScope::Body(_) => Some(self.body),
            ExprMetadataScope::ParameterDefault(_) => self.defaults,
        }
    }
}
