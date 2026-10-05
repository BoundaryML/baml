use ouroboros::self_referencing;
use sys_types::DefKey;

pub use crate::jsonish::parse;
use crate::{
    deserializer::coercer::{ParsingContext, ParsingError},
    sap_model::{Ty, TyResolvedRef, TypeRefDb},
};

pub mod baml_value;
pub mod deserializer;
pub mod jsonish;
pub mod sap_model;
#[cfg(test)]
mod tests;
pub mod to_baml_ty;

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
        let inner = CompiledSapModelInner::try_new(
            type_ctx,
            target,
            sap_model::TypeCtx::build_db,
            sap_model::TypeCtx::convert_ty,
        )?;
        Ok(Self { inner })
    }
    pub fn db(&self) -> &TypeRefDb<'_, DefKey> {
        self.inner.borrow_db()
    }

    pub fn ty(&self) -> &Ty<'_, DefKey> {
        self.inner.borrow_ty()
    }

    /// The parse target, resolved in [`Self::db`].
    ///
    /// # Errors
    ///
    /// The target names a type the database does not hold: a schema the
    /// converter accepted but cannot resolve, which no input text can fix.
    pub fn resolved_target(&self) -> Result<TyResolvedRef<'_, DefKey>, ParsingError> {
        self.db()
            .resolve(self.ty())
            .map_err(|name| ParsingContext::new(self.db()).error_type_resolution(name))
    }
}

#[self_referencing]
struct CompiledSapModelInner {
    pub type_ctx: sap_model::TypeCtx,
    /// The target type
    pub parse_ty: sys_types::SapTy,
    #[borrows(type_ctx)]
    #[covariant]
    pub db: TypeRefDb<'this, DefKey>,
    #[borrows(type_ctx, parse_ty)]
    #[covariant]
    pub ty: Ty<'this, DefKey>,
}
