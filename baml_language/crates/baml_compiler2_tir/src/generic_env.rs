use std::sync::Arc;

use baml_base::Name;
use baml_compiler2_ast as ast;
use baml_compiler2_hir::{
    loc::{ClassLoc, FunctionLoc, ImplLoc, InterfaceLoc},
    scope::ScopeId,
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

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) enum GenericOwner<'db> {
    Class(ClassLoc<'db>),
    Interface(InterfaceLoc<'db>),
    Function(FunctionLoc<'db>),
    Impl(ImplLoc<'db>),
    RequiredMethod {
        interface: InterfaceLoc<'db>,
        method_index: u32,
    },
    Scope(ScopeId<'db>),
    Builtin(Name),
}

impl std::fmt::Debug for GenericOwner<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Class(_) => f.write_str("Class"),
            Self::Interface(_) => f.write_str("Interface"),
            Self::Function(_) => f.write_str("Function"),
            Self::Impl(_) => f.write_str("Impl"),
            Self::RequiredMethod { method_index, .. } => {
                f.debug_tuple("RequiredMethod").field(method_index).finish()
            }
            Self::Scope(_) => f.write_str("Scope"),
            Self::Builtin(name) => f.debug_tuple("Builtin").field(name).finish(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum GenericParamKind {
    Type,
    SelfType,
    AssociatedType,
    SyntheticEffect,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct GenericParamDefId<'db> {
    owner: GenericOwner<'db>,
    kind: GenericParamKind,
    local_index: u32,
}

/// One generic parameter declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GenericParamDef<'db> {
    def_id: GenericParamDefId<'db>,
    param: ParamTy,
}

impl GenericParamDef<'_> {
    pub(crate) fn param(&self) -> &ParamTy {
        &self.param
    }

    pub(crate) fn kind(&self) -> GenericParamKind {
        self.def_id.kind
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GenericEnvData<'db> {
    parent: Option<GenericEnv<'db>>,
    parent_count: u32,
    own_params: Vec<GenericParamDef<'db>>,
    own_predicates: Vec<(ParamTy, BoundSource<'db>)>,
    own_concrete_bounds: Vec<(ParamTy, baml_type::Interface)>,
    all_params: Vec<ParamTy>,
    source_params: Vec<ParamTy>,
}

/// The generic declarations visible in one TIR scope.
///
/// Like rustc's `ty::Generics`, an environment owns only the parameters declared
/// by its scope and links to the enclosing declaration. `all_params` is a
/// derived flat lookup cache; declaration data remains in `own_params`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GenericEnv<'db>(Arc<GenericEnvData<'db>>);

impl Default for GenericEnv<'_> {
    fn default() -> Self {
        Self::from_sources(
            None,
            &GenericOwner::Builtin(Name::new("<empty>")),
            std::iter::empty(),
        )
    }
}

impl<'db> GenericEnv<'db> {
    fn from_sources(
        parent: Option<Self>,
        owner: &GenericOwner<'db>,
        params: impl IntoIterator<Item = (GenericParamKind, Name, Vec<BoundSource<'db>>)>,
    ) -> Self {
        let parent_count = parent.as_ref().map_or(0, Self::param_count);
        let mut own_predicates = Vec::new();
        let own_params: Vec<_> = params
            .into_iter()
            .enumerate()
            .map(|(local_index, (kind, name, bounds))| {
                let index = parent_count
                    + u32::try_from(local_index).expect("generic parameter index fits in u32");
                let param = ParamTy::new(index, name);
                own_predicates.extend(bounds.into_iter().map(|bound| (param.clone(), bound)));
                GenericParamDef {
                    def_id: GenericParamDefId {
                        owner: owner.clone(),
                        kind,
                        local_index: u32::try_from(local_index)
                            .expect("generic parameter index fits in u32"),
                    },
                    param,
                }
            })
            .collect();
        let mut all_params = parent
            .as_ref()
            .map(|parent| parent.params().to_vec())
            .unwrap_or_default();
        all_params.extend(own_params.iter().map(|param| param.param.clone()));
        let mut source_params = parent
            .as_ref()
            .map(|parent| parent.source_params().to_vec())
            .unwrap_or_default();
        source_params.extend(
            own_params
                .iter()
                .filter(|param| param.kind() != GenericParamKind::AssociatedType)
                .map(|param| param.param.clone()),
        );
        Self(Arc::new(GenericEnvData {
            parent,
            parent_count,
            own_params,
            own_predicates,
            own_concrete_bounds: Vec::new(),
            all_params,
            source_params,
        }))
    }

    pub(crate) fn root_refs(
        owner: &GenericOwner<'db>,
        params: &[Name],
        store: &'db TypeRefStore,
        bounds: &[Option<TypeRefId>],
    ) -> Self {
        Self::from_sources(
            None,
            owner,
            params.iter().enumerate().map(|(index, name)| {
                let bounds = bounds
                    .get(index)
                    .copied()
                    .flatten()
                    .map(|id| BoundSource::Ref(store, id))
                    .into_iter()
                    .collect();
                (GenericParamKind::Type, name.clone(), bounds)
            }),
        )
    }

    #[cfg(test)]
    pub(crate) fn root_unbounded(params: &[Name]) -> Self {
        Self::from_sources(
            None,
            &GenericOwner::Builtin(Name::new("<test>")),
            params
                .iter()
                .cloned()
                .map(|name| (GenericParamKind::Type, name, Vec::new())),
        )
    }

    pub(crate) fn child_refs(
        &self,
        owner: &GenericOwner<'db>,
        params: &[Name],
        store: &'db TypeRefStore,
        bounds: &[Option<TypeRefId>],
    ) -> Self {
        Self::from_sources(
            Some(self.clone()),
            owner,
            params.iter().enumerate().map(|(index, name)| {
                let bounds = bounds
                    .get(index)
                    .copied()
                    .flatten()
                    .map(|id| BoundSource::Ref(store, id))
                    .into_iter()
                    .collect();
                (GenericParamKind::Type, name.clone(), bounds)
            }),
        )
    }

    #[expect(
        deprecated,
        reason = "the one sanctioned Ast producer until the body TypeRef migration"
    )]
    pub(crate) fn child_unique_ast(
        &self,
        owner: &GenericOwner<'db>,
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
                GenericParamKind::Type,
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
        Self::from_sources(Some(self.clone()), owner, own)
    }

    pub(crate) fn with_additional_own_unbounded(
        mut self,
        owner: &GenericOwner<'db>,
        kind: GenericParamKind,
        params: &[Name],
    ) -> Self {
        let data = Arc::make_mut(&mut self.0);
        for (local_index, name) in params.iter().enumerate() {
            let index = data.parent_count
                + u32::try_from(data.own_params.len())
                    .expect("generic parameter index fits in u32");
            let def_id = GenericParamDefId {
                owner: owner.clone(),
                kind,
                local_index: u32::try_from(local_index)
                    .expect("generic parameter index fits in u32"),
            };
            let param = ParamTy::new(index, name.clone());
            data.own_params.push(GenericParamDef {
                def_id,
                param: param.clone(),
            });
            data.all_params.push(param.clone());
            if kind != GenericParamKind::AssociatedType {
                data.source_params.push(param);
            }
        }
        self
    }

    pub(crate) fn with_concrete_bound(
        mut self,
        param: ParamTy,
        bound: baml_type::Interface,
    ) -> Self {
        Arc::make_mut(&mut self.0)
            .own_concrete_bounds
            .push((param, bound));
        self
    }

    pub(crate) fn parent(&self) -> Option<&Self> {
        self.0.parent.as_ref()
    }

    pub(crate) fn parent_count(&self) -> u32 {
        self.0.parent_count
    }

    pub(crate) fn own_params(&self) -> &[GenericParamDef<'db>] {
        &self.0.own_params
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

    #[cfg(test)]
    pub(crate) fn all_param_defs(&self) -> Vec<&GenericParamDef<'db>> {
        fn collect<'a, 'db>(env: &'a GenericEnv<'db>, params: &mut Vec<&'a GenericParamDef<'db>>) {
            if let Some(parent) = env.parent() {
                collect(parent, params);
            }
            params.extend(env.own_params());
        }

        let mut params = Vec::with_capacity(self.param_count() as usize);
        collect(self, &mut params);
        params
    }

    pub(crate) fn params(&self) -> &[ParamTy] {
        &self.0.all_params
    }

    pub(crate) fn source_params(&self) -> &[ParamTy] {
        &self.0.source_params
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

    pub(crate) fn concrete_bounds(&self) -> Vec<(&ParamTy, &baml_type::Interface)> {
        fn collect<'a>(
            env: &'a GenericEnv<'_>,
            bounds: &mut Vec<(&'a ParamTy, &'a baml_type::Interface)>,
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
        &GenericOwner::Class(class),
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
    let mut params =
        Vec::with_capacity(1 + data.generic_params.len() + data.associated_types.len());
    params.push((GenericParamKind::SelfType, self_name.clone(), Vec::new()));
    for (index, name) in data.generic_params.iter().enumerate() {
        let bounds = data
            .generic_param_bounds
            .get(index)
            .copied()
            .flatten()
            .map(|id| BoundSource::Ref(&data.type_refs, id))
            .into_iter()
            .collect();
        params.push((GenericParamKind::Type, name.clone(), bounds));
    }
    params.extend(data.associated_types.iter().map(|assoc| {
        (
            GenericParamKind::AssociatedType,
            assoc.name.clone(),
            Vec::new(),
        )
    }));
    let env = GenericEnv::from_sources(None, &GenericOwner::Interface(interface), params);
    let qtn = crate::lower_type_expr::qualify_def(
        db,
        baml_compiler2_hir::contributions::Definition::Interface(interface),
        &data.name,
    );
    let args = data
        .generic_params
        .iter()
        .map(|name| {
            Ty::TypeVar(
                env.resolve_param(name)
                    .expect("interface generic parameter is in its environment")
                    .clone(),
                TyAttr::default(),
            )
        })
        .collect();
    let self_param = env
        .resolve_param(&self_name)
        .expect("interface Self parameter is in its environment")
        .clone();
    env.with_concrete_bound(self_param, baml_type::Interface::new(qtn, args, Vec::new()))
}

pub(crate) fn interface_declared_params<'db>(
    db: &'db dyn crate::Db,
    interface: InterfaceLoc<'db>,
) -> Vec<ParamTy> {
    let data = baml_compiler2_ppir::item_data::interface_data(db, interface);
    let env = interface_generic_env(db, interface);
    data.generic_params
        .iter()
        .map(|name| {
            env.resolve_param(name)
                .expect("interface generic parameter is in its environment")
                .clone()
        })
        .collect()
}

pub(crate) fn append_params(parent: &[ParamTy], names: &[Name]) -> Vec<ParamTy> {
    let mut params = parent.to_vec();
    let first_index = parent
        .iter()
        .map(ParamTy::index)
        .max()
        .map_or(0, |index| index + 1);
    params.extend(names.iter().enumerate().map(|(offset, name)| {
        ParamTy::new(
            first_index + u32::try_from(offset).expect("generic parameter index fits in u32"),
            name.clone(),
        )
    }));
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
                &GenericOwner::Impl(block),
                generics.iter().map(|param| {
                    let bounds = param
                        .bounds
                        .first()
                        .copied()
                        .map(|id| BoundSource::Ref(&data.type_refs, id))
                        .into_iter()
                        .collect();
                    (GenericParamKind::Type, param.name.clone(), bounds)
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
    let owner = GenericOwner::Function(function);
    let env = match parent {
        Some(parent) => parent.child_refs(
            &owner,
            &data.generic_params,
            &data.type_refs,
            &data.generic_param_bounds,
        ),
        None => GenericEnv::root_refs(
            &owner,
            &data.generic_params,
            &data.type_refs,
            &data.generic_param_bounds,
        ),
    };
    let sig = baml_compiler2_ppir::item_data::elaborated_function_data(db, function);
    env.with_additional_own_unbounded(
        &owner,
        GenericParamKind::SyntheticEffect,
        &sig.synthetic_effect_params,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_indices_follow_parent_parameters() {
        let parent = GenericEnv::root_unbounded(&[Name::new("T")]);
        let child = GenericEnv::from_sources(
            Some(parent),
            &GenericOwner::Builtin(Name::new("<child-test>")),
            [
                (GenericParamKind::Type, Name::new("U"), Vec::new()),
                (GenericParamKind::Type, Name::new("V"), Vec::new()),
            ],
        );

        assert_eq!(child.parent_count(), 1);
        assert_eq!(
            child
                .all_param_defs()
                .into_iter()
                .map(|param| (param.param().index(), param.param().name().clone()))
                .collect::<Vec<_>>(),
            vec![
                (0, Name::new("T")),
                (1, Name::new("U")),
                (2, Name::new("V")),
            ],
        );
    }
}
