use baml_compiler2_ast::ExprId;
use baml_compiler2_hir::{
    body::BodyOwnerId,
    loc::{FunctionLoc, LetLoc},
    semantic_index::ExprMetadataScope,
};
use baml_compiler2_hir_ty::infer::{InferenceResult, ScopedTypeBinding, infer_body};
use baml_type::ParamTy;
use rustc_hash::FxHashMap;

pub(crate) struct InferenceTables<'db> {
    body: BodyInference<'db>,
    defaults: Option<BodyInference<'db>>,
}

pub(crate) struct BodyInference<'db> {
    pub(crate) result: &'db InferenceResult<'db>,
    pub(crate) runtime_type_bindings: FxHashMap<ExprId, &'db ScopedTypeBinding>,
    pub(crate) runtime_type_params: Vec<ParamTy>,
}

impl<'db> BodyInference<'db> {
    fn new(result: &'db InferenceResult<'db>) -> Self {
        let mut runtime_type_bindings = FxHashMap::default();
        for binding in result.type_ref_bindings.values().flatten() {
            if let Some(operand) = binding.operand {
                runtime_type_bindings.entry(operand).or_insert(binding);
            }
        }
        let mut runtime_type_params: Vec<_> = runtime_type_bindings
            .values()
            .map(|binding| binding.parameter.clone())
            .collect();
        runtime_type_params.sort_by(|left, right| {
            left.index()
                .cmp(&right.index())
                .then_with(|| left.name().cmp(right.name()))
        });
        Self {
            result,
            runtime_type_bindings,
            runtime_type_params,
        }
    }
}

impl<'db> InferenceTables<'db> {
    pub(crate) fn for_function(db: &'db dyn crate::Db, function: FunctionLoc<'db>) -> Self {
        Self {
            body: BodyInference::new(infer_body(db, BodyOwnerId::Function(function))),
            defaults: Some(BodyInference::new(infer_body(
                db,
                BodyOwnerId::ParameterDefaults(function),
            ))),
        }
    }

    pub(crate) fn for_let(db: &'db dyn crate::Db, let_binding: LetLoc<'db>) -> Self {
        Self {
            body: BodyInference::new(infer_body(db, BodyOwnerId::Let(let_binding))),
            defaults: None,
        }
    }

    pub(crate) fn for_scope(&self, scope: ExprMetadataScope) -> Option<&BodyInference<'db>> {
        match scope {
            ExprMetadataScope::Body(_) => Some(&self.body),
            ExprMetadataScope::ParameterDefault(_) => self.defaults.as_ref(),
        }
    }
}
