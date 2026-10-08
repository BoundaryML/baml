//! The declared types a walk reads, built once from a program descriptor.

use std::collections::HashMap;

use baml_sdkgen_types::ProgramDescriptor;
use baml_type::{Name, ParamTy, RuntimeTy, TypeName};

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
    match ty {
        RuntimeTy::TypeVar(param) => resolve(param).unwrap_or_else(|| ty.clone()),
        other => other.map_children(&mut |child| substitute(child, resolve)),
    }
}

/// Solve the class parameters `template` mentions (by De Bruijn index into
/// `solved`) from the concrete type `ty` a value turned out to have. Where the
/// shapes differ nothing is learned; a parameter two fields determine
/// differently is a conflict, reported as its index.
pub(crate) fn unify(
    template: &RuntimeTy,
    ty: &RuntimeTy,
    solved: &mut [Option<RuntimeTy>],
) -> Result<(), usize> {
    match template {
        RuntimeTy::TypeVar(param) => {
            let Some(index) = usize::try_from(param.index())
                .ok()
                .filter(|index| *index < solved.len())
            else {
                return Ok(());
            };
            match &solved[index] {
                None => {
                    solved[index] = Some(ty.clone());
                    Ok(())
                }
                Some(bound) if bound == ty => Ok(()),
                Some(_) => Err(index),
            }
        }
        RuntimeTy::List(inner) => match ty {
            RuntimeTy::List(found) => unify(inner, found, solved),
            _ => Ok(()),
        },
        RuntimeTy::Map { key, value } => match ty {
            RuntimeTy::Map {
                key: found_key,
                value: found_value,
            } => {
                unify(key, found_key, solved)?;
                unify(value, found_value, solved)
            }
            _ => Ok(()),
        },
        RuntimeTy::Class(name, args) => match ty {
            RuntimeTy::Class(found_name, found_args)
                if found_name == name && found_args.len() == args.len() =>
            {
                args.iter()
                    .zip(found_args.iter())
                    .try_for_each(|(arg, found)| unify(arg, found, solved))
            }
            _ => Ok(()),
        },
        RuntimeTy::Function {
            params,
            ret,
            throws,
        } => match ty {
            RuntimeTy::Function {
                params: found_params,
                ret: found_ret,
                throws: found_throws,
            } if found_params.len() == params.len() => {
                params
                    .iter()
                    .zip(found_params.iter())
                    .try_for_each(|(param, found)| unify(&param.ty, &found.ty, solved))?;
                unify(ret, found_ret, solved)?;
                unify(throws, found_throws, solved)
            }
            _ => Ok(()),
        },
        RuntimeTy::Future(value, error) => match ty {
            RuntimeTy::Future(found_value, found_error) => {
                unify(value, found_value, solved)?;
                unify(error, found_error, solved)
            }
            _ => Ok(()),
        },
        // `T?` learns nothing from a null and binds `T` from anything else;
        // a union with several variables is left to the engine.
        RuntimeTy::Union(members) => {
            if members.iter().any(|member| member == ty) {
                return Ok(());
            }
            let mut vars = members
                .iter()
                .filter(|member| matches!(member, RuntimeTy::TypeVar(_)));
            match (vars.next(), vars.next()) {
                (Some(var), None) => unify(var, ty, solved),
                (Some(_), Some(_)) | (None, _) => Ok(()),
            }
        }
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
        | RuntimeTy::Interface(..)
        | RuntimeTy::AssociatedTypeProjection { .. }
        | RuntimeTy::RustType
        | RuntimeTy::Type
        | RuntimeTy::Resource
        | RuntimeTy::PromptAst
        | RuntimeTy::Void
        | RuntimeTy::Unknown
        | RuntimeTy::Never => Ok(()),
    }
}

/// Does `ty` mention a type variable anywhere?
pub(crate) fn mentions_type_var(ty: &RuntimeTy) -> bool {
    if matches!(ty, RuntimeTy::TypeVar(_)) {
        return true;
    }
    let mut found = false;
    ty.visit_children(&mut |child| found = found || mentions_type_var(child));
    found
}
