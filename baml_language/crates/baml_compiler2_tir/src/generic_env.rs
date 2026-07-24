use std::sync::Arc;

use baml_base::Name;
use baml_compiler2_ast as ast;
use baml_compiler2_hir::{
    loc::{ClassLoc, FunctionLoc, ImplLoc, InterfaceLoc},
    type_ref::{TypeRefId, TypeRefStore},
};

use crate::ty::{Ty, TyAttr};

/// One declared generic bound before it is lowered in the complete generic scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BoundSource<'db> {
    Ref(&'db TypeRefStore, TypeRefId),
    #[deprecated(
        note = "transitional: dies with the body TypeRef migration; do not add new producers"
    )]
    Ast(ast::TypeExpr),
}

/// One generic parameter declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GenericParam<'db> {
    index: usize,
    name: Name,
    bound: Option<BoundSource<'db>>,
}

impl<'db> GenericParam<'db> {
    pub(crate) fn index(&self) -> usize {
        self.index
    }

    pub(crate) fn name(&self) -> &Name {
        &self.name
    }

    pub(crate) fn bound(&self) -> Option<&BoundSource<'db>> {
        self.bound.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GenericEnvData<'db> {
    parent: Option<GenericEnv<'db>>,
    parent_count: usize,
    own_params: Vec<GenericParam<'db>>,
    own_concrete_bounds: Vec<(Name, baml_type::Interface)>,
    all_param_names: Vec<Name>,
}

/// The generic declarations visible in one TIR scope.
///
/// Like rustc's `ty::Generics`, an environment owns only the parameters declared
/// by its scope and links to the enclosing declaration. `all_param_names` is a
/// derived flat lookup cache for the current name-based `Ty::TypeVar`
/// representation; declaration data remains in `own_params`.
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
        params: impl IntoIterator<Item = (Name, Option<BoundSource<'db>>)>,
    ) -> Self {
        let parent_count = parent.as_ref().map_or(0, Self::param_count);
        let own_params: Vec<_> = params
            .into_iter()
            .enumerate()
            .map(|(offset, (name, bound))| GenericParam {
                index: parent_count + offset,
                name,
                bound,
            })
            .collect();
        let mut all_param_names = parent
            .as_ref()
            .map(|parent| parent.param_names().to_vec())
            .unwrap_or_default();
        all_param_names.extend(own_params.iter().map(|param| param.name.clone()));
        Self(Arc::new(GenericEnvData {
            parent,
            parent_count,
            own_params,
            own_concrete_bounds: Vec::new(),
            all_param_names,
        }))
    }

    pub(crate) fn root_refs(
        params: &[Name],
        store: &'db TypeRefStore,
        bounds: &[Option<TypeRefId>],
    ) -> Self {
        Self::from_sources(
            None,
            params.iter().enumerate().map(|(index, name)| {
                (
                    name.clone(),
                    bounds
                        .get(index)
                        .copied()
                        .flatten()
                        .map(|id| BoundSource::Ref(store, id)),
                )
            }),
        )
    }

    #[cfg(test)]
    pub(crate) fn root_unbounded(params: &[Name]) -> Self {
        Self::from_sources(None, params.iter().cloned().map(|name| (name, None)))
    }

    pub(crate) fn child_refs(
        &self,
        params: &[Name],
        store: &'db TypeRefStore,
        bounds: &[Option<TypeRefId>],
    ) -> Self {
        Self::from_sources(
            Some(self.clone()),
            params.iter().enumerate().map(|(index, name)| {
                (
                    name.clone(),
                    bounds
                        .get(index)
                        .copied()
                        .flatten()
                        .map(|id| BoundSource::Ref(store, id)),
                )
            }),
        )
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
        let mut visible = self.param_names().to_vec();
        let own = params.iter().enumerate().filter_map(|(index, name)| {
            if visible.contains(name) {
                return None;
            }
            visible.push(name.clone());
            Some((
                name.clone(),
                bounds.get(index).cloned().flatten().map(BoundSource::Ast),
            ))
        });
        Self::from_sources(Some(self.clone()), own)
    }

    pub(crate) fn with_additional_own_unbounded(mut self, params: &[Name]) -> Self {
        let data = Arc::make_mut(&mut self.0);
        for name in params {
            let index = data.parent_count + data.own_params.len();
            data.own_params.push(GenericParam {
                index,
                name: name.clone(),
                bound: None,
            });
            data.all_param_names.push(name.clone());
        }
        self
    }

    pub(crate) fn with_concrete_bound(mut self, name: Name, bound: baml_type::Interface) -> Self {
        Arc::make_mut(&mut self.0)
            .own_concrete_bounds
            .push((name, bound));
        self
    }

    pub(crate) fn parent(&self) -> Option<&Self> {
        self.0.parent.as_ref()
    }

    pub(crate) fn parent_count(&self) -> usize {
        self.0.parent_count
    }

    pub(crate) fn own_params(&self) -> &[GenericParam<'db>] {
        &self.0.own_params
    }

    pub(crate) fn visit_params(&self, visitor: &mut impl FnMut(&GenericParam<'db>)) {
        fn visit<'db>(env: &GenericEnv<'db>, visitor: &mut impl FnMut(&GenericParam<'db>)) {
            if let Some(parent) = env.parent() {
                visit(parent, visitor);
            }
            for param in env.own_params() {
                visitor(param);
            }
        }

        visit(self, visitor);
    }

    #[cfg(test)]
    pub(crate) fn all_params(&self) -> Vec<&GenericParam<'db>> {
        fn collect<'a, 'db>(env: &'a GenericEnv<'db>, params: &mut Vec<&'a GenericParam<'db>>) {
            if let Some(parent) = env.parent() {
                collect(parent, params);
            }
            params.extend(env.own_params());
        }

        let mut params = Vec::with_capacity(self.param_count());
        collect(self, &mut params);
        params
    }

    pub(crate) fn param_names(&self) -> &[Name] {
        &self.0.all_param_names
    }

    pub(crate) fn param_count(&self) -> usize {
        self.0.all_param_names.len()
    }

    pub(crate) fn concrete_bounds(&self) -> Vec<(&Name, &baml_type::Interface)> {
        fn collect<'a>(
            env: &'a GenericEnv<'_>,
            bounds: &mut Vec<(&'a Name, &'a baml_type::Interface)>,
        ) {
            if let Some(parent) = env.parent() {
                collect(parent, bounds);
            }
            bounds.extend(
                env.0
                    .own_concrete_bounds
                    .iter()
                    .map(|(name, bound)| (name, bound)),
            );
        }

        let mut bounds = Vec::new();
        collect(self, &mut bounds);
        bounds
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
    let mut env = GenericEnv::root_refs(
        &data.generic_params,
        &data.type_refs,
        &data.generic_param_bounds,
    );
    let self_name = Name::new("Self");
    if !env.param_names().contains(&self_name) {
        env = env.with_additional_own_unbounded(std::slice::from_ref(&self_name));
    }
    let qtn = crate::lower_type_expr::qualify_def(
        db,
        baml_compiler2_hir::contributions::Definition::Interface(interface),
        &data.name,
    );
    let args = data
        .generic_params
        .iter()
        .map(|param| Ty::TypeVar(param.clone(), TyAttr::default()))
        .collect();
    env.with_concrete_bound(self_name, baml_type::Interface::new(qtn, args, Vec::new()))
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
                    (
                        param.name.clone(),
                        param
                            .bounds
                            .first()
                            .copied()
                            .map(|id| BoundSource::Ref(&data.type_refs, id)),
                    )
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
    match parent {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_indices_follow_parent_parameters() {
        let parent = GenericEnv::root_unbounded(&[Name::new("T")]);
        let child = GenericEnv::from_sources(
            Some(parent),
            [(Name::new("U"), None), (Name::new("V"), None)],
        );

        assert_eq!(child.parent_count(), 1);
        assert_eq!(
            child
                .all_params()
                .into_iter()
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
