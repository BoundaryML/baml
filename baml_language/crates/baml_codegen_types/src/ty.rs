//! Shared BAML types exposed to every code generator.
//!
//! The representation itself belongs to `baml_type`: generators consume the
//! same compiler-owned qualified names and the same structurally narrowed type
//! family instead of maintaining a parallel enum with lossy conversions.

use baml_base::Literal;
pub use baml_type::{
    CodegenFunctionParamTy as CallableParam, CodegenTy as Ty, Freshness,
    FunctionParamMode as CodegenFunctionParamMode, QualifiedTypeName as Name,
};

use crate::CodegenTypeError;

pub(crate) fn validate_ty(ty: &Ty) -> Result<(), CodegenTypeError> {
    match ty {
        Ty::Int { .. }
        | Ty::Bigint { .. }
        | Ty::Float { .. }
        | Ty::String { .. }
        | Ty::Bool { .. }
        | Ty::Null { .. }
        | Ty::Uint8Array { .. }
        | Ty::Media(..)
        | Ty::Literal(..)
        | Ty::Enum(..)
        | Ty::EnumVariant(..)
        | Ty::TypeAlias(..)
        | Ty::TypeVar(..)
        | Ty::RustType { .. }
        | Ty::Type { .. }
        | Ty::Resource { .. }
        | Ty::PromptAst { .. }
        | Ty::BuiltinUnknown { .. }
        | Ty::Never { .. }
        | Ty::Void { .. } => Ok(()),
        Ty::Class(_, args, _) => args.iter().try_for_each(validate_ty),
        Ty::Interface(_, generics, associated_types, _) => {
            generics.iter().try_for_each(validate_ty)?;
            associated_types
                .iter()
                .try_for_each(|(_, ty)| validate_ty(ty))
        }
        Ty::List(inner, _) => {
            if matches!(inner.as_ref(), Ty::Void { .. }) {
                return Err(CodegenTypeError::InvalidUnit(inner.clone()));
            }
            validate_ty(inner)
        }
        Ty::Map { key, value, .. } => {
            if !is_potential_map_key(key) {
                return Err(CodegenTypeError::InvalidMapKey(key.clone()));
            }
            validate_ty(key)?;
            if matches!(value.as_ref(), Ty::Void { .. }) {
                return Err(CodegenTypeError::InvalidUnit(value.clone()));
            }
            validate_ty(value)
        }
        Ty::Union(items, _) => {
            if items.is_empty() {
                return Err(CodegenTypeError::InvalidUnionUsage(Box::new(ty.clone())));
            }
            items.iter().try_for_each(validate_ty)?;
            let null_count = items
                .iter()
                .filter(|ty| matches!(ty, Ty::Null { .. }))
                .count();
            if null_count > 1
                || items
                    .iter()
                    .any(|ty| matches!(ty, Ty::Union(..) | Ty::Void { .. }))
            {
                Err(CodegenTypeError::InvalidUnionUsage(Box::new(ty.clone())))
            } else {
                Ok(())
            }
        }
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            params.iter().try_for_each(|param| validate_ty(&param.ty))?;
            validate_ty(ret)?;
            validate_ty(throws)
        }
        Ty::Future(value, error, _) => {
            validate_ty(value)?;
            validate_ty(error)
        }
    }
}

/// Structural pre-check used before a symbol pool is available. Alias nodes
/// are candidates here; pool validation follows the declarations and proves
/// that their resolved targets are string-denoting.
fn is_potential_map_key(ty: &Ty) -> bool {
    match ty {
        Ty::String { .. }
        | Ty::Literal(Literal::String(_), ..)
        | Ty::TypeAlias(..)
        | Ty::Never { .. } => true,
        Ty::Union(members, _) => members.iter().all(is_potential_map_key),
        _ => false,
    }
}
