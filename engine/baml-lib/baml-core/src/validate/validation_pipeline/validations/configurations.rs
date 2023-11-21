use internal_baml_diagnostics::DatamodelError;

use crate::validate::validation_pipeline::context::Context;

pub(super) fn validate(ctx: &mut Context<'_>) {
    for _config in ctx.db.walk_retry_policies() {
        // Nothing to validate.
    }
    for _config in ctx.db.walk_printers() {
        // Nothing to validate.
    }
    for config in ctx.db.walk_test_cases() {
        // Ensure that the test case name is valid.
        let case = config.test_case();
        if ctx.db.find_function_by_name(&case.function.0).is_none() {
            ctx.push_error(DatamodelError::new_type_not_found_error(
                &case.function.0,
                ctx.db.valid_function_names(),
                case.function.1.clone(),
            ));
        }
    }
}
