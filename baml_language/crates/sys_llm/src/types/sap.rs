use ::baml_type::TypeName;
use ::bex_sap::sap_model;

/// Contains all the information needed to run SAP parsing on a stream (or oneshot),
/// with the ability to cache data for streaming.
pub struct SapStreamCache {
    types: bex_sap::CompiledSapModel,
}

impl SapStreamCache {
    pub fn new(types: bex_sap::CompiledSapModel) -> Self {
        Self { types }
    }

    pub fn db(&self) -> &sap_model::TypeRefDb<'_, TypeName> {
        self.types.db()
    }

    pub fn ty(&self) -> &sap_model::AnnotatedTy<'_, TypeName> {
        self.types.ty()
    }

    pub fn ty_resolved(
        &self,
    ) -> Result<
        sap_model::TyWithMeta<
            sap_model::TyResolvedRef<'_, TypeName>,
            &sap_model::TypeAnnotations<'_, TypeName>,
        >,
        &TypeName,
    > {
        self.db().resolve_with_meta(self.ty().as_ref())
    }

    pub fn stream_ty_resolved(
        &self,
    ) -> Result<
        sap_model::TyWithMeta<
            sap_model::TyResolvedRef<'_, TypeName>,
            &sap_model::TypeAnnotations<'_, TypeName>,
        >,
        &TypeName,
    > {
        self.db().resolve_with_meta(self.types.stream_ty().as_ref())
    }
}
