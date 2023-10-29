mod context;
mod validations;

use crate::{internal_baml_diagnostics::Diagnostics, PreviewFeature};
use enumflags2::BitFlags;
use internal_baml_parser_database::ParserDatabase;

/// Validate a Prisma schema.
pub(crate) fn validate(
    db: &ParserDatabase,
    preview_features: BitFlags<PreviewFeature>,
    mut diagnostics: &mut Diagnostics,
) {
    // Early return so that the validator does not have to deal with invalid schemas

    let mut context = context::Context {
        db: &db,
        preview_features,
        diagnostics: &mut diagnostics,
    };

    validations::validate(&mut context);
}
