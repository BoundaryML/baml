//! Resolve interface caller contracts through the language's member lookup.
//! Own implementation obligations stay separate from this inherited surface.

use std::collections::BTreeMap;

use baml_codegen_types as cg;
use baml_compiler2_hir_ty::{facts::Facts, lower, method_resolution as resolution};
use baml_db::ProjectDatabase;
use baml_type::{
    Interface, Name, ParamTy, RuntimeInterface, RuntimeTy, Ty, TyAttr, interned::Ty as InternedTy,
};

type Result<T> = std::result::Result<T, cg::InterfaceExportError>;

fn error(path: &str, reason: impl ToString) -> cg::InterfaceExportError {
    cg::InterfaceExportError {
        path: path.to_owned(),
        reason: reason.to_string(),
    }
}

fn variable(param: &ParamTy) -> Ty {
    Ty::TypeVar(param.clone(), TyAttr::EMPTY)
}

fn rewrite(value: &Ty, parameters: &BTreeMap<ParamTy, Ty>) -> Ty {
    baml_type::unify::rewrite_ty(value, &mut |node| match node {
        Ty::TypeVar(param, _) => parameters.get(param).cloned(),
        _ => None,
    })
}

fn plain_interface(value: &baml_type::interned::InterfaceRef) -> Interface {
    Interface::new(
        value.name.clone(),
        value.generics.iter().map(InternedTy::to_plain).collect(),
        value
            .associated_types
            .iter()
            .map(|(name, ty)| (name.clone(), ty.to_plain()))
            .collect(),
    )
}

fn plain_runtime_interface(value: &RuntimeInterface) -> Interface {
    Interface::new(
        value.name.clone(),
        value.generics.iter().cloned().map(Ty::from).collect(),
        value
            .associated_types
            .iter()
            .map(|(name, ty)| (name.clone(), Ty::from(ty.clone())))
            .collect(),
    )
}

fn rewrite_interface(value: &Interface, parameters: &BTreeMap<ParamTy, Ty>) -> Interface {
    Interface::new(
        value.name.clone(),
        value
            .generics
            .iter()
            .map(|ty| rewrite(ty, parameters))
            .collect(),
        value
            .associated_types
            .iter()
            .map(|(name, ty)| (name.clone(), rewrite(ty, parameters)))
            .collect(),
    )
}

fn runtime_interface(
    value: &Interface,
    facts: &Facts<'_>,
    parameters: &BTreeMap<ParamTy, Ty>,
    path: &str,
) -> Result<RuntimeInterface> {
    let convert = |value: &Ty| {
        let normalized = baml_type::normalize::normalize(value, facts);
        RuntimeTy::try_from(&rewrite(&normalized, parameters)).map_err(|reason| error(path, reason))
    };
    Ok(RuntimeInterface::new(
        value.name.clone(),
        value.generics.iter().map(convert).collect::<Result<_>>()?,
        value
            .associated_types
            .iter()
            .map(|(name, ty)| Ok((name.clone(), convert(ty)?)))
            .collect::<Result<_>>()?,
    ))
}

pub(super) fn build(
    db: &ProjectDatabase,
    declarations: &BTreeMap<cg::Name, cg::InterfaceDeclaration>,
) -> Result<BTreeMap<cg::Name, cg::InterfaceCallers>> {
    let mut result = BTreeMap::new();
    for declaration in declarations.values() {
        let path = declaration.name.to_string();
        // A required method's own frame can reuse both the index and spelling
        // of a root parameter. Resolve with private root variables first, then
        // rebase the method suffix into the root's exported frame.
        let root_params = std::iter::once(&declaration.self_param)
            .chain(declaration.generic_params.iter().map(|p| &p.param))
            .collect::<Vec<_>>();
        let private = root_params
            .iter()
            .enumerate()
            .map(|(index, _)| {
                ParamTy::new(
                    index.try_into().expect("interface frame size"),
                    Name::new(format!("$sdk$interface${index}")),
                )
            })
            .collect::<Vec<_>>();
        let to_private = root_params
            .iter()
            .zip(&private)
            .map(|(original, private)| ((*original).clone(), variable(private)))
            .collect::<BTreeMap<_, _>>();
        let from_private = private
            .iter()
            .zip(&root_params)
            .map(|(private, original)| (private.clone(), variable(original)))
            .collect::<BTreeMap<_, _>>();
        let root = Interface::new(
            declaration.name.clone(),
            private.iter().skip(1).map(variable).collect(),
            vec![],
        );
        let mut bounds = declaration
            .generic_params
            .iter()
            .zip(private.iter().skip(1))
            .map(|(parameter, private)| {
                (
                    private.clone(),
                    parameter
                        .bounds
                        .iter()
                        .map(|bound| {
                            rewrite_interface(&plain_runtime_interface(bound), &to_private)
                        })
                        .collect(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        bounds.insert(private[0].clone(), vec![root]);
        let facts = Facts::with_bounds(db, bounds.into_iter().collect());
        let receiver = InternedTy::from_plain(&variable(&private[0]));
        let mut callers = cg::InterfaceCallers::default();
        for candidate in resolution::member_candidates(db, &facts, &receiver) {
            if !candidate.is_method {
                continue;
            }
            let member_path = format!("{path}.{}", candidate.name);
            let member =
                match resolution::lookup_interface_member(db, &facts, &receiver, &candidate.name) {
                    resolution::InterfaceMemberLookup::Found(member) if member.is_method => member,
                    resolution::InterfaceMemberLookup::Ambiguous { sources, .. } => {
                        let mut sources = sources
                            .iter()
                            .map(|source| {
                                runtime_interface(
                                    &plain_interface(source),
                                    &facts,
                                    &from_private,
                                    &member_path,
                                )
                            })
                            .collect::<Result<Vec<_>>>()?;
                        sources.sort();
                        callers.ambiguous_methods.insert(candidate.name, sources);
                        continue;
                    }
                    // A field shadows a same-named method according to BAML.
                    resolution::InterfaceMemberLookup::Found(_) => continue,
                    _ => {
                        return Err(error(
                            &member_path,
                            "enumerated interface member did not resolve",
                        ));
                    }
                };
            let target = declarations
                .get(&member.declaring_interface.name)
                .ok_or_else(|| error(&member_path, "missing declaring interface"))?;
            let mut method = target
                .required_methods
                .iter()
                .chain(&target.default_methods)
                .find(|method| method.name == candidate.name)
                .ok_or_else(|| error(&member_path, "missing declaring method"))?
                .clone();
            let mut parameters = from_private.clone();
            let own = super::concrete_export::own_parameters(
                db,
                member.pending_own,
                &facts,
                &member_path,
            )?;
            let rebased = own
                .iter()
                .enumerate()
                .map(|(index, parameter)| {
                    ParamTy::new(
                        (root_params.len() + index)
                            .try_into()
                            .expect("method frame size"),
                        Name::new(parameter.param.as_str()),
                    )
                })
                .collect::<Vec<_>>();
            for (parameter, rebased) in own.iter().zip(&rebased) {
                parameters.insert(parameter.param.clone(), variable(rebased));
            }
            let signature =
                super::concrete_export::runtime_signature(&member.ty, &facts, &member_path)?;
            let signature = RuntimeTy::try_from(&rewrite(&Ty::from(signature), &parameters))
                .map_err(|reason| error(&member_path, reason))?;
            let RuntimeTy::Function { params, ret, .. } = signature else {
                return Err(error(
                    &member_path,
                    "interface member has no function contract",
                ));
            };
            // Preserve callable effects (including inferred default-body
            // effects) instead of reconstructing them from annotations.
            let prefix = std::iter::once(receiver.clone())
                .chain(member.declaring_interface.generics.iter().cloned())
                .collect::<Vec<_>>();
            let effect = |value: &RuntimeTy| {
                let substituted = lower::substitute_params(
                    &InternedTy::from_plain(&Ty::from(value.clone())),
                    &prefix,
                );
                let normalized = baml_type::normalize::normalize(&substituted.to_plain(), &facts);
                RuntimeTy::try_from(&rewrite(&normalized, &parameters))
                    .map_err(|reason| error(&member_path, reason))
            };
            method.params = params;
            method.return_type = *ret;
            method.declared_throws = method.declared_throws.as_ref().map(effect).transpose()?;
            method.callable_throws = effect(&method.callable_throws)?;
            method.generic_params = own
                .iter()
                .zip(rebased)
                .map(|(parameter, param)| {
                    Ok(cg::InterfaceParameter {
                        param,
                        bounds: parameter
                            .bounds
                            .iter()
                            .map(|bound| {
                                runtime_interface(
                                    &plain_runtime_interface(bound),
                                    &facts,
                                    &parameters,
                                    &member_path,
                                )
                            })
                            .collect::<Result<_>>()?,
                    })
                })
                .collect::<Result<_>>()?;
            callers.methods.push(cg::InterfaceCaller {
                declaring_interface: runtime_interface(
                    &plain_interface(&member.declaring_interface),
                    &facts,
                    &from_private,
                    &member_path,
                )?,
                method,
            });
        }
        callers
            .methods
            .sort_by(|a, b| a.method.name.cmp(&b.method.name));
        result.insert(declaration.name.clone(), callers);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDbExt;
    use std::path::Path;

    fn graph(source: &str) -> cg::InterfaceGraph {
        let root = Path::new("/tmp/interface_caller_contracts");
        let mut db = ProjectDatabase::new();
        db.workspace(root);
        db.file(&root.join("main.baml"), source);
        let diagnostics = baml_db::collect_compiler2_diagnostics(&db);
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        super::super::interface_export::build(&db).unwrap()
    }

    fn name(value: &str) -> cg::Name {
        cg::Name::new("user".into(), vec![], value.into())
    }

    fn method<'a>(
        declaration: &'a cg::InterfaceDeclaration,
        name: &str,
    ) -> &'a cg::InterfaceCaller {
        declaration
            .callers
            .methods
            .iter()
            .find(|m| m.method.name.as_str() == name)
            .unwrap()
    }

    #[test]
    fn required_callers_normalize_transitive_pins_without_changing_own_obligations() {
        let graph = graph(include_str!(
            "../../../../interface_probes/baml/required_interfaces.baml"
        ));
        let root = &graph.declarations[&name("RequiredRoot")];
        assert!(root.required_methods.is_empty());
        assert!(root.default_methods.is_empty());
        let apply = method(root, "apply");
        assert_eq!(
            apply.declaring_interface,
            RuntimeInterface::new(
                name("RequiredBase"),
                vec![RuntimeTy::string()],
                vec![
                    (
                        Name::new("Output"),
                        RuntimeTy::TypeVar(root.generic_params[0].param.clone(), TyAttr::EMPTY)
                    ),
                    (
                        Name::new("Error"),
                        RuntimeTy::Never {
                            attr: TyAttr::EMPTY
                        }
                    ),
                ]
            )
        );
        assert_eq!(apply.method.params[1].name, Some(Name::new("value")));
        assert_eq!(apply.method.params[1].ty, RuntimeTy::string());
        assert_eq!(
            apply.method.return_type,
            RuntimeTy::TypeVar(root.generic_params[0].param.clone(), TyAttr::EMPTY)
        );
        assert_eq!(
            apply.method.callable_throws,
            RuntimeTy::Never {
                attr: TyAttr::EMPTY
            }
        );
        let unpinned = &graph.declarations[&name("RequiredUnpinned")];
        let apply = method(unpinned, "apply");
        assert!(apply.declaring_interface.associated_types.is_empty());
        assert!(matches!(
            apply.method.return_type,
            RuntimeTy::AssociatedTypeProjection { .. }
        ));
        assert!(matches!(
            apply.method.callable_throws,
            RuntimeTy::AssociatedTypeProjection { .. }
        ));
    }

    #[test]
    fn required_callers_keep_root_and_method_generic_frames_distinct() {
        let graph = graph(
            r#"
interface Identity {
    function echo<T>(self, value: T) -> T throws never { value }
}
interface Nested<T> requires Identity {
    function own(self, value: T) -> T throws never { value }
}
"#,
        );
        let nested = &graph.declarations[&name("Nested")];
        let echo = &method(nested, "echo").method;
        let own = &method(nested, "own").method;
        let root_param = &nested.generic_params[0].param;
        let method_param = &echo.generic_params[0].param;
        assert_ne!(method_param, root_param);
        assert_eq!(method_param.as_str(), "T");
        assert_eq!(
            method_param.index() as usize,
            nested.generic_params.len() + 1
        );
        assert_eq!(
            echo.params[1].ty,
            RuntimeTy::TypeVar(method_param.clone(), TyAttr::EMPTY)
        );
        assert_eq!(echo.return_type, echo.params[1].ty);
        assert_eq!(
            own.return_type,
            RuntimeTy::TypeVar(root_param.clone(), TyAttr::EMPTY)
        );
    }

    #[test]
    fn required_callers_preserve_shadowing_diamonds_ambiguity_and_self_restrictions() {
        let graph = graph(
            r#"
interface Base {
    function label(self) -> string throws never { "base" }
    function combine(self, other: Self) -> Self throws never { other }
    function create() -> int throws never { 1 }
}
interface Left requires Base {}
interface Right requires Base {}
interface Diamond requires Left, Right {}
interface Other { function label(self) -> int throws never { 2 } }
interface Ambiguous requires Base, Other {}
interface Shadow requires Base, Other { function label(self) -> bool throws never { true } }
"#,
        );
        let diamond = &graph.declarations[&name("Diamond")];
        assert!(diamond.callers.ambiguous_methods.is_empty());
        assert_eq!(
            method(diamond, "label").declaring_interface.name,
            name("Base")
        );
        assert_eq!(
            method(diamond, "combine").method.callability,
            Some(cg::InterfaceCallability::ConcreteSelf)
        );
        assert_eq!(
            method(diamond, "create").method.callability,
            Some(cg::InterfaceCallability::Receiverless)
        );
        let ambiguous = &graph.declarations[&name("Ambiguous")];
        assert_eq!(
            ambiguous.callers.ambiguous_methods[&Name::new("label")].len(),
            2
        );
        assert!(
            !ambiguous
                .callers
                .methods
                .iter()
                .any(|m| m.method.name.as_str() == "label")
        );
        let shadow = &graph.declarations[&name("Shadow")];
        assert!(shadow.callers.ambiguous_methods.is_empty());
        assert_eq!(
            method(shadow, "label").declaring_interface.name,
            name("Shadow")
        );
        assert_eq!(
            method(shadow, "label").method.return_type,
            RuntimeTy::bool()
        );
    }
}
