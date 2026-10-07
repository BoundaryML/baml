//! The declared types a walk reads, built once from a program descriptor.

use std::collections::HashMap;

use baml_sdkgen_types::ProgramDescriptor;
use baml_type::{Name, ParamTy, RuntimeFunctionParamTy, RuntimeInterface, RuntimeTy, TypeName};

use crate::error::EncodeErrorKind;

/// Every signature and declaration of one program, in the runtime type family.
#[derive(Debug, Default)]
pub struct Facts {
    functions: HashMap<String, FunctionSig>,
    classes: HashMap<TypeName, ClassFacts>,
    enums: HashMap<TypeName, EnumFacts>,
    aliases: HashMap<TypeName, RuntimeTy>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSig {
    /// Type parameters of the enclosing class, first in De Bruijn order.
    pub class_generic_params: Vec<Name>,
    /// The callable's own type parameters.
    pub generic_params: Vec<Name>,
    pub params: Vec<Param>,
    pub ret: RuntimeTy,
    pub throws: RuntimeTy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub ty: RuntimeTy,
    pub has_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassFacts {
    pub name: TypeName,
    pub generic_params: Vec<Name>,
    /// Field types mention the class's parameters as `TypeVar`s indexed by
    /// position in `generic_params`.
    pub fields: Vec<(String, RuntimeTy)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumFacts {
    pub name: TypeName,
    pub variants: Vec<String>,
}

impl Facts {
    pub fn from_descriptor(descriptor: &ProgramDescriptor) -> Self {
        Self {
            functions: descriptor
                .functions
                .iter()
                .map(|function| {
                    (
                        function.fqn.clone(),
                        FunctionSig {
                            class_generic_params: function.class_generic_params.clone(),
                            generic_params: function.generic_params.clone(),
                            params: function
                                .params
                                .iter()
                                .map(|param| Param {
                                    name: param.name.as_str().to_string(),
                                    ty: RuntimeTy::from(&param.ty),
                                    has_default: param.has_default,
                                })
                                .collect(),
                            ret: RuntimeTy::from(&function.ret),
                            throws: RuntimeTy::from(&function.throws),
                        },
                    )
                })
                .collect(),
            classes: descriptor
                .classes
                .iter()
                .map(|class| {
                    (
                        class.name.clone(),
                        ClassFacts {
                            name: class.name.clone(),
                            generic_params: class.generic_params.clone(),
                            fields: class
                                .fields
                                .iter()
                                .map(|field| {
                                    (field.name.as_str().to_string(), RuntimeTy::from(&field.ty))
                                })
                                .collect(),
                        },
                    )
                })
                .collect(),
            enums: descriptor
                .enums
                .iter()
                .map(|enumeration| {
                    (
                        enumeration.name.clone(),
                        EnumFacts {
                            name: enumeration.name.clone(),
                            variants: enumeration
                                .variants
                                .iter()
                                .map(|variant| variant.as_str().to_string())
                                .collect(),
                        },
                    )
                })
                .collect(),
            aliases: descriptor
                .type_aliases
                .iter()
                .map(|alias| (alias.name.clone(), RuntimeTy::from(&alias.resolves_to)))
                .collect(),
        }
    }

    pub fn function(&self, fqn: &str) -> Option<&FunctionSig> {
        self.functions.get(fqn)
    }

    pub fn class(&self, name: &TypeName) -> Result<&ClassFacts, Box<EncodeErrorKind>> {
        self.classes
            .get(name)
            .ok_or_else(|| Box::new(EncodeErrorKind::UnknownClass(name.clone())))
    }

    pub fn enumeration(&self, name: &TypeName) -> Result<&EnumFacts, Box<EncodeErrorKind>> {
        self.enums
            .get(name)
            .ok_or_else(|| Box::new(EncodeErrorKind::UnknownEnum(name.clone())))
    }

    /// `ty` with alias names expanded until the head is not an alias. One
    /// layer of a recursive alias is expanded per call, so a walk over a finite
    /// value terminates; a chain of aliases that returns to itself is an error.
    pub fn expand<'a>(
        &'a self,
        mut ty: &'a RuntimeTy,
    ) -> Result<&'a RuntimeTy, Box<EncodeErrorKind>> {
        let mut seen: Vec<&TypeName> = Vec::new();
        while let RuntimeTy::TypeAlias(name) = ty {
            if seen.contains(&name) {
                return Err(Box::new(EncodeErrorKind::AliasCycle(name.clone())));
            }
            seen.push(name);
            ty = self
                .aliases
                .get(name)
                .ok_or_else(|| Box::new(EncodeErrorKind::UnknownAlias(name.clone())))?;
        }
        Ok(ty)
    }
}

/// `ty` with every `TypeVar` that `resolve` answers replaced by its answer.
/// A variable `resolve` declines stays a `TypeVar`.
pub(crate) fn substitute(
    ty: &RuntimeTy,
    resolve: &impl Fn(&ParamTy) -> Option<RuntimeTy>,
) -> RuntimeTy {
    let sub = |ty: &RuntimeTy| substitute(ty, resolve);
    let sub_all = |tys: &[RuntimeTy]| tys.iter().map(sub).collect::<Box<[_]>>();
    match ty {
        RuntimeTy::TypeVar(param) => resolve(param).unwrap_or_else(|| ty.clone()),
        RuntimeTy::Int
        | RuntimeTy::Bigint
        | RuntimeTy::Float
        | RuntimeTy::String
        | RuntimeTy::Bool
        | RuntimeTy::Null
        | RuntimeTy::Uint8Array
        | RuntimeTy::Media(_)
        | RuntimeTy::Literal(..)
        | RuntimeTy::Enum(_)
        | RuntimeTy::EnumVariant(..)
        | RuntimeTy::TypeAlias(_)
        | RuntimeTy::RustType
        | RuntimeTy::Type
        | RuntimeTy::Resource
        | RuntimeTy::PromptAst
        | RuntimeTy::Void
        | RuntimeTy::Unknown
        | RuntimeTy::Never => ty.clone(),
        RuntimeTy::Class(name, args) => RuntimeTy::Class(name.clone(), sub_all(args)),
        RuntimeTy::Interface(name, args, bindings) => RuntimeTy::Interface(
            name.clone(),
            sub_all(args),
            bindings
                .iter()
                .map(|(name, ty)| (name.clone(), sub(ty)))
                .collect(),
        ),
        RuntimeTy::List(inner) => RuntimeTy::List(Box::new(sub(inner))),
        RuntimeTy::Map { key, value } => RuntimeTy::Map {
            key: Box::new(sub(key)),
            value: Box::new(sub(value)),
        },
        RuntimeTy::Union(members) => RuntimeTy::Union(sub_all(members)),
        RuntimeTy::Function {
            params,
            ret,
            throws,
        } => RuntimeTy::Function {
            params: params
                .iter()
                .map(|param| RuntimeFunctionParamTy {
                    name: param.name.clone(),
                    ty: sub(&param.ty),
                    mode: param.mode,
                })
                .collect(),
            ret: Box::new(sub(ret)),
            throws: Box::new(sub(throws)),
        },
        RuntimeTy::Future(value, error) => {
            RuntimeTy::Future(Box::new(sub(value)), Box::new(sub(error)))
        }
        RuntimeTy::AssociatedTypeProjection {
            base,
            interface,
            member,
        } => RuntimeTy::AssociatedTypeProjection {
            base: Box::new(sub(base)),
            interface: Box::new(RuntimeInterface::new(
                interface.name.clone(),
                sub_all(&interface.generics),
                interface
                    .associated_types
                    .iter()
                    .map(|(name, ty)| (name.clone(), sub(ty)))
                    .collect(),
            )),
            member: member.clone(),
        },
    }
}

/// Does `ty` mention a type variable anywhere?
pub(crate) fn mentions_type_var(ty: &RuntimeTy) -> bool {
    match ty {
        RuntimeTy::TypeVar(_) => true,
        RuntimeTy::Int
        | RuntimeTy::Bigint
        | RuntimeTy::Float
        | RuntimeTy::String
        | RuntimeTy::Bool
        | RuntimeTy::Null
        | RuntimeTy::Uint8Array
        | RuntimeTy::Media(_)
        | RuntimeTy::Literal(..)
        | RuntimeTy::Enum(_)
        | RuntimeTy::EnumVariant(..)
        | RuntimeTy::TypeAlias(_)
        | RuntimeTy::RustType
        | RuntimeTy::Type
        | RuntimeTy::Resource
        | RuntimeTy::PromptAst
        | RuntimeTy::Void
        | RuntimeTy::Unknown
        | RuntimeTy::Never => false,
        RuntimeTy::Class(_, args) | RuntimeTy::Union(args) => args.iter().any(mentions_type_var),
        RuntimeTy::Interface(_, args, bindings) => {
            args.iter().any(mentions_type_var)
                || bindings.iter().any(|(_, ty)| mentions_type_var(ty))
        }
        RuntimeTy::List(inner) => mentions_type_var(inner),
        RuntimeTy::Map { key, value } => mentions_type_var(key) || mentions_type_var(value),
        RuntimeTy::Function {
            params,
            ret,
            throws,
        } => {
            params.iter().any(|param| mentions_type_var(&param.ty))
                || mentions_type_var(ret)
                || mentions_type_var(throws)
        }
        RuntimeTy::Future(value, error) => mentions_type_var(value) || mentions_type_var(error),
        RuntimeTy::AssociatedTypeProjection {
            base, interface, ..
        } => {
            mentions_type_var(base)
                || interface.generics.iter().any(mentions_type_var)
                || interface
                    .associated_types
                    .iter()
                    .any(|(_, ty)| mentions_type_var(ty))
        }
    }
}
