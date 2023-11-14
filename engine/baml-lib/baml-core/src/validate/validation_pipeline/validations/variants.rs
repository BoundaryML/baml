use std::collections::HashSet;

use internal_baml_diagnostics::DatamodelError;

use internal_baml_parser_database::{PrinterType, PromptVariable};
use internal_baml_schema_ast::ast::{WithIdentifier, WithName, WithSpan};

use crate::validate::validation_pipeline::context::Context;

pub(super) fn validate(ctx: &mut Context<'_>) {
    for variant in ctx.db.walk_variants() {
        let client = &variant.properties().client;

        if variant.client().is_none() {
            ctx.push_error(DatamodelError::new_validation_error(
                &format!("Unknown client `{}`", client.value),
                client.span.clone(),
            ));
        }

        if let Some(_function) = variant.walk_function() {
            // Ensure that every serializer is valid.
            variant.ast_variant().iter_serializers().for_each(|(_, f)| {
                match ctx.db.find_type(f.identifier()) {
                    Some(_) => {}
                    None => {
                        ctx.push_error(DatamodelError::new_validation_error(
                            &format!("Unknown override `{}`", f.identifier().name()),
                            f.identifier().span().clone(),
                        ));
                    }
                }
            });

            match variant.properties().pre_deserializer.as_ref() {
                Some((p, key_span)) => {
                    // Ensure no duplicate languages in pre_deserializer.
                    if p.len()
                        != p.iter()
                            .map(|f| {
                                if f.language.is_none() {
                                    ctx.push_error(DatamodelError::new_validation_error(
                                        "Missing language in pre_deserializer. e.g. python#\"...\"# instead of #\"...\"#",
                                        f.span().clone(),
                                    ));
                                    return "unknown";
                                }
                                f.language.as_ref().unwrap().0.as_str()
                            })
                            .collect::<HashSet<_>>()
                            .len()
                    {
                        ctx.push_error(DatamodelError::new_validation_error(
                            "Duplicate languages in pre_deserializer",
                            key_span.clone(),
                        ));
                    }
                }
                None => {}
            }

            // Ensure that all blocks are valid.
            variant
                .properties()
                .prompt_replacements
                .iter()
                .filter_map(|p| match p {
                    PromptVariable::Enum(e) => e.printer.as_ref().map(|f| (f, "enum")),
                    PromptVariable::Type(t) => t.printer.as_ref().map(|f| (f, "type")),
                    PromptVariable::Input(_) => None,
                })
                .for_each(|(printer, t)| {
                    match ctx.db.find_printer(&printer.0) {
                        Some(w) => {
                            match w.printer() {
                                PrinterType::Enum(_) => {
                                    if t == "enum" {
                                        // All good.
                                    } else {
                                        ctx.push_error(DatamodelError::new_validation_error(
                                            &format!(
                                                "Expected a printer<type>, found printer<enum> {}",
                                                printer.0
                                            ),
                                            printer.1.clone(),
                                        ));
                                    }
                                }
                                PrinterType::Type(_) => {
                                    if t == "type" {
                                        // All good.
                                    } else {
                                        ctx.push_error(DatamodelError::new_validation_error(
                                            &format!(
                                                "Expected a printer<enum>, found printer<type> {}",
                                                printer.0
                                            ),
                                            printer.1.clone(),
                                        ));
                                    }
                                }
                            }
                        }
                        None => ctx.push_error(DatamodelError::new_type_not_found_error(
                            &printer.0,
                            ctx.db.valid_printer_names(),
                            printer.1.clone(),
                        )),
                    }
                });
        } else {
            ctx.push_error(DatamodelError::new_type_not_found_error(
                variant.function_identifier().name(),
                ctx.db.valid_function_names(),
                variant.function_identifier().span().clone(),
            ));
        }
    }
}
