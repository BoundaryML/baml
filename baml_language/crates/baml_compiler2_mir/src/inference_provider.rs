//! MIR borrows the body's and parameter defaults' `hir_ty` inference results
//! directly, one per arena.

use baml_compiler2_hir::{
    body::BodyOwnerId,
    loc::{FunctionLoc, LetLoc},
};
use baml_compiler2_hir_ty::infer::infer_body;
pub(crate) use baml_compiler2_hir_ty::infer::{
    CallPlan, GlobalRoot, InferenceResult, MemberResolution, MethodCallee, ParamBinding, Receiver,
    ResolvedPath, ScopedTypeBinding,
};

/// The inference results behind the `tir_*` accessors, one per metadata scope.
pub(crate) struct InferenceTables<'db> {
    body: &'db InferenceResult<'db>,
    /// A function's parameter defaults; a `let` has no parameters.
    defaults: Option<&'db InferenceResult<'db>>,
}

impl<'db> InferenceTables<'db> {
    pub(crate) fn for_function(db: &'db dyn crate::Db, function: FunctionLoc<'db>) -> Self {
        let tables = InferenceTables {
            body: infer_body(db, BodyOwnerId::Function(function)),
            defaults: Some(infer_body(db, BodyOwnerId::ParameterDefaults(function))),
        };
        #[cfg(debug_assertions)]
        {
            let owner = || {
                format!(
                    "`{}` in {}",
                    baml_compiler2_hir::item_data::function_data(db, function).name,
                    function.file(db).path(db).display()
                )
            };
            assert_checked(tables.body, &owner);
            if let Some(defaults) = tables.defaults {
                assert_checked(defaults, &owner);
            }
            assert_signature_checked(db, function, &owner);
        }
        tables
    }

    pub(crate) fn for_let(db: &'db dyn crate::Db, let_binding: LetLoc<'db>) -> Self {
        let tables = InferenceTables {
            body: infer_body(db, BodyOwnerId::Let(let_binding)),
            defaults: None,
        };
        #[cfg(debug_assertions)]
        assert_checked(tables.body, &|| {
            format!(
                "`let {}` in {}",
                baml_compiler2_hir::item_data::let_data(db, let_binding).name,
                let_binding.file(db).path(db).display()
            )
        });
        tables
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

/// Code generation's probe of what it is about to read: lowering runs only
/// for a world that checked clean (`baml_db::CheckedWorld`), where no
/// body made an error diagnostic and none recorded the error sentinel
/// (`erase_infer`'s precondition - an `Error` is the trace of a report).
/// Probing what is lowered, when it is lowered, covers every road to emit
/// at the cost of the bodies emit types anyway. Debug builds only: a release
/// build refuses the sentinel at the runtime conversion boundary instead.
#[cfg(debug_assertions)]
fn assert_checked(result: &InferenceResult<'_>, owner: &dyn Fn() -> String) {
    assert!(
        result.diagnostics.iter().all(|diagnostic| {
            diagnostic.severity != baml_compiler2_hir_ty::diagnostics::DiagnosticSeverity::Error
        }),
        "code generation reached a body with an error diagnostic: {}",
        owner()
    );
    if let Some((site, ty)) = result.first_error_site() {
        panic!(
            "the error sentinel survived to code generation in {} at {site:?}: {ty:?}",
            owner()
        );
    }
}

/// [`assert_checked`] for a function's typed signature.
#[cfg(debug_assertions)]
fn assert_signature_checked(
    db: &dyn crate::Db,
    function: FunctionLoc<'_>,
    owner: &dyn Fn() -> String,
) {
    // The throws checked is the one emit consumes (`callable_throws`, which
    // for a closed clause IS the signature's).
    let signature = baml_compiler2_hir_ty::lower::function_signature(db, function);
    let throws = baml_compiler2_hir_ty::callable::callable_throws(db, function).0;
    let erroneous = signature
        .params
        .iter()
        .find(|param| baml_type::contains_error(&param.ty))
        .map(|param| format!("parameter `{}`: {:?}", param.name, param.ty))
        .or_else(|| {
            baml_type::contains_error(&signature.ret)
                .then(|| format!("return type {:?}", signature.ret))
        })
        .or_else(|| baml_type::contains_error(&throws).then(|| format!("throws {throws:?}")));
    if let Some(part) = erroneous {
        panic!(
            "the error sentinel survived to code generation in the signature of {}: {part}",
            owner()
        );
    }
}
