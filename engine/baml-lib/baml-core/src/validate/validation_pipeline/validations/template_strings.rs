use std::collections::HashSet;

use internal_baml_ast::ast::{FieldType, TypeExpId, WithIdentifier, WithName, WithSpan};
use internal_baml_diagnostics::{DatamodelError, DatamodelWarning, Span};

use super::types::validate_type;
use crate::validate::validation_pipeline::context::Context;

pub(super) fn validate(ctx: &mut Context<'_>) {
    let mut defined_types = internal_baml_jinja_types::PredefinedTypes::default(
        internal_baml_jinja_types::JinjaContext::Prompt,
    );
    ctx.db.walk_classes().for_each(|t| {
        t.add_to_types(&mut defined_types);
    });
    ctx.db.walk_enums().for_each(|t| {
        t.add_enums_to_types(&mut defined_types);
    });
    ctx.db.walk_templates().for_each(|t| {
        t.add_to_types(&mut defined_types);
    });
    ctx.db.walk_type_aliases().for_each(|t| {
        t.add_to_types(&mut defined_types);
    });

    for template in ctx.db.walk_templates() {
        for args in template.walk_input_args() {
            let arg = args.ast_arg();
            validate_type(ctx, &arg.1.field_type);
        }

        for args in template.walk_input_args() {
            let arg = args.ast_arg();
            let field_type = &arg.1.field_type;

            let span = field_type.span().clone();
            if super::functions::has_checks_nested(ctx, field_type) {
                ctx.push_error(DatamodelError::new_validation_error(
                    "Types with checks are not allowed as function parameters.",
                    span,
                ));
            }
        }

        let prompt = match template.template_raw() {
            Some(p) => p,
            None => {
                ctx.push_error(DatamodelError::new_validation_error(
                    "Template string must be a raw string literal like `template_string MyTemplate(myArg: string) #\"\n\n\"#`",
                    template.identifier().span().clone(),
                ));
                continue;
            }
        };

        defined_types.start_scope();

        template.walk_input_args().for_each(|arg| {
            let name = match arg.ast_arg().0 {
                Some(arg) => arg.name().to_string(),
                None => {
                    ctx.push_error(DatamodelError::new_validation_error(
                        "Argument name is missing.",
                        arg.ast_arg().1.span().clone(),
                    ));
                    return;
                }
            };

            let field_type = ctx.db.to_jinja_type(&arg.ast_arg().1.field_type);

            defined_types.add_variable(&name, field_type);
        });
        match internal_baml_jinja_types::validate_template(
            template.name(),
            prompt.raw_value(),
            &mut defined_types,
        ) {
            Ok(_) => {}
            Err(e) => {
                let pspan = prompt.span();
                if let Some(e) = e.parsing_errors {
                    let range = match e.range() {
                        Some(range) => range,
                        None => {
                            ctx.push_error(DatamodelError::new_validation_error(
                                &format!("Error parsing jinja template: {e}"),
                                pspan.clone(),
                            ));
                            continue;
                        }
                    };

                    let start_offset = pspan.start + range.start;
                    let end_offset = pspan.start + range.end;

                    let span = Span::new(pspan.file.clone(), start_offset, end_offset);

                    ctx.push_error(DatamodelError::new_validation_error(
                        &format!("Error parsing jinja template: {e}"),
                        span,
                    ))
                } else {
                    e.errors.iter().for_each(|t| {
                        let span = t.span();
                        let span = Span::new(
                            pspan.file.clone(),
                            pspan.start + span.start_offset as usize,
                            pspan.start + span.end_offset as usize,
                        );
                        ctx.push_warning(DatamodelWarning::new(t.message().to_string(), span))
                    })
                }
            }
        }
        defined_types.end_scope();
        defined_types.errors_mut().clear();
    }
}
