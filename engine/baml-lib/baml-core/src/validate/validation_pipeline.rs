mod context;
pub mod validations;

use enumflags2::BitFlags;
use internal_baml_parser_database::ParserDatabase;

use crate::{feature_flags::FeatureFlags, internal_baml_diagnostics::Diagnostics, PreviewFeature};

/// Validate a Prisma schema.
pub(crate) fn validate(
    db: &ParserDatabase,
    preview_features: BitFlags<PreviewFeature>,
    feature_flags: FeatureFlags,
    diagnostics: &mut Diagnostics,
) {
    // Early return so that the validator does not have to deal with invalid schemas

    let mut context = context::Context::new(db, preview_features, feature_flags, diagnostics);

    validations::validate(&mut context);
}
