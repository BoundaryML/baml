use std::sync::Arc;

use baml_base::Name;
use baml_compiler2_ast as ast;
use baml_compiler2_hir::{
    loc::{ClassLoc, FunctionLoc, ImplLoc, InterfaceLoc},
    type_ref::{TypeRefId, TypeRefStore},
};

use crate::ty::{ParamTy, Ty, TyAttr};

/// One declared generic bound before it is lowered in the complete generic scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BoundSource<'db> {
    Ref(&'db TypeRefStore, TypeRefId),
    #[deprecated(
        note = "transitional: dies with the body TypeRef migration; do not add new producers"
    )]
    Ast(ast::TypeExpr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GenericEnvData<'db> {
    parent: Option<GenericEnv<'db>>,
    own_predicates: Vec<(ParamTy, BoundSource<'db>)>,
    self_bound: Option<(ParamTy, baml_type::Interface)>,
    all_params: Vec<ParamTy>,
    source_params: Vec<ParamTy>,
}

/// The generic declarations visible in one TIR scope.
///
/// Like rustc's `ty::Generics`, an environment owns only the parameters declared
/// by its scope and links to the enclosing declaration. `all_params` is a
/// derived flat lookup cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GenericEnv<'db>(Arc<GenericEnvData<'db>>);

impl Default for GenericEnv<'_> {
    fn default() -> Self {
        Self::from_sources(None, std::iter::empty())
    }
}

impl<'db> GenericEnv<'db> {
    fn from_sources(
        parent: Option<Self>,
        params: impl IntoIterator<Item = (Name, Vec<BoundSource<'db>>)>,
    ) -> Self {
        let mut own_predicates = Vec::new();
        let mut all_params = parent
            .as_ref()
            .map(|parent| parent.params().to_vec())
            .unwrap_or_default();
        let mut source_params = parent
            .as_ref()
            .map(|parent| parent.source_params().to_vec())
            .unwrap_or_default();
        for (name, bounds) in params {
            let index =
                u32::try_from(all_params.len()).expect("generic parameter index fits in u32");
            let param = ParamTy::new(index, name);
            own_predicates.extend(bounds.into_iter().map(|bound| (param.clone(), bound)));
            all_params.push(param.clone());
            source_params.push(param);
        }
        Self(Arc::new(GenericEnvData {
            parent,
            own_predicates,
            self_bound: None,
            all_params,
            source_params,
        }))
    }

    fn from_refs(
        parent: Option<Self>,
        params: &[Name],
        store: &'db TypeRefStore,
        bounds: &[Option<TypeRefId>],
    ) -> Self {
        Self::from_sources(
            parent,
            params.iter().enumerate().map(|(index, name)| {
                let bounds = bounds
                    .get(index)
                    .copied()
                    .flatten()
                    .map(|id| BoundSource::Ref(store, id))
                    .into_iter()
                    .collect();
                (name.clone(), bounds)
            }),
        )
    }

    pub(crate) fn root_refs(
        params: &[Name],
        store: &'db TypeRefStore,
        bounds: &[Option<TypeRefId>],
    ) -> Self {
        Self::from_refs(None, params, store, bounds)
    }

    #[cfg(test)]
    pub(crate) fn root_unbounded(params: &[Name]) -> Self {
        Self::from_sources(None, params.iter().cloned().map(|name| (name, Vec::new())))
    }

    pub(crate) fn child_refs(
        &self,
        params: &[Name],
        store: &'db TypeRefStore,
        bounds: &[Option<TypeRefId>],
    ) -> Self {
        Self::from_refs(Some(self.clone()), params, store, bounds)
    }

    #[expect(
        deprecated,
        reason = "the one sanctioned Ast producer until the body TypeRef migration"
    )]
    pub(crate) fn child_unique_ast(
        &self,
        params: &[Name],
        bounds: &[Option<ast::TypeExpr>],
    ) -> Self {
        let mut visible: Vec<_> = self
            .source_params()
            .iter()
            .map(|param| param.name().clone())
            .collect();
        let own = params.iter().enumerate().filter_map(|(index, name)| {
            if visible.contains(name) {
                return None;
            }
            visible.push(name.clone());
            Some((
                name.clone(),
                bounds
                    .get(index)
                    .cloned()
                    .flatten()
                    .map(BoundSource::Ast)
                    .into_iter()
                    .collect(),
            ))
        });
        Self::from_sources(Some(self.clone()), own)
    }

    fn with_additional_params(
        mut self,
        params: impl IntoIterator<Item = Name>,
        source_visible: bool,
    ) -> Self {
        let data = Arc::make_mut(&mut self.0);
        for name in params {
            let index =
                u32::try_from(data.all_params.len()).expect("generic parameter index fits in u32");
            let param = ParamTy::new(index, name);
            data.all_params.push(param.clone());
            if source_visible {
                data.source_params.push(param);
            }
        }
        self
    }

    pub(crate) fn with_additional_own_unbounded(self, params: &[Name]) -> Self {
        self.with_additional_params(params.iter().cloned(), true)
    }

    fn with_associated_params(self, params: impl IntoIterator<Item = Name>) -> Self {
        self.with_additional_params(params, false)
    }

    fn with_self_bound(mut self, param: ParamTy, bound: baml_type::Interface) -> Self {
        Arc::make_mut(&mut self.0).self_bound = Some((param, bound));
        self
    }

    pub(crate) fn parent(&self) -> Option<&Self> {
        self.0.parent.as_ref()
    }

    pub(crate) fn parent_count(&self) -> u32 {
        self.parent().map_or(0, Self::param_count)
    }

    pub(crate) fn own_params(&self) -> &[ParamTy] {
        &self.params()[self.parent_count() as usize..]
    }

    pub(crate) fn visit_predicates(&self, visitor: &mut impl FnMut(&ParamTy, &BoundSource<'db>)) {
        fn visit<'db>(
            env: &GenericEnv<'db>,
            visitor: &mut impl FnMut(&ParamTy, &BoundSource<'db>),
        ) {
            if let Some(parent) = env.parent() {
                visit(parent, visitor);
            }
            for (param, bound) in &env.0.own_predicates {
                visitor(param, bound);
            }
        }

        visit(self, visitor);
    }

    pub(crate) fn params(&self) -> &[ParamTy] {
        &self.0.all_params
    }

    pub(crate) fn source_params(&self) -> &[ParamTy] {
        &self.0.source_params
    }

    pub(crate) fn interface_param_parts(&self) -> (&ParamTy, &[ParamTy]) {
        let (self_param, declared_params) = self
            .source_params()
            .split_first()
            .expect("interface generic environment starts with Self");
        debug_assert_eq!(self_param.as_str(), "Self");
        (self_param, declared_params)
    }

    pub(crate) fn resolve_param(&self, name: &Name) -> Option<&ParamTy> {
        self.source_params()
            .iter()
            .rev()
            .find(|param| param.name() == name)
    }

    pub(crate) fn resolve_any_param(&self, name: &Name) -> Option<&ParamTy> {
        self.params()
            .iter()
            .rev()
            .find(|param| param.name() == name)
    }

    pub(crate) fn param_count(&self) -> u32 {
        u32::try_from(self.0.all_params.len()).expect("generic parameter count fits in u32")
    }

    pub(crate) fn self_bound(&self) -> Option<(&ParamTy, &baml_type::Interface)> {
        if let Some((param, bound)) = &self.0.self_bound {
            return Some((param, bound));
        }
        self.parent()?.self_bound()
    }
}

pub(crate) fn class_generic_env<'db>(
    db: &'db dyn crate::Db,
    class: ClassLoc<'db>,
) -> GenericEnv<'db> {
    let data = baml_compiler2_ppir::item_data::class_data(db, class);
    GenericEnv::root_refs(
        &data.generic_params,
        &data.type_refs,
        &data.generic_param_bounds,
    )
}

pub(crate) fn interface_generic_env<'db>(
    db: &'db dyn crate::Db,
    interface: InterfaceLoc<'db>,
) -> GenericEnv<'db> {
    let data = baml_compiler2_ppir::item_data::interface_data(db, interface);
    let self_name = Name::new("Self");
    let mut params = Vec::with_capacity(1 + data.generic_params.len());
    params.push((self_name, Vec::new()));
    for (index, name) in data.generic_params.iter().enumerate() {
        let bounds = data
            .generic_param_bounds
            .get(index)
            .copied()
            .flatten()
            .map(|id| BoundSource::Ref(&data.type_refs, id))
            .into_iter()
            .collect();
        params.push((name.clone(), bounds));
    }
    let env = GenericEnv::from_sources(None, params)
        .with_associated_params(data.associated_types.iter().map(|assoc| assoc.name.clone()));
    let qtn = crate::lower_type_expr::qualify_def(
        db,
        baml_compiler2_hir::contributions::Definition::Interface(interface),
        &data.name,
    );
    let (self_param, args) = {
        let (self_param, declared_params) = env.interface_param_parts();
        let args = declared_params
            .iter()
            .map(|param| Ty::TypeVar(param.clone(), TyAttr::default()))
            .collect();
        (self_param.clone(), args)
    };
    env.with_self_bound(self_param, baml_type::Interface::new(qtn, args, Vec::new()))
}

pub(crate) fn append_params(parent: &[ParamTy], names: &[Name]) -> Vec<ParamTy> {
    let mut params = parent.to_vec();
    ParamTy::extend_frame(&mut params, names);
    params
}

pub(crate) fn impl_generic_env<'db>(
    db: &'db dyn crate::Db,
    block: ImplLoc<'db>,
) -> GenericEnv<'db> {
    let data = baml_compiler2_ppir::item_data::impl_block_data(db, block);
    match &data.subject {
        baml_compiler2_ppir::item_data::ImplSubjectData::InClass { class, .. } => {
            class_generic_env(db, *class)
        }
        baml_compiler2_ppir::item_data::ImplSubjectData::Free { generics, .. } => {
            GenericEnv::from_sources(
                None,
                generics.iter().map(|param| {
                    let bounds = param
                        .bounds
                        .first()
                        .copied()
                        .map(|id| BoundSource::Ref(&data.type_refs, id))
                        .into_iter()
                        .collect();
                    (param.name.clone(), bounds)
                }),
            )
        }
    }
}

pub(crate) fn function_generic_env<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> GenericEnv<'db> {
    let data = baml_compiler2_ppir::item_data::function_data(db, function);
    let parent = match baml_compiler2_ppir::item_data::method_owner(db, function) {
        Some(baml_compiler2_ppir::item_data::MethodOwner::Class(class)) => {
            Some(class_generic_env(db, class))
        }
        Some(baml_compiler2_ppir::item_data::MethodOwner::Interface(interface)) => {
            Some(interface_generic_env(db, interface))
        }
        Some(baml_compiler2_ppir::item_data::MethodOwner::FreeImpl(block)) => {
            Some(impl_generic_env(db, block))
        }
        None => None,
    };
    let env = match parent {
        Some(parent) => parent.child_refs(
            &data.generic_params,
            &data.type_refs,
            &data.generic_param_bounds,
        ),
        None => GenericEnv::root_refs(
            &data.generic_params,
            &data.type_refs,
            &data.generic_param_bounds,
        ),
    };
    let sig = baml_compiler2_ppir::item_data::elaborated_function_data(db, function);
    env.with_additional_own_unbounded(&sig.synthetic_effect_params)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_indices_follow_parent_parameters() {
        let parent = GenericEnv::root_unbounded(&[Name::new("T")]);
        let child = GenericEnv::from_sources(
            Some(parent),
            [(Name::new("U"), Vec::new()), (Name::new("V"), Vec::new())],
        );

        assert_eq!(child.parent_count(), 1);
        assert_eq!(
            child
                .params()
                .iter()
                .map(|param| (param.index(), param.name().clone()))
                .collect::<Vec<_>>(),
            vec![
                (0, Name::new("T")),
                (1, Name::new("U")),
                (2, Name::new("V")),
            ],
        );
    }
}
