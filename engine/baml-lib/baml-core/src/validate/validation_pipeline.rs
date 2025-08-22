mod context;
pub mod validations;

use internal_baml_parser_database::ParserDatabase;

use crate::{configuration::Configuration, internal_baml_diagnostics::Diagnostics};

/// Validate a Prisma schema.
pub(crate) fn validate(
    db: &ParserDatabase,
    configuration: &Configuration,
    diagnostics: &mut Diagnostics,
) {
    // Early return so that the validator does not have to deal with invalid schemas

    let mut context = context::Context::new(
        db,
        configuration.preview_features(),
        configuration,
        diagnostics,
    );

    validations::validate(&mut context);
}
