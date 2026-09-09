//! Resolve concrete caller contracts through the language's member ladder.
//! Generators must not reconstruct implementation matching or pick a method
//! merely because its name occurs in an implements block.

use std::collections::BTreeMap;

use baml_codegen_types as cg;
use baml_compiler2_hir::package::PackageId;
use baml_compiler2_hir_ty::{
    facts::Facts,
    lower,
    method_resolution::{
        self as resolution, InterfaceMemberLookup, MethodCandidateSource, PendingOwnGenerics,
    },
    package_interface::{self, ExportedType},
};
use baml_db::ProjectDatabase;
use baml_type::{
    Name, ParamTy, RuntimeInterface, RuntimeTy, Ty as PlainTy, TyAttr,
    interned::{InterfaceRef, Ty},
};

type Result<T> = std::result::Result<T, cg::InterfaceExportError>;

fn error(path: &str, reason: impl ToString) -> cg::InterfaceExportError {
    cg::InterfaceExportError {
        path: path.to_owned(),
        reason: reason.to_string(),
    }
}

fn runtime_ty(value: &Ty, facts: &Facts<'_>, path: &str) -> Result<RuntimeTy> {
    RuntimeTy::try_from(&baml_type::normalize::normalize(&value.to_plain(), facts))
        .map_err(|reason| error(path, reason))
}

pub(super) fn runtime_signature(value: &Ty, facts: &Facts<'_>, path: &str) -> Result<RuntimeTy> {
    let baml_type::interned::TyKind::Function {
        params,
        ret,
        throws,
        attr,
    } = value.kind()
    else {
        return Err(error(path, "resolved method is not a function"));
    };
    // Type normalization may discard required parameter names because they
    // do not affect function subtyping. A declaration API still needs those
    // names for keyword calls, so normalize its component types separately.
    Ok(RuntimeTy::Function {
        params: params
            .iter()
            .map(|param| {
                Ok(baml_type::RuntimeFunctionParamTy {
                    name: param.name.clone(),
                    ty: runtime_ty(&param.ty, facts, path)?,
                    mode: param.mode,
                })
            })
            .collect::<Result<_>>()?,
        ret: Box::new(runtime_ty(ret, facts, path)?),
        throws: Box::new(runtime_ty(throws, facts, path)?),
        attr: attr.clone(),
    })
}

fn interface(value: &InterfaceRef, facts: &Facts<'_>, path: &str) -> Result<RuntimeInterface> {
    Ok(RuntimeInterface::new(
        value.name.clone(),
        value
            .generics
            .iter()
            .map(|ty| runtime_ty(ty, facts, path))
            .collect::<Result<_>>()?,
        value
            .associated_types
            .iter()
            .map(|(name, ty)| Ok((name.clone(), runtime_ty(ty, facts, path)?)))
            .collect::<Result<_>>()?,
    ))
}

fn substitute_bound(bound: &baml_type::Interface, prefix: &[Ty]) -> baml_type::Interface {
    baml_type::Interface::new(
        bound.name.clone(),
        bound
            .generics
            .iter()
            .map(|ty| lower::substitute_params(&Ty::from_plain(ty), prefix).to_plain())
            .collect(),
        bound
            .associated_types
            .iter()
            .map(|(name, ty)| {
                (
                    name.clone(),
                    lower::substitute_params(&Ty::from_plain(ty), prefix).to_plain(),
                )
            })
            .collect(),
    )
}

pub(super) fn own_parameters(
    db: &ProjectDatabase,
    pending: Option<PendingOwnGenerics<'_>>,
    facts: &Facts<'_>,
    path: &str,
) -> Result<Vec<cg::InterfaceParameter>> {
    let (params, bounds, prefix) = match pending {
        None => return Ok(Vec::new()),
        Some(PendingOwnGenerics::Source { method, prefix }) => {
            let signature = lower::function_signature(db, method);
            let all_bounds = lower::function_generic_bounds(db, method);
            let params = signature.generic_params[prefix.len()..].to_vec();
            let bounds = params
                .iter()
                .map(|param| {
                    all_bounds
                        .get(param)
                        .into_iter()
                        .flatten()
                        .map(|bound| {
                            let PlainTy::Interface(name, args, pins, _) =
                                bound.existential().to_plain()
                            else {
                                unreachable!()
                            };
                            baml_type::Interface::new(name, args, pins)
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            (params, bounds, prefix)
        }
        Some(PendingOwnGenerics::External { function, prefix }) => (
            function.generic_params,
            function.generic_param_bounds,
            prefix,
        ),
    };
    if params.len() != bounds.len() {
        return Err(error(
            path,
            "method generic parameter and bound counts differ",
        ));
    }
    params
        .into_iter()
        .zip(bounds)
        .map(|(param, bounds)| {
            Ok(cg::InterfaceParameter {
                param,
                bounds: bounds
                    .iter()
                    .map(|bound| {
                        interface(
                            &InterfaceRef::from_constraint(&substitute_bound(bound, &prefix)),
                            facts,
                            path,
                        )
                    })
                    .collect::<Result<_>>()?,
            })
        })
        .collect()
}

pub(super) fn build(
    db: &ProjectDatabase,
    declarations: &BTreeMap<cg::Name, cg::InterfaceDeclaration>,
) -> Result<BTreeMap<cg::Name, cg::ConcreteDeclaration>> {
    let mut out = BTreeMap::new();
    for package in super::interface_export::packages(db) {
        let exported = package_interface::package_interface(db, PackageId::new(db, package));
        for declaration in exported
            .types
            .values()
            .flat_map(|namespace| namespace.values())
        {
            let ExportedType::Class {
                qtn,
                boundary_projection,
                generic_params,
                generic_param_bounds,
                ..
            } = declaration
            else {
                continue;
            };
            if *boundary_projection == baml_type::ClassProjection::Builtin {
                continue;
            }
            let path = qtn.to_string();
            if generic_params.len() != generic_param_bounds.len() {
                return Err(error(
                    &path,
                    "class generic parameter and bound counts differ",
                ));
            }
            // A default method's own frame may overlap the class's indices
            // and authored names. Private rigid class parameters prevent
            // capturing that method variable during receiver substitution.
            let private_params = generic_params
                .iter()
                .enumerate()
                .map(|(index, _)| {
                    ParamTy::new(
                        index.try_into().expect("class parameter count"),
                        Name::new(format!("$sdk$class${index}")),
                    )
                })
                .collect::<Vec<_>>();
            let args = private_params
                .iter()
                .map(|param| Ty::from_plain(&PlainTy::TypeVar(param.clone(), TyAttr::default())))
                .collect::<Vec<_>>();
            let facts = Facts::with_bounds(
                db,
                private_params
                    .iter()
                    .zip(generic_param_bounds)
                    .map(|(param, bounds)| {
                        (
                            param.clone(),
                            bounds
                                .iter()
                                .map(|bound| substitute_bound(bound, &args))
                                .collect(),
                        )
                    })
                    .collect(),
            );
            let receiver = lower::class_ty(qtn.clone(), args);
            let params = private_params
                .iter()
                .map(|param| {
                    Ok(cg::InterfaceParameter {
                        param: param.clone(),
                        bounds: facts
                            .bounds()
                            .get(param)
                            .into_iter()
                            .flatten()
                            .map(|bound| {
                                interface(&InterfaceRef::from_constraint(bound), &facts, &path)
                            })
                            .collect::<Result<_>>()?,
                    })
                })
                .collect::<Result<_>>()?;
            let mut implemented_interfaces =
                resolution::concrete_interface_views(db, &facts, &receiver)
                    .iter()
                    .map(|view| interface(view, &facts, &path))
                    .collect::<Result<Vec<_>>>()?;
            implemented_interfaces.sort();
            implemented_interfaces.dedup();
            for view in &implemented_interfaces {
                let declaration = declarations.get(&view.name).ok_or_else(|| {
                    error(
                        &path,
                        format!("missing interface declaration {}", view.name),
                    )
                })?;
                if view.generics.len() != declaration.generic_params.len()
                    || view.associated_types.len() != declaration.associated_types.len()
                    || declaration.associated_types.iter().any(|member| {
                        !view
                            .associated_types
                            .iter()
                            .any(|(name, _)| name == &member.name)
                    })
                {
                    return Err(error(
                        &path,
                        format!("incomplete implemented view {}", view.name),
                    ));
                }
            }
            let mut result = cg::ConcreteDeclaration {
                name: qtn.clone(),
                generic_params: params,
                implemented_interfaces,
                methods: Vec::new(),
                ambiguous_methods: BTreeMap::new(),
            };
            // A copied record still supplies checked interface input evidence.
            // Downstream/blanket methods do not change its native projection.
            if *boundary_projection != baml_type::ClassProjection::Live {
                out.insert(qtn.clone(), result);
                continue;
            }
            for candidate in resolution::member_candidates(db, &facts, &receiver) {
                if !candidate.is_method || candidate.is_static {
                    continue;
                }
                let method_path = format!("{path}.{}", candidate.name);
                // Match the compiler's ambiguity gate before its inherent
                // tier; enumeration alone deliberately includes ambiguous names.
                if let Some((sources, _)) =
                    resolution::concrete_member_ambiguity(db, &facts, &receiver, &candidate.name)
                {
                    let mut sources = sources
                        .iter()
                        .map(|source| interface(source, &facts, &method_path))
                        .collect::<Result<Vec<_>>>()?;
                    sources.sort();
                    result.ambiguous_methods.insert(candidate.name, sources);
                    continue;
                }
                // Class-owned source lists also contain in-body impl methods.
                // Their compiler provenance sends them through the same impl
                // lookup as out-of-body rules, never a made-up named function.
                let inherent = resolution::lookup_method(db, &facts, &receiver, &candidate.name)
                    .filter(|method| match &method.source {
                        MethodCandidateSource::Source { method, .. } =>
                            baml_compiler2_ppir::item_data::method_interface_target(db, *method).is_none(),
                        MethodCandidateSource::External(function) =>
                            !function.external.as_ref().is_some_and(|callable| matches!(
                                callable.target,
                                baml_compiler2_hir_ty::callable::ExternalCallTarget::Interface { .. }
                            )),
                    });
                let (signature, pending, target) = if let Some(method) = inherent {
                    let prefix = method.class_args;
                    let (signature, pending) = match method.source {
                        MethodCandidateSource::Source { method, .. } => {
                            let signature = lower::function_signature(db, method);
                            (
                                resolution::instantiate_signature(signature, &prefix),
                                Some(PendingOwnGenerics::Source { method, prefix }),
                            )
                        }
                        MethodCandidateSource::External(function) => (
                            resolution::instantiate_external_signature(&function, &prefix),
                            Some(PendingOwnGenerics::External { function, prefix }),
                        ),
                    };
                    (
                        signature,
                        pending,
                        cg::ConcreteMethodTarget::Inherent { class: qtn.clone() },
                    )
                } else {
                    match resolution::lookup_interface_member(
                        db,
                        &facts,
                        &receiver,
                        &candidate.name,
                    ) {
                        InterfaceMemberLookup::Found(member) if member.is_method => {
                            let obligation =
                                interface(&member.declaring_interface, &facts, &method_path)?;
                            // Member lookup carries the declaring obligation,
                            // which may omit associated pins. The checked
                            // bridge target needs the complete compiler-proven
                            // view, including defaults and block overrides.
                            let mut views = result.implemented_interfaces.iter().filter(|view| {
                                view.name == obligation.name
                                    && view.generics == obligation.generics
                                    && obligation
                                        .associated_types
                                        .iter()
                                        .all(|pin| view.associated_types.contains(pin))
                            });
                            let target = views
                                .next()
                                .ok_or_else(|| {
                                    error(
                                        &method_path,
                                        "concrete method has no complete implemented view",
                                    )
                                })?
                                .clone();
                            if views.next().is_some() {
                                return Err(error(
                                    &method_path,
                                    "concrete method has multiple complete implemented views",
                                ));
                            }
                            (
                                member.ty,
                                member.pending_own,
                                cg::ConcreteMethodTarget::Interface(target),
                            )
                        }
                        // Conditional methods not provable in the declaration's
                        // parameter environment do not become unconditional APIs.
                        InterfaceMemberLookup::NotFound => continue,
                        _ => {
                            return Err(error(
                                &method_path,
                                "enumerated method did not resolve to a concrete callable",
                            ));
                        }
                    }
                };
                result.methods.push(cg::ConcreteMethod {
                    name: candidate.name,
                    signature: runtime_signature(&signature, &facts, &method_path)?,
                    generic_params: own_parameters(db, pending, &facts, &method_path)?,
                    target,
                });
            }
            result
                .methods
                .sort_by(|left, right| left.name.cmp(&right.name));
            out.insert(qtn.clone(), result);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDbExt;
    use std::path::Path;

    fn graph(source: &str) -> cg::InterfaceGraph {
        let root = Path::new("/tmp/concrete_export_contracts");
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

    fn method<'a>(class: &'a cg::ConcreteDeclaration, name: &str) -> &'a cg::ConcreteMethod {
        class
            .methods
            .iter()
            .find(|method| method.name.as_str() == name)
            .unwrap()
    }

    #[test]
    fn concrete_export_preserves_implementation_refinement_and_concrete_self() {
        let graph = graph(
            r#"
interface Source {
    type Output
    type Error = string
    function read(self) -> Self.Output throws Self.Error
    function label(self) -> string throws never { "source" }
    function combine(self, other: Self) -> Self throws never { other }
}
class TextSource {
    value: string,
    function local(self, amount: int = 1) -> int throws never { amount }
    implements Source {
        type Output = string
        function read(self) -> string throws never { self.value }
    }
}
"#,
        );
        let class = &graph.concrete_classes[&name("TextSource")];
        let source = class
            .implemented_interfaces
            .iter()
            .find(|view| view.name == name("Source"))
            .unwrap();
        assert_eq!(
            source.associated_types,
            vec![
                (Name::new("Error"), RuntimeTy::string()),
                (Name::new("Output"), RuntimeTy::string()),
            ],
            "the view retains the declared Error pin despite read's narrower effect"
        );
        let read = method(class, "read");
        assert_eq!(
            read.target,
            cg::ConcreteMethodTarget::Interface(source.clone())
        );
        assert!(
            matches!(&read.target, cg::ConcreteMethodTarget::Interface(target) if target.name == name("Source"))
        );
        let RuntimeTy::Function {
            params,
            ret,
            throws,
            ..
        } = &read.signature
        else {
            panic!("function")
        };
        assert_eq!(
            params[0].ty,
            RuntimeTy::Class(name("TextSource"), vec![], TyAttr::default())
        );
        assert_eq!(ret.as_ref(), &RuntimeTy::string());
        assert_eq!(
            throws.as_ref(),
            &RuntimeTy::Never {
                attr: TyAttr::default()
            },
            "keep the provided narrower effect"
        );
        assert!(matches!(
            method(class, "label").target,
            cg::ConcreteMethodTarget::Interface(_)
        ));
        let RuntimeTy::Function { params, ret, .. } = &method(class, "combine").signature else {
            panic!("function")
        };
        assert_eq!(
            params[1].ty,
            RuntimeTy::Class(name("TextSource"), vec![], TyAttr::default())
        );
        assert_eq!(
            ret.as_ref(),
            &RuntimeTy::Class(name("TextSource"), vec![], TyAttr::default())
        );
        let local = method(class, "local");
        assert!(matches!(
            local.target,
            cg::ConcreteMethodTarget::Inherent { .. }
        ));
        let RuntimeTy::Function { params, .. } = &local.signature else {
            panic!("function")
        };
        assert_eq!(params[1].mode, baml_type::FunctionParamMode::Optional);
        assert_eq!(params[0].name.as_ref().map(Name::as_str), Some("self"));
        assert_eq!(params[1].name.as_ref().map(Name::as_str), Some("amount"));
    }

    #[test]
    fn concrete_export_keeps_class_and_default_method_frames_distinct() {
        let graph = graph(
            r#"
interface Source {
    type Output
    function read(self) -> Self.Output throws never
    function echo<T>(self, value: T) -> T throws never { value }
}
class Box<A, T> {
    first: A,
    second: T,
    implements Source {
        type Output = T
        function read(self) -> T throws never { self.second }
    }
}
"#,
        );
        let class = &graph.concrete_classes[&name("Box")];
        let echo = method(class, "echo");
        assert_eq!(echo.generic_params.len(), 1);
        let method_param = &echo.generic_params[0].param;
        let class_param = &class.generic_params[1].param;
        let source = class
            .implemented_interfaces
            .iter()
            .find(|view| view.name == name("Source"))
            .unwrap();
        assert!(matches!(
            &source.associated_types[0].1,
            RuntimeTy::TypeVar(param, _) if param == class_param
        ));
        assert_ne!(method_param, class_param);
        assert_eq!(
            echo.target,
            cg::ConcreteMethodTarget::Interface(source.clone())
        );
        let RuntimeTy::Function { params, ret, .. } = &echo.signature else {
            panic!("function")
        };
        assert!(matches!(ret.as_ref(), RuntimeTy::TypeVar(param, _) if param == method_param));
        assert_eq!(&params[1].ty, ret.as_ref());
        let RuntimeTy::Function { ret, .. } = &method(class, "read").signature else {
            panic!("function")
        };
        assert!(matches!(ret.as_ref(), RuntimeTy::TypeVar(param, _) if param == class_param));
    }

    #[test]
    fn concrete_export_does_not_invent_conditional_or_ambiguous_methods() {
        let graph = graph(
            r#"
interface Named { function label(self) -> string throws never }
interface Extra { function extra(self) -> string throws never { "extra" } }
class Box<T> { function local(self) -> int throws never { 1 } }
implements Extra for Box<string> {}
class Bounded<T extends Named> { function local(self) -> int throws never { 1 } }
implements<T extends Named> Extra for Bounded<T> {}
interface First { function collision(self) -> int throws never { 1 } }
interface Second { function collision(self) -> int throws never { 2 } }
class Both { implements First {} implements Second {} }
"#,
        );
        let unbounded = &graph.concrete_classes[&name("Box")];
        assert!(
            !unbounded
                .implemented_interfaces
                .iter()
                .any(|view| view.name == name("Extra"))
        );
        assert!(
            !unbounded
                .methods
                .iter()
                .any(|method| method.name.as_str() == "extra")
        );
        assert!(
            graph
                .implementations
                .iter()
                .any(|rule| rule.interface.name == name("Extra"))
        );
        let bounded = &graph.concrete_classes[&name("Bounded")];
        assert!(
            bounded
                .implemented_interfaces
                .iter()
                .any(|view| view.name == name("Extra"))
        );
        method(bounded, "extra");
        let both = &graph.concrete_classes[&name("Both")];
        assert_eq!(both.ambiguous_methods[&Name::new("collision")].len(), 2);
        assert!(
            !both
                .methods
                .iter()
                .any(|method| method.name.as_str() == "collision")
        );
    }

    #[test]
    fn concrete_export_preserves_required_views_and_overridden_associated_defaults() {
        let graph = graph(
            r#"
interface Source {
    type Output
    type Error = never
    function read(self) -> Self.Output throws Self.Error
}
interface Child requires Source<Output = string, Error = string> {}
interface Marker {}
class TextSource {
    implements Source {
        type Output = string
        type Error = string
        function read(self) -> string throws string { throw "unavailable" }
    }
    implements Child {}
    implements Marker {}
}
"#,
        );
        let class = &graph.concrete_classes[&name("TextSource")];
        for interface_name in ["Source", "Child", "Marker"] {
            assert!(
                class
                    .implemented_interfaces
                    .iter()
                    .any(|view| view.name == name(interface_name)),
                "missing implemented view {interface_name}"
            );
        }
        let source = class
            .implemented_interfaces
            .iter()
            .find(|view| view.name == name("Source"))
            .unwrap();
        assert_eq!(
            source.associated_types,
            vec![
                (Name::new("Error"), RuntimeTy::string()),
                (Name::new("Output"), RuntimeTy::string()),
            ]
        );
    }
}
