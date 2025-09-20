use internal_baml_ast::ast::{Identifier, WithName, WithSpan};
use internal_baml_diagnostics::DatamodelError;

use crate::validate::validation_pipeline::{
    context::Context, validations::reserved_names::baml_keywords,
};

pub(super) fn validate_identifier_not_keyword(ctx: &mut Context<'_>, identifier: &Identifier) {
    if baml_keywords().contains(identifier.name()) {
        ctx.push_error(DatamodelError::new_validation_error(
            &format!("'{}' is a reserved keyword.", identifier.name()),
            identifier.span().clone(),
        ));
    }
}
