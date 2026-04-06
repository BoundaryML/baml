use baml_type::TypeName;
use ouroboros::self_referencing;

pub use crate::jsonish::parse;
use crate::sap_model::{AnnotatedTy, TypeRefDb};

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

/// Self-referential struct that owns the [`TypeCtx`] and the [`baml_type::Ty`]
/// for the parse target, and borrows `TypeRefDb` + `AnnotatedTy` from them.
pub struct CompiledSapModel {
    inner: CompiledSapModelInner,
}

impl CompiledSapModel {
    pub fn from_type_ctx(
        type_ctx: sap_model::TypeCtx,
        target: baml_type::Ty,
    ) -> Result<Self, sap_model::ConvertError> {
        let inner = CompiledSapModelInner::try_new(
            type_ctx,
            target,
            sap_model::TypeCtx::build_db,
            sap_model::TypeCtx::convert_ty,
        )?;
        Ok(Self { inner })
    }
    pub fn from_sys_op_context(
        ctx: &::sys_types::SysOpContext,
        target: baml_type::Ty,
    ) -> Result<Self, sap_model::ConvertError> {
        let type_ctx = sap_model::TypeCtx::from_sys_op_context(ctx);
        Self::from_type_ctx(type_ctx, target)
    }

    pub fn db(&self) -> &TypeRefDb<'_, TypeName> {
        self.inner.borrow_db()
    }

    pub fn ty(&self) -> &AnnotatedTy<'_, TypeName> {
        self.inner.borrow_ty()
    }
}

#[self_referencing]
struct CompiledSapModelInner {
    type_ctx: sap_model::TypeCtx,
    /// The target type
    parse_ty: baml_type::Ty,
    #[borrows(type_ctx)]
    #[covariant]
    pub db: TypeRefDb<'this, TypeName>,
    #[borrows(type_ctx, parse_ty)]
    #[covariant]
    pub ty: AnnotatedTy<'this, TypeName>,
}
