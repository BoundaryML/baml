mod context;
mod validations;

use crate::{configuration, internal_baml_diagnostics::Diagnostics, PreviewFeature};
use enumflags2::BitFlags;
use internal_baml_parser_database::ParserDatabase;

pub struct ValidateOutput {
    pub(crate) db: ParserDatabase,
    pub(crate) diagnostics: Diagnostics,
}

/// Validate a Prisma schema.
pub(crate) fn validate(
    db: ParserDatabase,
    preview_features: BitFlags<PreviewFeature>,
    diagnostics: Diagnostics,
) -> ValidateOutput {
    let mut output = ValidateOutput { db, diagnostics };

    // Early return so that the validator does not have to deal with invalid schemas
    if !output.diagnostics.errors().is_empty() {
        return output;
    }

    let mut context = context::Context {
        db: &output.db,
        preview_features,
        diagnostics: &mut output.diagnostics,
    };

    validations::validate(&mut context);

    output
}
