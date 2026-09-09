//! Project compiler-resolved concrete contracts onto the ordinary method IR.
//! Class slots and method slots are rewritten by identity before native names.

use std::collections::BTreeMap;

use baml_codegen_types::{
    Class, ConcreteDeclaration, ConcreteMethodTarget, FunctionArgumentDefault, Ty,
};
use baml_type::{ParamTy, RuntimeTy, Ty as SemanticTy};
use prost::Message;

use crate::{
    emit::{
        function::SyncAsync,
        method::{ConcreteTarget, MethodKind, OptionalArg, PyMethodBinding, RequiredArg},
    },
    names::{BindingRole, PythonNames},
};

pub(crate) fn build(
    declaration: &ConcreteDeclaration,
    class: &Class,
    names: &PythonNames,
) -> Vec<PyMethodBinding> {
    assert_eq!(declaration.generic_params.len(), class.generic_params.len());
    declaration
        .methods
        .iter()
        .map(|method| {
            let fqn = format!("{}.{}", declaration.name, method.name);
            let mut parameters: BTreeMap<ParamTy, String> = declaration
                .generic_params
                .iter()
                .zip(&class.generic_params)
                .map(|(p, raw)| {
                    (
                        p.param.clone(),
                        names
                            .generic(&declaration.name.to_string(), raw.as_str())
                            .into_owned(),
                    )
                })
                .collect();
            let generic_params: Vec<_> = method
                .generic_params
                .iter()
                .enumerate()
                .map(|(index, p)| {
                    let native = names.concrete_generic(&fqn, index).to_owned();
                    parameters.insert(p.param.clone(), native.clone());
                    native
                })
                .collect();
            let project = |ty: &RuntimeTy| {
                let semantic = SemanticTy::from(ty.clone());
                let rewritten = baml_type::unify::rewrite_ty(&semantic, &mut |node| match node {
                    SemanticTy::TypeVar(p, attr) => parameters.get(p).map(|native| {
                        SemanticTy::TypeVar(
                            ParamTy::new(p.index(), baml_base::Name::new(native)),
                            attr.clone(),
                        )
                    }),
                    _ => None,
                });
                Ty::try_from(&rewritten).unwrap_or_else(|error| {
                    panic!("cannot generate Python concrete caller {fqn}: {error}")
                })
            };
            let RuntimeTy::Function {
                params,
                ret,
                throws,
                ..
            } = &method.signature
            else {
                panic!("concrete method {fqn} is not callable");
            };
            let mut required_args = vec![];
            let mut optional_args = vec![];
            for (index, arg) in params.iter().skip(1).enumerate() {
                let wire_name = arg
                    .name
                    .as_ref()
                    .map_or_else(|| format!("arg{index}"), ToString::to_string);
                let native = names.param(&fqn, &wire_name).into_owned();
                if arg.mode == baml_type::FunctionParamMode::Optional {
                    optional_args.push(OptionalArg {
                        name: native,
                        wire_name,
                        ty: project(&arg.ty),
                        default: FunctionArgumentDefault::Expression { source: None },
                    });
                } else {
                    required_args.push(RequiredArg {
                        name: native,
                        wire_name,
                        ty: project(&arg.ty),
                    });
                }
            }
            let pattern = match &method.target {
                ConcreteMethodTarget::Inherent { class } => {
                    assert_eq!(
                        class, &declaration.name,
                        "inherent method belongs to another declaration"
                    );
                    None
                }
                ConcreteMethodTarget::Interface(interface) => {
                    let ty = RuntimeTy::Interface(
                        interface.name.clone(),
                        interface.generics.clone(),
                        interface.associated_types.clone(),
                        baml_base::TyAttr::EMPTY,
                    );
                    Some(bridge_ctypes::runtime_ty_to_proto_ty(&ty).encode_to_vec())
                }
            };
            let errors = project(throws);
            PyMethodBinding {
                py_name: names.callable(&fqn, BindingRole::DirectSync).into_owned(),
                baml_fqn: fqn.clone(),
                concrete_target: Some(ConcreteTarget {
                    class_name: declaration.name.to_string(),
                    member: method.name.to_string(),
                    interface_pattern: pattern,
                }),
                mode: SyncAsync::Async,
                required_args,
                optional_args,
                kind: MethodKind::Instance,
                return_ty: project(ret),
                generic_params,
                wire_generic_params: method
                    .generic_params
                    .iter()
                    .map(|p| p.param.as_str().to_owned())
                    .collect(),
                type_var_names: BTreeMap::new(),
                docstring: None,
                raises_names: crate::emit::collect_raises_names(Some(&errors), names),
            }
        })
        .collect()
}

/// Native proof signatures retain the complete compiler-proven view. They do
/// not re-check conformance by comparing the concrete method's parameter names.
pub(crate) fn input_proofs(
    declaration: &ConcreteDeclaration,
    class: &Class,
    names: &PythonNames,
) -> Vec<(String, Ty)> {
    let parameters: BTreeMap<_, _> = declaration
        .generic_params
        .iter()
        .zip(&class.generic_params)
        .map(|(p, raw)| {
            (
                p.param.clone(),
                names
                    .generic(&declaration.name.to_string(), raw.as_str())
                    .into_owned(),
            )
        })
        .collect();
    declaration
        .implemented_interfaces
        .iter()
        .map(|view| {
            let mut args = view.generics.clone();
            for binding in names
                .interface_associated_names(&view.name)
                .expect("exported interface has an associated order")
            {
                args.push(
                    view.associated_types
                        .iter()
                        .find(|(name, _)| name == binding)
                        .expect("complete interface view")
                        .1
                        .clone(),
                );
            }
            let ty = RuntimeTy::Class(
                names.interface_witness_name(&view.name),
                args,
                baml_base::TyAttr::EMPTY,
            );
            let rewritten =
                baml_type::unify::rewrite_ty(&SemanticTy::from(ty), &mut |node| match node {
                    SemanticTy::TypeVar(param, attr) => parameters.get(param).map(|name| {
                        SemanticTy::TypeVar(
                            ParamTy::new(param.index(), baml_base::Name::new(name)),
                            attr.clone(),
                        )
                    }),
                    _ => None,
                });
            (
                PythonNames::interface_proof(&view.name),
                Ty::try_from(&rewritten).unwrap_or_else(|error| {
                    panic!(
                        "cannot project input proof {} for {}: {error}",
                        view.name, declaration.name
                    )
                }),
            )
        })
        .collect()
}
