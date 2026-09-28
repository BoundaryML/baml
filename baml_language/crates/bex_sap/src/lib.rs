use sys_types::DefKey;

pub use crate::jsonish::parse;
use crate::sap_model::{Ty, TypeRefDb};

pub mod baml_value;
pub mod deserializer;
pub mod jsonish;
pub mod sap_model;
#[cfg(test)]
mod tests;
pub mod to_external;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamingMode {
    NonStreaming,
    Streaming,
}

/// Self-referential struct that owns the [`sap_model::TypeCtx`] and the [`sys_types::SapTy`]
/// for the parse target, and borrows `TypeRefDb` + `Ty` from them.
///
/// Partial and final parses share the one target type.
pub struct CompiledSapModel {
    inner: CompiledSapModelInner,
}

impl CompiledSapModel {
    pub fn from_type_ctx(
        type_ctx: sap_model::TypeCtx,
        target: sys_types::SapTy,
    ) -> Result<Self, sap_model::ConvertError> {
        // A generic `T` can materialize as a union after the context's declared
        // types were simplified. Normalize the runtime target at the SAP
        // boundary so the converter always receives its required flat form.
        let target = type_ctx.normalize_parse_target(target);
        let owner = SapModelOwner {
            type_ctx,
            parse_ty: target,
        };
        let inner = CompiledSapModelInner::try_new(owner, |owner| {
            Ok::<_, sap_model::ConvertError>(SapModelViews {
                db: owner.type_ctx.build_db()?,
                ty: owner.type_ctx.convert_ty(&owner.parse_ty)?,
            })
        })?;
        Ok(Self { inner })
    }
    pub fn from_sys_op_context(
        ctx: &::sys_types::SysOpContext,
        target: sys_types::SapTy,
    ) -> Result<Self, sap_model::ConvertError> {
        let type_ctx = sap_model::TypeCtx::from_sys_op_context(ctx);
        Self::from_type_ctx(type_ctx, target)
    }

    pub fn db(&self) -> &TypeRefDb<'_, DefKey> {
        &self.inner.borrow_dependent().db
    }

    pub fn ty(&self) -> &Ty<'_, DefKey> {
        &self.inner.borrow_dependent().ty
    }
}

/// What [`CompiledSapModel`] owns.
struct SapModelOwner {
    type_ctx: sap_model::TypeCtx,
    /// The target type
    parse_ty: sys_types::SapTy,
}

/// What [`CompiledSapModel`] borrows from its [`SapModelOwner`].
struct SapModelViews<'a> {
    db: TypeRefDb<'a, DefKey>,
    ty: Ty<'a, DefKey>,
}

self_cell::self_cell!(
    struct CompiledSapModelInner {
        owner: SapModelOwner,
        #[covariant]
        dependent: SapModelViews,
    }
);
