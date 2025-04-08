use baml_types::GeneratorOutputType;
use internal_baml_schema_ast::ast::{Field, FieldType, WithIdentifier, WithName, WithSpan};

use super::types::validate_type;
use crate::validate::validation_pipeline::context::Context;
use internal_baml_diagnostics::DatamodelError;

use itertools::join;
use std::collections::{HashMap, HashSet};

pub(super) fn validate(ctx: &mut Context<'_>) {
    let mut defined_types = internal_baml_jinja_types::PredefinedTypes::default(
        internal_baml_jinja_types::JinjaContext::Prompt,
    );

    for cls in ctx.db.walk_classes() {
        for c in cls.static_fields() {
            let field = c.ast_field();
            if let Some(ft) = &field.expr {
                validate_type(ctx, ft);
            }
        }

        for args in cls.walk_input_args() {
            let arg = args.ast_arg();
            validate_type(ctx, &arg.1.field_type)
        }

        defined_types.start_scope();

        cls.walk_input_args().for_each(|arg| {
            let name = match arg.ast_arg().0 {
                Some(arg) => arg.name(),
                None => {
                    ctx.push_error(DatamodelError::new_validation_error(
                        "Argument name is missing.",
                        arg.ast_arg().1.span().clone(),
                    ));
                    return;
                }
            };

            let field_type = ctx.db.to_jinja_type(&arg.ast_arg().1.field_type);

            defined_types.add_variable(name, field_type);
        });

        defined_types.end_scope();
        defined_types.errors_mut().clear();
    }
}

/// Enforce that keywords in the user's requested target languages
/// do not appear as field names in BAML classes, and that field
/// names are not equal to type names when using Pydantic.
pub(super) fn assert_no_field_name_collisions(
    ctx: &mut Context<'_>,
    generator_output_types: &HashSet<GeneratorOutputType>,
) {
    // The list of reserved words for all user-requested codegen targets.
    let reserved = reserved_names(generator_output_types, ReservedNamesMode::FieldNames);

    for cls in ctx.db.walk_classes() {
        for c in cls.static_fields() {
            let field: &Field<FieldType> = c.ast_field();

            // Check for keyword in field name.
            if let Some(langs) = reserved.get(field.name()) {
                let msg = match langs.as_slice() {
                    [lang] => format!("Field name is a reserved word in generated {lang} clients."),
                    _ => format!(
                        "Field name is a reserved word in language clients: {}.",
                        join(langs, ", ")
                    ),
                };
                ctx.push_error(DatamodelError::new_field_validation_error(
                    msg,
                    "class",
                    c.name(),
                    field.name(),
                    field.identifier().span().clone(),
                ))
            }

            // Check for collision between field name and type name when using Pydantic.
            if generator_output_types.contains(&GeneratorOutputType::PythonPydantic) {
                let type_name = field
                    .expr
                    .as_ref()
                    .map_or("".to_string(), |r#type| r#type.name());
                if field.name() == type_name {
                    ctx.push_error(DatamodelError::new_field_validation_error(
                        "When using the python/pydantic generator, a field name must not be exactly equal to the type name. Consider changing the field name and using an alias.".to_string(),
                        "class",
                        c.name(),
                        field.name(),
                        field.identifier().span().clone(),
                    ))
                }
            }
        }
    }

    // check for reserved names in function parameters
    let reserved = reserved_names(generator_output_types, ReservedNamesMode::FunctionParameters);
    for func in ctx.db.walk_functions() {
        for param in func.walk_input_args() {
            match param.ast_arg().0 {
                Some(id) => {
                    match reserved.get(id.name()) {
                        Some(langs) => {
                            match langs.as_slice() {
                                [lang] => {
                                    ctx.push_error(DatamodelError::new_validation_error(
                                        &format!("{} is a reserved word in {}", id.name(), lang),
                                        id.span().clone(),
                                    ));
                                }
                                _ => {
                                    ctx.push_error(DatamodelError::new_validation_error(
                                        &format!("{} is a reserved word in language clients: {}", id.name(), join(langs, ", ")),
                                        id.span().clone(),
                                    ));
                                }
                            }
                        }
                        None => {}
                    }
                }
                None => {}
            }
        }
    }
}

enum ReservedNamesMode {
    FieldNames,
    FunctionParameters,
}

/// For a given set of target languages, construct a map from keyword to the
/// list of target languages in which that identifier is a keyword.
///
/// This will be used later to make error messages like, "Could not use name
/// `continue` becase that is a keyword in Python", "Could not use the name
/// `return` because that is a keyword in Python and Typescript".
fn reserved_names(
    generator_output_types: &HashSet<GeneratorOutputType>,
    mode: ReservedNamesMode,
) -> HashMap<&'static str, Vec<GeneratorOutputType>> {
    let mut keywords: HashMap<&str, Vec<GeneratorOutputType>> = HashMap::new();

    let language_keywords: Vec<(&str, GeneratorOutputType)> = [
        if generator_output_types.contains(&GeneratorOutputType::PythonPydantic) {
            match mode {
                ReservedNamesMode::FieldNames => {
                    RESERVED_NAMES_PYTHON
                        .iter()
                        .map(|name| (*name, GeneratorOutputType::PythonPydantic))
                        .collect()
                }
                ReservedNamesMode::FunctionParameters => {
                    RESERVED_NAMES_FUNCTION_PARAMETERS
                        .iter()
                        .chain(RESERVED_NAMES_PYTHON.iter())
                        .map(|name| (*name, GeneratorOutputType::PythonPydantic))
                        .collect()
                }
            }
        } else {
            Vec::new()
        },
        if generator_output_types.contains(&GeneratorOutputType::Typescript) {
            RESERVED_NAMES_TYPESCRIPT
                .iter()
                .map(|name| (*name, GeneratorOutputType::Typescript))
                .collect()
        } else {
            Vec::new()
        },
    ]
    .iter()
    .flatten()
    .cloned()
    .collect();

    language_keywords
        .into_iter()
        .for_each(|(keyword, generator_output_type)| {
            keywords
                .entry(keyword)
                .and_modify(|types| (*types).push(generator_output_type))
                .or_insert(vec![generator_output_type]);
        });

    keywords
}

// This list of keywords was copied from
// https://www.w3schools.com/python/python_ref_keywords.asp
// .
const RESERVED_NAMES_PYTHON: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield", // additional keywords that we use in BAML as they are types
];

const RESERVED_NAMES_FUNCTION_PARAMETERS: &[&str] = &[
    "list", "dict", "set", "tuple", "int", "float", "str", "bool",
];

// Typescript is much more flexible in the key names it allows.
const RESERVED_NAMES_TYPESCRIPT: &[&str] = &[];
