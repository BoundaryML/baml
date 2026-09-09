//! Project the compiler's package interfaces once for all SDK backends.

use std::collections::BTreeSet;

use baml_codegen_types as cg;
use baml_compiler2_ast::BuiltinKind;
use baml_compiler2_hir::{compiler2_all_files, file_package::file_package, package};
use baml_compiler2_hir_ty::{
    callable::{ExternalCallTarget, ExternalLinkability},
    package_interface::{self, ExportedFunction, ExportedImplOrigin, ExportedType},
};
use baml_db::ProjectDatabase;
use baml_type::{Interface, ParamTy, RuntimeInterface, RuntimeTy, Ty};

type Result<T> = std::result::Result<T, cg::InterfaceExportError>;

pub(super) fn build(db: &ProjectDatabase) -> Result<cg::InterfaceGraph> {
    let mut graph = cg::InterfaceGraph::default();
    for name in packages(db) {
        let exported =
            package_interface::package_interface(db, package::PackageId::new(db, name.clone()));
        append_package(db, &mut graph, &name, exported)?;
    }
    for (name, callers) in super::interface_callers::build(db, &graph.declarations)? {
        graph.declarations.get_mut(&name).unwrap().callers = callers;
    }
    graph.concrete_classes = super::concrete_export::build(db, &graph.declarations)?;
    Ok(graph)
}

pub(super) fn packages(db: &ProjectDatabase) -> BTreeSet<baml_base::Name> {
    // Include source-less packages as well as files: their PackageInterface is
    // authoritative and may contain implementations with no source class.
    let mut packages: BTreeSet<_> = compiler2_all_files(db)
        .into_iter()
        .map(|file| file_package(db, file).package.clone())
        .collect();
    packages.extend(package::workspace_package_names(db));
    packages.extend(package::external_package_names(db));
    packages
}

pub(super) fn class_projections(
    db: &ProjectDatabase,
) -> std::collections::BTreeMap<cg::Name, baml_type::ClassProjection> {
    let mut projections = std::collections::BTreeMap::new();
    for name in packages(db) {
        let exported = package_interface::package_interface(db, package::PackageId::new(db, name));
        for declaration in exported
            .types
            .values()
            .flat_map(|namespace| namespace.values())
        {
            if let ExportedType::Class {
                qtn,
                boundary_projection,
                ..
            } = declaration
            {
                projections.insert(qtn.clone(), *boundary_projection);
            }
        }
    }
    projections
}

fn ty(value: &Ty, path: &str) -> Result<RuntimeTy> {
    RuntimeTy::try_from(value).map_err(|error| cg::InterfaceExportError {
        path: path.to_owned(),
        reason: error.to_string(),
    })
}

fn interface(value: &Interface, path: &str) -> Result<RuntimeInterface> {
    Ok(RuntimeInterface::new(
        value.name.clone(),
        value
            .generics
            .iter()
            .map(|value| ty(value, path))
            .collect::<Result<_>>()?,
        bindings(&value.associated_types, path)?,
    ))
}

fn bindings(
    values: &[(baml_base::Name, Ty)],
    path: &str,
) -> Result<Vec<(baml_base::Name, RuntimeTy)>> {
    values
        .iter()
        .map(|(name, value)| Ok((name.clone(), ty(value, &format!("{path}.{name}"))?)))
        .collect()
}

fn parameters(
    params: &[ParamTy],
    bounds: &[Vec<Interface>],
    path: &str,
) -> Result<Vec<cg::InterfaceParameter>> {
    if params.len() != bounds.len() {
        return Err(cg::InterfaceExportError {
            path: path.to_owned(),
            reason: "generic parameter and bound counts differ".to_owned(),
        });
    }
    params
        .iter()
        .zip(bounds)
        .map(|(param, bounds)| {
            Ok(cg::InterfaceParameter {
                param: param.clone(),
                bounds: bounds
                    .iter()
                    .map(|bound| interface(bound, path))
                    .collect::<Result<_>>()?,
            })
        })
        .collect()
}

fn method(value: &ExportedFunction, owner: &str) -> Result<cg::InterfaceMethod> {
    let path = format!("{owner}.{}", value.name);
    Ok(cg::InterfaceMethod {
        name: value.name.clone(),
        params: value
            .params
            .iter()
            .map(|param| {
                Ok(baml_type::RuntimeFunctionParamTy {
                    name: param.name.clone(),
                    ty: ty(
                        &param.ty,
                        &format!(
                            "{path} argument {}",
                            param
                                .name
                                .as_ref()
                                .map_or("<unnamed>", baml_base::Name::as_str)
                        ),
                    )?,
                    mode: param.mode,
                })
            })
            .collect::<Result<_>>()?,
        return_type: ty(&value.return_type, &format!("{path} return"))?,
        declared_throws: value
            .declared_throws
            .as_ref()
            .map(|value| ty(value, &format!("{path} declared throws")))
            .transpose()?,
        callable_throws: ty(&value.callable_throws, &format!("{path} throws"))?,
        generic_params: parameters(&value.generic_params, &value.generic_param_bounds, &path)?,
        target: match &value.target {
            ExternalCallTarget::Free {
                package,
                namespace,
                name,
            } => cg::InterfaceMethodTarget::Free(cg::Name::new(
                package.clone(),
                namespace.clone(),
                name.clone(),
            )),
            ExternalCallTarget::Method {
                package,
                namespace,
                class,
                name,
            } => cg::InterfaceMethodTarget::Method {
                class: cg::Name::new(package.clone(), namespace.clone(), class.clone()),
                method: name.clone(),
            },
            ExternalCallTarget::Interface { interface, method } => {
                cg::InterfaceMethodTarget::Interface {
                    interface: interface.clone(),
                    method: method.clone(),
                }
            }
        },
        linkage: match value.linkability {
            ExternalLinkability::Linkable => cg::InterfaceMethodLinkage::Linkable,
            ExternalLinkability::ReservedBuiltin => cg::InterfaceMethodLinkage::ReservedBuiltin,
        },
        builtin: value.builtin_kind.map(|kind| match kind {
            BuiltinKind::Vm => cg::InterfaceBuiltin::Vm,
            BuiltinKind::Io => cg::InterfaceBuiltin::Io,
            BuiltinKind::Intrinsic => cg::InterfaceBuiltin::Intrinsic,
            BuiltinKind::AwaitAny => cg::InterfaceBuiltin::AwaitAny,
        }),
        callability: None,
    })
}

fn append_package(
    db: &ProjectDatabase,
    graph: &mut cg::InterfaceGraph,
    package: &baml_base::Name,
    exported: &package_interface::PackageInterface,
) -> Result<()> {
    for declaration in exported
        .types
        .values()
        .flat_map(|namespace| namespace.values())
    {
        let ExportedType::Interface {
            qtn,
            self_param,
            generic_params,
            param_bounds,
            requires,
            associated_types,
            fields,
            required_methods,
            default_methods,
        } = declaration
        else {
            continue;
        };
        let path = qtn.to_string();
        // Required interfaces are exported transitively. Resolve projections
        // through intermediate requirements in the declaring interface's
        // parameter environment, leaving its own associated types symbolic.
        // Native generators must not reconstruct this implication relation.
        let requirement_facts = baml_compiler2_hir_ty::facts::Facts::with_bounds(
            db,
            generic_params
                .iter()
                .cloned()
                .zip(param_bounds.iter().cloned())
                .chain(std::iter::once((
                    self_param.clone(),
                    vec![Interface::new(
                        qtn.clone(),
                        generic_params
                            .iter()
                            .map(|param| Ty::TypeVar(param.clone(), Default::default()))
                            .collect(),
                        vec![],
                    )],
                )))
                .collect(),
        );
        let required_interface = |value: &Interface| -> Result<RuntimeInterface> {
            let normalized = |value: &Ty| {
                ty(
                    &baml_type::normalize::normalize(value, &requirement_facts),
                    &path,
                )
            };
            Ok(RuntimeInterface::new(
                value.name.clone(),
                value
                    .generics
                    .iter()
                    .map(normalized)
                    .collect::<Result<_>>()?,
                value
                    .associated_types
                    .iter()
                    .map(|(name, value)| Ok((name.clone(), normalized(value)?)))
                    .collect::<Result<_>>()?,
            ))
        };
        let declaration_method = |value: &ExportedFunction| -> Result<cg::InterfaceMethod> {
            let mut exported = method(value, &path)?;
            let has_receiver = value
                .params
                .first()
                .and_then(|param| param.name.as_ref())
                .is_some_and(|name| name.as_str() == "self");
            let target =
                baml_type::interned::InterfaceRef::new(qtn.clone(), Box::new([]), Vec::new());
            let self_restriction =
                baml_compiler2_hir_ty::method_resolution::declared_method_self_restriction(
                    db,
                    &baml_compiler2_hir_ty::facts::Facts::new(db),
                    &target,
                    &value.name,
                );
            exported.callability = Some(if !has_receiver {
                cg::InterfaceCallability::Receiverless
            } else if self_restriction.is_some() {
                cg::InterfaceCallability::ConcreteSelf
            } else {
                cg::InterfaceCallability::Existential
            });
            Ok(exported)
        };
        let declaration = cg::InterfaceDeclaration {
            callers: cg::InterfaceCallers::default(),
            name: qtn.clone(),
            self_param: self_param.clone(),
            generic_params: parameters(generic_params, param_bounds, &path)?,
            requires: requires
                .iter()
                .map(required_interface)
                .collect::<Result<_>>()?,
            associated_types: associated_types
                .iter()
                .map(|value| {
                    Ok(cg::AssociatedType {
                        name: value.name.clone(),
                        bound: value
                            .bound
                            .as_ref()
                            .map(|value| interface(value, &path))
                            .transpose()?,
                        default: value
                            .default
                            .as_ref()
                            .map(|value| ty(value, &path))
                            .transpose()?,
                    })
                })
                .collect::<Result<_>>()?,
            fields: fields
                .iter()
                .map(|(name, value, attrs)| {
                    Ok(cg::InterfaceField {
                        name: name.clone(),
                        ty: ty(value, &format!("{path}.{name}"))?,
                        alias: attrs.alias.clone(),
                        description: attrs.description.clone(),
                        docstring: attrs.docstring.clone(),
                    })
                })
                .collect::<Result<_>>()?,
            required_methods: required_methods
                .iter()
                .map(declaration_method)
                .collect::<Result<_>>()?,
            default_methods: default_methods
                .iter()
                .map(declaration_method)
                .collect::<Result<_>>()?,
        };
        graph.declarations.insert(qtn.clone(), declaration);
    }
    for rule in &exported.impls {
        let path = format!(
            "{package}: {} for {}",
            rule.interface.name, rule.for_ty_pattern
        );
        graph.implementations.push(cg::InterfaceImplementation {
            package: package.clone(),
            interface: interface(&rule.interface, &path)?,
            receiver_pattern: ty(&rule.for_ty_pattern, &format!("{path} receiver"))?,
            generic_params: parameters(&rule.generic_params, &rule.param_bounds, &path)?,
            associated_types: bindings(&rule.associated_types, &path)?,
            field_links: rule.field_links.clone(),
            enclosing_class: match &rule.origin {
                ExportedImplOrigin::InBodyClass { class_qtn } => Some(class_qtn.clone()),
                ExportedImplOrigin::OutOfBody => None,
            },
            methods: rule
                .methods
                .iter()
                .map(|value| method(value, &path))
                .collect::<Result<_>>()?,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use baml_type::{RuntimeTy, TyAttr};

    use super::*;
    use crate::test_support::TestDbExt;

    fn name(package: &str, namespace: &[&str], name: &str) -> cg::Name {
        cg::Name::new(
            baml_base::Name::new(package),
            namespace
                .iter()
                .map(|name| baml_base::Name::new(*name))
                .collect(),
            baml_base::Name::new(name),
        )
    }

    fn assert_projection(ty: &RuntimeTy, owner: &cg::InterfaceDeclaration, member: &str) {
        let RuntimeTy::AssociatedTypeProjection {
            base,
            interface,
            member: actual,
            ..
        } = ty
        else {
            panic!("expected symbolic {member}, got {ty:?}");
        };
        assert!(
            matches!(base.as_ref(), RuntimeTy::TypeVar(param, _) if param == &owner.self_param)
        );
        assert_eq!(interface.name, owner.name);
        assert_eq!(actual.as_str(), member);
    }

    #[test]
    fn interface_export_preserves_record_and_multiple_instantiation_input_views() {
        let root = Path::new("/tmp/interface_export_input_views");
        let mut db = ProjectDatabase::new();
        db.workspace(root);
        db.file(
            &root.join("main.baml"),
            include_str!("../../../../interface_probes/baml/interface_inputs.baml"),
        );
        let diagnostics = baml_db::collect_compiler2_diagnostics(&db);
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let pool = super::super::build_symbol_pool(&db).expect("input views export");
        let record_name = name("user", &[], "TaggedRecord");
        assert_eq!(
            pool.class_projections[&record_name],
            baml_type::ClassProjection::Record
        );
        let record = &pool.interfaces.concrete_classes[&record_name];
        assert!(record.methods.is_empty());
        assert_eq!(
            record
                .implemented_interfaces
                .iter()
                .filter(|view| view.name == name("user", &[], "Tagged"))
                .count(),
            1
        );
        let dual = &pool.interfaces.concrete_classes[&name("user", &[], "DualMapper")];
        let views: Vec<_> = dual
            .implemented_interfaces
            .iter()
            .filter(|view| view.name == name("user", &[], "Mapper"))
            .collect();
        assert_eq!(views.len(), 2);
        for (item, error) in [
            (RuntimeTy::int(), RuntimeTy::string()),
            (
                RuntimeTy::string(),
                RuntimeTy::Never {
                    attr: TyAttr::EMPTY,
                },
            ),
        ] {
            let view = views
                .iter()
                .find(|view| view.generics == vec![item.clone()])
                .expect("instantiated view");
            assert_eq!(
                view.associated_types,
                vec![(baml_base::Name::new("Error"), error)]
            );
        }
        assert!(
            !dual
                .methods
                .iter()
                .any(|method| method.name.as_str() == "map")
        );
        assert!(
            dual.ambiguous_methods
                .contains_key(&baml_base::Name::new("map"))
        );
    }

    #[test]
    fn interface_export_normalizes_transitive_required_associated_bindings() {
        let root = Path::new("/tmp/interface_export_required_views");
        let mut db = ProjectDatabase::new();
        db.workspace(root);
        db.file(
            &root.join("main.baml"),
            include_str!("../../../../interface_probes/baml/required_interfaces.baml"),
        );
        let diagnostics = baml_db::collect_compiler2_diagnostics(&db);
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let pool = super::super::build_symbol_pool(&db).expect("required interfaces export");
        let root = &pool.interfaces.declarations[&name("user", &[], "RequiredRoot")];
        let base = root
            .requires
            .iter()
            .find(|view| view.name == name("user", &[], "RequiredBase"))
            .unwrap();
        assert_eq!(base.generics, vec![RuntimeTy::string()]);
        let output = base
            .associated_types
            .iter()
            .find(|(name, _)| name.as_str() == "Output")
            .unwrap();
        assert!(
            matches!(&output.1, RuntimeTy::TypeVar(param, _) if param == &root.generic_params[0].param)
        );
        let error = base
            .associated_types
            .iter()
            .find(|(name, _)| name.as_str() == "Error")
            .unwrap();
        assert!(matches!(error.1, RuntimeTy::Never { .. }));
        let middle = &pool.interfaces.declarations[&name("user", &[], "RequiredMiddle")];
        let base = middle
            .requires
            .iter()
            .find(|view| view.name == name("user", &[], "RequiredBase"))
            .unwrap();
        for (pin, projection) in [("Output", "Value"), ("Error", "Failure")] {
            assert_projection(
                &base
                    .associated_types
                    .iter()
                    .find(|(name, _)| name.as_str() == pin)
                    .unwrap()
                    .1,
                middle,
                projection,
            );
        }
        let unpinned = &pool.interfaces.declarations[&name("user", &[], "RequiredUnpinned")];
        assert!(
            unpinned
                .requires
                .iter()
                .find(|view| view.name == name("user", &[], "RequiredBase"))
                .unwrap()
                .associated_types
                .is_empty()
        );
    }

    #[test]
    fn interface_export_preserves_compiler_callability() {
        let root = Path::new("/tmp/interface_export_callability");
        let mut db = ProjectDatabase::new();
        db.workspace(root);
        db.file(
            &root.join("main.baml"),
            r#"
interface Operations {
    function label(self) -> string throws never
    function combine(self, other: Self) -> Self throws never
    function make() -> Self throws never
    function default_label(self) -> string throws never { self.label() }
}
"#,
        );
        let diagnostics = baml_db::collect_compiler2_diagnostics(&db);
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let pool = super::super::build_symbol_pool(&db).expect("interface graph exports");
        let declaration = &pool.interfaces.declarations[&name("user", &[], "Operations")];
        let callability = declaration
            .required_methods
            .iter()
            .chain(&declaration.default_methods)
            .map(|m| (m.name.as_str(), m.callability))
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(
            callability["label"],
            Some(cg::InterfaceCallability::Existential)
        );
        assert_eq!(
            callability["default_label"],
            Some(cg::InterfaceCallability::Existential)
        );
        assert_eq!(
            callability["combine"],
            Some(cg::InterfaceCallability::ConcreteSelf)
        );
        assert_eq!(
            callability["make"],
            Some(cg::InterfaceCallability::Receiverless)
        );
    }

    #[test]
    fn class_projection_distinguishes_behavior_from_data_and_factory_methods() {
        use baml_type::ClassProjection::{Builtin, Live, Record};
        let root = Path::new("/tmp/class_projection_owner");
        let mut db = ProjectDatabase::new();
        db.workspace(root);
        db.file(
            &root.join("main.baml"),
            r#"
interface Marker {}
interface Named {
    function label(self) -> string throws never { "name" }
}
interface Inherits requires Named {}
interface Fields { label: string }
interface Blanket {
    function universal(self) -> string throws never
}
implements<T> Blanket for T {
    function universal(self) -> string throws never { "universal" }
}
class Data { value: string }
class Factory {
    value: string,
    function new(value: string) -> Factory throws never { Factory { value } }
}
class Marked { implements Marker {} }
class FieldOnly { label: string, implements Fields {} }
class OwnMethod {
    function label(self) -> string throws never { "own" }
}
class DefaultOnly { implements Named {} }
class Inherited { implements Named {} implements Inherits {} }
class ExternalBody<T> { value: T }
implements<T> Named for ExternalBody<T> {}
class Conditional<T> { value: T }
implements Named for Conditional<int> {}
class ResourceOwner { function cleanup(self) -> void throws never {} }
class Outer { nested: OwnMethod }
implements Named for int {}
"#,
        );
        let diagnostics = baml_db::collect_compiler2_diagnostics(&db);
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let pool = super::super::build_symbol_pool(&db).unwrap();
        for class in ["Data", "Factory", "Marked", "FieldOnly", "Outer"] {
            assert_eq!(
                pool.class_projections[&name("user", &[], class)],
                Record,
                "{class}"
            );
        }
        for class in [
            "OwnMethod",
            "DefaultOnly",
            "Inherited",
            "ExternalBody",
            "Conditional",
            "ResourceOwner",
        ] {
            assert_eq!(
                pool.class_projections[&name("user", &[], class)],
                Live,
                "{class}"
            );
        }
        for class in ["Image", "Audio", "Video", "Pdf"] {
            assert_eq!(
                pool.class_projections[&name("baml", &["media"], class)],
                Builtin
            );
        }
        assert_eq!(pool.class_projections[&name("baml", &[], "Array")], Builtin);
        assert_eq!(pool.class_projections[&name("ai", &[], "Prompt")], Builtin);
        assert_eq!(
            pool.map_symbols(Clone::clone).class_projections,
            pool.class_projections
        );
    }

    #[test]
    fn class_projection_is_stable_when_a_downstream_package_adds_behavior() {
        use baml_type::ClassProjection::{Live, Record};
        let root = Path::new("/tmp/class_projection_downstream");
        let mut db = ProjectDatabase::new();
        db.workspace(root);
        let dependency = db
            .add_source_root(baml_db::SourceRootSpec {
                path: root.join("dependency"),
                package: baml_base::Name::new("shapes"),
                kind: baml_base::SourceRootKind::Dependency,
            })
            .unwrap();
        db.add_or_update_file_in(
            dependency,
            &root.join("dependency/main.baml"),
            r#"
class Point { x: int }
interface Named {
    function label(self) -> string throws never { "name" }
}
"#,
        );
        db.file(
            &root.join("main.baml"),
            r#"
interface Label {
    function label(self) -> string throws never
}
implements Label for shapes.Point {
    function label(self) -> string throws never { "point" }
}
class Local { implements shapes.Named {} }
function view(value: shapes.Point) -> Label throws never { value }
"#,
        );
        let diagnostics = baml_db::collect_compiler2_diagnostics(&db);
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let pool = super::super::build_symbol_pool(&db).unwrap();
        assert_eq!(
            pool.class_projections[&name("shapes", &[], "Point")],
            Record
        );
        assert_eq!(pool.class_projections[&name("user", &[], "Local")], Live);
        assert!(pool.interfaces.implementations.iter().any(|rule| {
            rule.interface.name == name("user", &[], "Label")
                && matches!(&rule.receiver_pattern, RuntimeTy::Class(n, _, _) if n == &name("shapes", &[], "Point"))
        }), "the extension still participates in normal interface dispatch");
    }

    #[test]
    fn interface_export_preserves_shared_fixture_and_stdlib_rules() {
        let root = Path::new("/tmp/interface_export_shared_fixture");
        let mut db = ProjectDatabase::new();
        db.workspace(root);
        db.file(
            &root.join("main.baml"),
            include_str!("../../../../sdk_tests/fixtures/interfaces/baml_src/main.baml"),
        );
        let diagnostics = baml_db::collect_compiler2_diagnostics(&db);
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let pool = super::super::build_symbol_pool(&db).expect("interface graph exports");
        let graph = &pool.interfaces;

        let decoder = &graph.declarations[&name("user", &[], "Decoder")];
        assert_eq!(
            decoder
                .associated_types
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>(),
            ["Output", "Error"]
        );
        assert!(matches!(
            decoder.associated_types[1].default,
            Some(RuntimeTy::Never { .. })
        ));
        let decode = &decoder.required_methods[0];
        assert_projection(&decode.return_type, decoder, "Output");
        assert_projection(&decode.callable_throws, decoder, "Error");
        assert_eq!(
            decode.declared_throws.as_ref(),
            Some(&decode.callable_throws)
        );
        assert_eq!(
            decode.target,
            cg::InterfaceMethodTarget::Interface {
                interface: decoder.name.clone(),
                method: baml_base::Name::new("decode")
            }
        );

        let greeter = &graph.declarations[&name("user", &[], "Greeter")];
        assert_eq!(greeter.required_methods[0].name.as_str(), "greet");
        assert_eq!(greeter.default_methods[0].name.as_str(), "label");
        assert!(
            graph
                .implementations
                .iter()
                .any(|rule| rule.enclosing_class.as_ref()
                    == Some(&name("user", &[], "FriendlyGreeter"))
                    && rule.interface.name == greeter.name)
        );

        let echo = &graph.declarations[&name("user", &[], "Echo")];
        let method = &echo.required_methods[0];
        let param = &method.generic_params[0].param;
        assert_ne!(param.index(), echo.self_param.index());
        assert!(matches!(&method.return_type, RuntimeTy::TypeVar(p, _) if p == param));
        assert_eq!(method.params[1].ty, method.return_type);

        let iterator = &graph.declarations[&name("baml", &["iter"], "Iterator")];
        let iterable = name("baml", &["iter"], "Iterable");
        let required = iterator
            .requires
            .iter()
            .find(|r| r.name == iterable)
            .expect("requires retained");
        for (member, value) in &required.associated_types {
            assert_projection(value, iterator, member.as_str());
        }
        let collect = iterator
            .default_methods
            .iter()
            .find(|m| m.name.as_str() == "collect")
            .unwrap();
        // Exercises callable_throws' owner scope, not only the declared type.
        assert_projection(&collect.callable_throws, iterator, "Error");
        assert_eq!(
            collect.declared_throws.as_ref(),
            Some(&collect.callable_throws)
        );

        let array = graph
            .implementations
            .iter()
            .find(|rule| {
                rule.interface.name == iterable
                    && matches!(rule.receiver_pattern, RuntimeTy::List(..))
            })
            .expect("generic array implementation exported");
        assert!(array.enclosing_class.is_none());
        assert!(!array.generic_params.is_empty());
        let RuntimeTy::List(element, _) = &array.receiver_pattern else {
            unreachable!()
        };
        assert_eq!(
            &array
                .associated_types
                .iter()
                .find(|(n, _)| n.as_str() == "Item")
                .unwrap()
                .1,
            element.as_ref()
        );
        assert!(
            graph
                .implementations
                .iter()
                .any(|rule| rule.interface.name == iterable
                    && matches!(rule.receiver_pattern, RuntimeTy::String { .. }))
        );

        // Native-only rewrites must not discard the semantic graph.
        assert_eq!(pool.map_symbols(Clone::clone).interfaces, *graph);
    }

    #[test]
    fn interface_export_rejects_nested_recovery_states() {
        let invalid = Ty::List(
            Box::new(Ty::Error {
                attr: TyAttr::default(),
            }),
            TyAttr::default(),
        );
        let error = ty(&invalid, "example.Decoder.decode return").unwrap_err();
        assert_eq!(error.path, "example.Decoder.decode return");
        assert!(error.reason.contains("Error"));
    }

    #[test]
    fn interface_export_preserves_bounded_method_frames() {
        let root = Path::new("/tmp/interface_export_bounded_method_frames");
        let mut db = ProjectDatabase::new();
        db.workspace(root);
        db.file(
            &root.join("main.baml"),
            r#"
interface Decoder {
    type Output
    type Error = never
    function decode(self, input: image) -> Self.Output throws Self.Error
}

interface Adapter<T> {
    function apply<U extends Decoder>(self, value: U, extra: T, input: image)
        -> U.Output throws U.Error { value.decode(input) }
}
"#,
        );
        let diagnostics = baml_db::collect_compiler2_diagnostics(&db);
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let graph = build(&db).unwrap();
        let adapter = &graph.declarations[&name("user", &[], "Adapter")];
        let method = &adapter.default_methods[0];
        let outer = &adapter.generic_params[0].param;
        let inner = &method.generic_params[0].param;
        assert_eq!(adapter.self_param.index(), 0);
        assert_eq!(outer.index(), 1);
        assert_eq!(inner.index(), 2);
        assert!(matches!(&method.params[2].ty, RuntimeTy::TypeVar(param, _) if param == outer));
        assert_eq!(
            method.generic_params[0].bounds[0].name,
            name("user", &[], "Decoder")
        );
        for (value, expected_member) in [
            (&method.return_type, "Output"),
            (&method.callable_throws, "Error"),
        ] {
            let RuntimeTy::AssociatedTypeProjection {
                base,
                interface,
                member,
                ..
            } = value
            else {
                panic!("expected a bounded method projection, got {value:?}");
            };
            assert!(matches!(base.as_ref(), RuntimeTy::TypeVar(param, _) if param == inner));
            assert_eq!(interface.name, name("user", &[], "Decoder"));
            assert_eq!(member.as_str(), expected_member);
        }
        assert_eq!(
            method.declared_throws.as_ref(),
            Some(&method.callable_throws)
        );
    }

    #[test]
    fn interface_export_source_less_stdlib_preserves_semantics_and_trusted_linkage() {
        let mut source = ProjectDatabase::new();
        source.ensure_stdlib_sources();
        let mut expected = build(&source).expect("source export");
        let expected_projections = class_projections(&source);
        let blobs = baml_builtins2::stdlib_package_names()
            .iter()
            .map(|name| {
                let id = package::PackageId::new(&source, baml_base::Name::new(*name));
                let exported = package_interface::package_interface(&source, id);
                (
                    (*name).to_owned(),
                    borsh::to_vec(exported).expect("serialize compiler interface"),
                )
            })
            .collect();
        let mut mounted = ProjectDatabase::new();
        mounted.set_precompiled_stdlib_packages(blobs);
        assert!(
            compiler2_all_files(&mounted).is_empty(),
            "the test must not use source fallback"
        );
        // Compiler-built prefixes make VM/IO symbols linkable. Ordinary
        // source declarations reserve them until lowering supplies the ABI;
        // intrinsic/await-any entries must retain their original linkage.
        for method in expected
            .declarations
            .values_mut()
            .flat_map(|decl| {
                decl.required_methods
                    .iter_mut()
                    .chain(&mut decl.default_methods)
            })
            .chain(
                expected
                    .implementations
                    .iter_mut()
                    .flat_map(|rule| &mut rule.methods),
            )
        {
            if matches!(
                method.builtin,
                Some(cg::InterfaceBuiltin::Vm | cg::InterfaceBuiltin::Io)
            ) {
                assert_eq!(method.linkage, cg::InterfaceMethodLinkage::ReservedBuiltin);
                method.linkage = cg::InterfaceMethodLinkage::Linkable;
            }
        }
        let actual = build(&mounted).expect("source-less export");
        assert_eq!(class_projections(&mounted), expected_projections);
        for (name, projection) in &expected_projections {
            assert_eq!(
                baml_compiler2_hir_ty::class_projection::for_name(&mounted, name),
                Some(*projection)
            );
        }
        assert_eq!(actual.declarations.len(), expected.declarations.len());
        assert_eq!(
            actual.concrete_classes.len(),
            expected.concrete_classes.len()
        );
        for (name, declaration) in &expected.concrete_classes {
            let actual = &actual.concrete_classes[name];
            assert_eq!(
                actual.generic_params, declaration.generic_params,
                "class parameters: {name}"
            );
            assert_eq!(
                actual.ambiguous_methods, declaration.ambiguous_methods,
                "ambiguities: {name}"
            );
            assert_eq!(
                actual.implemented_interfaces, declaration.implemented_interfaces,
                "implemented views: {name}"
            );
            assert_eq!(
                actual.methods.len(),
                declaration.methods.len(),
                "method count: {name}"
            );
            for (actual, expected) in actual.methods.iter().zip(&declaration.methods) {
                assert_eq!(
                    actual, expected,
                    "source-less concrete method differs: {name}.{}",
                    expected.name
                );
            }
        }
        for (name, declaration) in &expected.declarations {
            assert!(
                actual.declarations.get(name) == Some(declaration),
                "source-less declaration differs: {name}"
            );
        }
        assert_eq!(actual.implementations.len(), expected.implementations.len());
        for (actual, expected) in actual.implementations.iter().zip(&expected.implementations) {
            assert!(
                actual == expected,
                "source-less rule differs: {} for {}",
                expected.interface.name,
                expected.receiver_pattern
            );
        }
    }
}
