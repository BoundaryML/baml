use crate::PreviewFeature;
use enumflags2::BitFlags;
use internal_baml_diagnostics::{DatamodelError, DatamodelWarning, Diagnostics};

/// The validation context. The lifetime parameter is _not_ the AST lifetime, but the subtype of
/// all relevant lifetimes. No data escapes for validations, so the context only need to be valid
/// for the duration of validations.
pub(crate) struct Context<'a> {
    pub(super) db: &'a internal_baml_parser_database::ParserDatabase,
    pub(super) preview_features: BitFlags<PreviewFeature>,
    pub(super) diagnostics: &'a mut Diagnostics,
}

impl Context<'_> {
    /// Pure convenience method. Forwards to internal_baml_diagnostics::push_error().
    pub(super) fn push_error(&mut self, error: DatamodelError) {
        self.diagnostics.push_error(error);
    }

    /// Pure convenience method. Forwards to internal_baml_diagnostics::push_warning().
    pub(super) fn push_warning(&mut self, warning: DatamodelWarning) {
        self.diagnostics.push_warning(warning);
    }
}
