mod context;
pub mod validations;

use enumflags2::BitFlags;
use internal_baml_parser_database::ParserDatabase;

use crate::{internal_baml_diagnostics::Diagnostics, PreviewFeature};

/// Validate a Prisma schema.
pub(crate) fn validate(
    db: &ParserDatabase,
    preview_features: BitFlags<PreviewFeature>,
    diagnostics: &mut Diagnostics,
) {
    // Early return so that the validator does not have to deal with invalid schemas

    let mut context = context::Context {
        db,
        preview_features,
        diagnostics,
    };

    validations::validate(&mut context);
}
