use baml_types::TypeIR;
use enumflags2::BitFlags;
use internal_baml_diagnostics::{DatamodelError, DatamodelWarning, Diagnostics};

use crate::{configuration::Configuration, feature_flags::FeatureFlags, PreviewFeature};

/// The validation context. The lifetime parameter is _not_ the AST lifetime, but the subtype of
/// all relevant lifetimes. No data escapes for validations, so the context only need to be valid
/// for the duration of validations.
pub(crate) struct Context<'a> {
    pub(super) db: &'a internal_baml_parser_database::ParserDatabase,
    #[allow(dead_code)]
    pub(super) preview_features: BitFlags<PreviewFeature>,
    pub(super) diagnostics: &'a mut Diagnostics,
    pub(super) configuration: &'a Configuration,
}

impl<'a> Context<'a> {
    pub(crate) fn new(
        db: &'a internal_baml_parser_database::ParserDatabase,
        preview_features: BitFlags<PreviewFeature>,
        configuration: &'a Configuration,
        diagnostics: &'a mut Diagnostics,
    ) -> Self {
        Self {
            db,
            preview_features,
            configuration,
            diagnostics,
        }
    }

    pub fn feature_flags(&self) -> &FeatureFlags {
        self.configuration.feature_flags()
    }

    /// Pure convenience method. Forwards to internal_baml_diagnostics::push_error().
    pub(super) fn push_error(&mut self, error: DatamodelError) {
        self.diagnostics.push_error(error);
    }

    /// Pure convenience method. Forwards to internal_baml_diagnostics::push_warning().
    pub(super) fn push_warning(&mut self, warning: DatamodelWarning) {
        self.diagnostics.push_warning(warning);
    }
}
