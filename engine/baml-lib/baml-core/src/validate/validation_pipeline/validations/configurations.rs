use internal_baml_diagnostics::{DatamodelError, DatamodelWarning};

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

        // Check for duplicate function names.
        let mut function_names = std::collections::HashSet::new();
        for (name, s) in &case.functions {
            if !function_names.insert(name) {
                ctx.push_error(DatamodelError::new_duplicate_function_errors(
                    name,
                    s.clone(),
                ));
            }
        }

        case.functions
            .iter()
            .for_each(|(name, s)| match ctx.db.find_function_by_name(name) {
                Some(f) => {
                    // TODO: Check args.
                }
                None => {
                    ctx.push_warning(DatamodelWarning::new_type_not_found_error(
                        name,
                        ctx.db.valid_function_names(),
                        s.clone(),
                    ));
                }
            });
    }
}
