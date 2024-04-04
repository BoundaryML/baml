use internal_baml_diagnostics::DatamodelError;
use internal_baml_schema_ast::ast::{WithName, WithSpan};

use crate::validate::validation_pipeline::context::Context;

use super::common::validate_type_exists;

pub(super) fn validate(ctx: &mut Context<'_>) {
    for func in ctx.db.walk_old_functions() {
        for args in func.walk_input_args().chain(func.walk_output_args()) {
            let arg = args.ast_arg();
            validate_type_exists(ctx, &arg.1.field_type)
        }

        // Check if the function has multiple impls, if it does,
        // we require an impl.
        match &func.metadata().default_impl {
            Some((default_impl, span)) => {
                if !func
                    .walk_variants()
                    .find(|v| v.name() == default_impl)
                    .is_some()
                {
                    ctx.push_error(DatamodelError::new_impl_not_found_error(
                        &default_impl,
                        func.walk_variants()
                            .map(|v| v.name().to_string())
                            .collect::<Vec<_>>(),
                        span.clone(),
                    ))
                }
            }
            None => {
                let num_impls = func.walk_variants().len();
                if num_impls >= 2 {
                    ctx.push_error(DatamodelError::new_validation_error(
                        &format!(
                            "{} has multiple impls({}). Add a `default_impl your_impl` field to the function",
                            func.name(),
                            num_impls
                        ),
                        func.identifier().span().clone(),
                    ))
                }
            }
        }
    }

    let clients = ctx
        .db
        .walk_clients()
        .map(|c| c.name().to_string())
        .collect::<Vec<_>>();
    for func in ctx.db.walk_new_functions() {
        for args in func.walk_input_args().chain(func.walk_output_args()) {
            let arg = args.ast_arg();
            validate_type_exists(ctx, &arg.1.field_type)
        }

        // Ensure the client is correct.
        match func.client() {
            Some(_) => {}
            None => {
                let client = func.metadata().client.as_ref().unwrap();
                ctx.push_error(DatamodelError::not_found_error(
                    "Client",
                    &client.0,
                    client.1.clone(),
                    clients.clone(),
                ))
            }
        }

        // TODO: @sxlijin do any validation on the prompt.
        let _prompt = func.jinja_prompt();
    }
}
