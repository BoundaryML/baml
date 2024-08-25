use internal_baml_diagnostics::DatamodelError;

use crate::{coerce, context::Context, types::DynamicStringAttributes};

pub(super) fn visit_get_attribute(attributes: &mut DynamicStringAttributes, ctx: &mut Context<'_>) {
    while let Ok((_, expr)) = ctx.visit_default_arg_with_idx("code") {
        match coerce::raw_string(expr, ctx.diagnostics) {
            Some(code) => {
                match &code.language {
                    Some((language, span)) => {
                        if !attributes.add_code(
                            ctx.interner.intern(language),
                            ctx.interner.intern(code.value()),
                        ) {
                            ctx.push_error(DatamodelError::new_validation_error(
                                "Duplicate language.",
                                span.clone(),
                            ));
                        }
                    }
                    None => {
                        ctx.push_error(DatamodelError::new_validation_error(
                            "All getters args must specify a language. Example: python#\"...\"#. instead of #\"...\"#",
                            expr.span().clone(),
                        ));
                        continue;
                    }
                };
            }
            None => continue,
        };
    }
}
