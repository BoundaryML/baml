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

/// Self-referential struct that owns the [`sap_model::TypeCtx`] and the [`baml_type::RuntimeTy`]
/// for the parse target, and borrows `TypeRefDb` + `AnnotatedTy` from them.
pub struct CompiledSapModel {
    inner: CompiledSapModelInner,
}

impl CompiledSapModel {
    pub fn from_type_ctx(
        type_ctx: sap_model::TypeCtx,
        target: baml_type::RuntimeTy,
        stream_target: baml_type::RuntimeTy,
    ) -> Result<Self, sap_model::ConvertError> {
        // A generic `T` can materialize as a union after the context's declared
        // types were simplified. Normalize both runtime targets at the SAP
        // boundary so the converter always receives its required flat form.
        let target = type_ctx.normalize_parse_target(target);
        let stream_target = type_ctx.normalize_parse_target(stream_target);
        let inner = CompiledSapModelInner::try_new(
            type_ctx,
            target,
            stream_target,
            sap_model::TypeCtx::build_db,
            sap_model::TypeCtx::convert_ty,
            sap_model::TypeCtx::convert_ty,
        )?;
        Ok(Self { inner })
    }
    pub fn from_sys_op_context(
        ctx: &::sys_types::SysOpContext,
        target: baml_type::RuntimeTy,
        stream_target: baml_type::RuntimeTy,
    ) -> Result<Self, sap_model::ConvertError> {
        let type_ctx = sap_model::TypeCtx::from_sys_op_context(ctx);
        Self::from_type_ctx(type_ctx, target, stream_target)
    }

    pub fn db(&self) -> &TypeRefDb<'_, TypeName> {
        self.inner.borrow_db()
    }

    pub fn ty(&self) -> &AnnotatedTy<'_, TypeName> {
        self.inner.borrow_ty()
    }

    pub fn stream_ty(&self) -> &AnnotatedTy<'_, TypeName> {
        self.inner.borrow_stream_ty()
    }
}

#[self_referencing]
struct CompiledSapModelInner {
    pub type_ctx: sap_model::TypeCtx,
    /// The target type
    pub parse_ty: baml_type::RuntimeTy,
    pub parse_stream_ty: baml_type::RuntimeTy,
    #[borrows(type_ctx)]
    #[covariant]
    pub db: TypeRefDb<'this, TypeName>,
    #[borrows(type_ctx, parse_ty)]
    #[covariant]
    pub ty: AnnotatedTy<'this, TypeName>,
    #[borrows(type_ctx, parse_stream_ty)]
    #[covariant]
    pub stream_ty: AnnotatedTy<'this, TypeName>,
}
