use std::collections::HashSet;

use internal_baml_ast::ast::{
    FieldType, TypeAliasId, TypeExpId, WithIdentifier, WithName, WithSpan,
};
use internal_baml_diagnostics::{DatamodelError, DatamodelWarning, Span};
use internal_baml_parser_database::TypeWalker;

use super::types::validate_type;
use crate::validate::validation_pipeline::context::Context;

pub(super) fn validate(ctx: &mut Context<'_>) {
    let clients = ctx
        .db
        .walk_clients()
        .map(|c| c.name().to_string())
        .collect::<Vec<_>>();

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

    for func in ctx.db.walk_functions() {
        for args in func.walk_input_args().chain(func.walk_output_args()) {
            let arg = args.ast_arg();
            validate_type(ctx, &arg.1.field_type);
        }

        for args in func.walk_input_args() {
            let arg = args.ast_arg();
            let field_type = &arg.1.field_type;

            let span = field_type.span().clone();
            if has_checks_nested(ctx, field_type) {
                ctx.push_error(DatamodelError::new_validation_error(
                    "Types with checks are not allowed as function parameters.",
                    span,
                ));
            }
        }

        // Ensure the client is correct.
        // TODO: message to the user that it should be either a client ref OR an inline client
        match func.client_spec() {
            Ok(_) => {}
            Err(e) => {
                let client = match func.metadata().client.as_ref() {
                    Some(client) => client,
                    None => {
                        ctx.push_error(DatamodelError::new_validation_error(
                            "Client metadata is missing.",
                            func.span().clone(),
                        ));
                        continue;
                    }
                };
                ctx.push_error(DatamodelError::not_found_error(
                    "Client",
                    &client.0,
                    client.1.clone(),
                    clients.clone(),
                    false,
                ))
            }
        }

        let prompt = match func.metadata().prompt.as_ref() {
            Some(prompt) => prompt,
            None => {
                ctx.push_error(DatamodelError::new_validation_error(
                    "Prompt metadata is missing.",
                    func.span().clone(),
                ));
                continue;
            }
        };
        defined_types.start_scope();

        func.walk_input_args().for_each(|arg| {
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
            func.name(),
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

/// Just syntactic sugar for the recursive check.
///
/// See [`NestedChecks::has_checks_nested`].
pub(super) fn has_checks_nested(ctx: &Context<'_>, field_type: &FieldType) -> bool {
    NestedChecks::new(ctx).has_checks_nested(field_type)
}

struct NestedChecks<'c> {
    ctx: &'c Context<'c>,
    visited_classes: HashSet<TypeExpId>,
    visited_type_aliases: HashSet<TypeAliasId>,
}

impl<'c> NestedChecks<'c> {
    fn new(ctx: &'c Context<'c>) -> Self {
        Self {
            ctx,
            visited_classes: HashSet::new(),
            visited_type_aliases: HashSet::new(),
        }
    }

    /// Recusively search for `check` attributes in a field type and all of its
    /// composed children.
    fn has_checks_nested(&mut self, field_type: &FieldType) -> bool {
        if field_type.has_checks() {
            return true;
        }

        match field_type {
            FieldType::Symbol(_, id, ..) => match self.ctx.db.find_type(id) {
                Some(TypeWalker::Class(class_walker)) => {
                    // Stop recursion when dealing with recursive types.
                    if !self.visited_classes.insert(class_walker.id) {
                        return false;
                    }

                    let mut fields = class_walker.static_fields();
                    fields.any(|field| {
                        field
                            .ast_field()
                            .expr
                            .as_ref()
                            .is_some_and(|ft| self.has_checks_nested(ft))
                    })
                }
                Some(TypeWalker::TypeAlias(type_alias_walker)) => {
                    if !self.visited_type_aliases.insert(type_alias_walker.id) {
                        return false;
                    }

                    let resolved = type_alias_walker.resolved();
                    self.has_checks_nested(resolved)
                }
                _ => false,
            },

            FieldType::Primitive(..) => false,
            FieldType::Union(_, children, ..) => {
                children.iter().any(|ft| self.has_checks_nested(ft))
            }
            FieldType::Literal(..) => false,
            FieldType::Tuple(_, children, ..) => {
                children.iter().any(|ft| self.has_checks_nested(ft))
            }
            FieldType::List(_, child, ..) => self.has_checks_nested(child),
            FieldType::Map(_, kv, ..) => {
                self.has_checks_nested(&kv.as_ref().0) || self.has_checks_nested(&kv.as_ref().1)
            }
        }
    }
}
