//! MIR borrows the body's and parameter defaults' `hir_ty` inference results
//! directly, one per arena.

use baml_compiler2_hir::{
    body::BodyOwnerId,
    loc::{FunctionLoc, LetLoc},
};
use baml_compiler2_hir_ty::infer::infer_body;
pub(crate) use baml_compiler2_hir_ty::infer::{
    CallPlan, InferenceResult, MemberResolution, MethodCallee, ParamBinding, Receiver,
    ResolvedPath, ScopedTypeBinding, ScopedTypeSource,
};

/// The inference results behind the `tir_*` accessors, one per metadata scope.
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

    /// The results `root` was typed in.
    pub(crate) fn of(&self, root: InferenceRoot) -> Option<&'db InferenceResult<'db>> {
        match root {
            InferenceRoot::Body => Some(self.body),
            InferenceRoot::ParameterDefaults => self.defaults,
        }
    }
}

/// Which of an owner's two arenas is being lowered, and so which inference
/// result types its expressions. The metadata scope cannot say: a lambda
/// keeps a scope of its own wherever it is written, so one inside a
/// parameter default is keyed like any other lambda and typed with the
/// defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InferenceRoot {
    Body,
    ParameterDefaults,
}
