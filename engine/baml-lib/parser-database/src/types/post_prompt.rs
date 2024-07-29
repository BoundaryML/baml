use either::Either;
use internal_baml_diagnostics::{DatamodelError, DatamodelWarning, Diagnostics};
use internal_baml_prompt_parser::ast::{PrinterBlock, Variable, WithSpan};
use internal_baml_schema_ast::ast::{self, WithName};

use crate::{walkers::FunctionWalker, ParserDatabase, WithSerialize};

pub(crate) fn process_input(
    db: &ParserDatabase,
    walker: FunctionWalker<'_>,
    variable: &Variable,
) -> Result<String, DatamodelError> {
    if variable.path[0] != "input" {
        return Err(DatamodelError::new_validation_error(
            "Must start with `input`",
            variable.span.clone(),
        ));
    }

    let args = walker.ast_function().input().expect("Expected input args");
    if variable.path.len() < 2 {
        return Err(DatamodelError::new_validation_error(
            "Named arguments must have at least one argument (input.my_var_name)",
            variable.span.clone(),
        ));
    }
    let path_name = &variable.path[1];
    match args
        .iter_args()
        .find(|(_, (name, _))| name.name() == path_name)
    {
        Some((_, (_, arg))) => {
            validate_variable_path(db, variable, 2, &arg.field_type)?;
            Ok(variable.path[1..].join("."))
        }
        None => Err(DatamodelError::new_validation_error(
            &format!(
                "Unknown arg `{}`. Could be one of: {}",
                path_name,
                args.iter_args()
                    .map(|(_, (name, _))| name.name())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            variable.span.clone(),
        )),
    }
}

pub(crate) fn process_print_enum(
    db: &ParserDatabase,
    fn_walker: FunctionWalker<'_>,
    blk: &PrinterBlock,
    diag: &mut Diagnostics,
) -> Result<String, DatamodelError> {
    let variable = &blk.target;
    if variable.text == "output" {
        return Err(DatamodelError::new_validation_error(
            "output can only be used with print_type()",
            variable.span.clone(),
        ));
    }

    let candidates = fn_walker
        .walk_output_args()
        .flat_map(|f| f.required_enums())
        .map(|f| f.name().to_string())
        .collect::<Vec<_>>();

    match db.find_type_by_str(&variable.text) {
        Some(Either::Right(enum_walker)) => {
            if !fn_walker
                .walk_output_args()
                .map(|f| {
                    f.required_enums()
                        .any(|idn| idn.name() == enum_walker.name())
                })
                .any(|f| f)
            {
                diag.push_warning(DatamodelWarning::type_not_used_in_prompt_error(
                    true,
                    true,
                    fn_walker.name(),
                    &variable.text,
                    candidates,
                    variable.span.clone(),
                ));
            }
            enum_walker.serialize(fn_walker.db, blk.span())
        }
        Some(Either::Left(_)) => Err(DatamodelError::new_validation_error(
            "Expected enum, found class",
            variable.span.clone(),
        )),
        None => Err(DatamodelError::type_not_used_in_prompt_error(
            true,
            false,
            fn_walker.name(),
            &variable.text,
            candidates,
            variable.span.clone(),
        )),
    }
}

pub(crate) fn process_print_type(
    db: &ParserDatabase,
    fn_walker: FunctionWalker<'_>,
    blk: &PrinterBlock,
) -> Result<String, DatamodelError> {
    let variable = &blk.target;
    if variable.text == "output" {
        return fn_walker.serialize(fn_walker.db, blk.span());
    }

    let candidates = fn_walker
        .walk_output_args()
        .flat_map(|f| f.required_classes())
        .map(|f| f.name().to_string())
        .collect::<Vec<_>>();

    match db.find_type_by_str(&variable.text) {
        Some(Either::Left(cls_walker)) => {
            // Also validate the function uses the enum.
            match fn_walker.walk_output_args().any(|f| {
                f.required_classes()
                    .any(|idn| idn.name() == cls_walker.name())
            }) {
                true => cls_walker.serialize(fn_walker.db, blk.span()),
                false => Err(DatamodelError::type_not_used_in_prompt_error(
                    false,
                    true,
                    fn_walker.name(),
                    &variable.text,
                    candidates,
                    variable.span.clone(),
                )),
            }
        }
        Some(Either::Right(_)) => Err(DatamodelError::new_validation_error(
            "Expected class, found enum",
            variable.span.clone(),
        )),
        None => Err(DatamodelError::type_not_used_in_prompt_error(
            false,
            false,
            fn_walker.name(),
            &variable.text,
            candidates,
            variable.span.clone(),
        )),
    }
}

fn validate_variable_path(
    db: &ParserDatabase,
    variable: &Variable,
    next_index: usize,
    current: &ast::FieldType,
) -> Result<(), DatamodelError> {
    if next_index >= variable.path.len() {
        // Consider throwing a warning if current is not primitive.
        return Ok(());
    }

    let next_path_name = variable.path[next_index].clone();
    match current {
        ast::FieldType::Union(_, ft, _) => match ft
            .iter()
            .any(|ft| validate_variable_path(db, variable, next_index, ft).is_ok())
        {
            true => Ok(()),
            false => Err(DatamodelError::new_validation_error(
                &format!("Unknown field `{}` in Union", next_path_name),
                variable.span.clone(),
            )),
        },
        ast::FieldType::Primitive(_, ft, _) => Err(DatamodelError::new_validation_error(
            "Primitive types are not indexable in the prompt",
            variable.span.clone(),
        )),
        ast::FieldType::Map(_, _) => Err(DatamodelError::new_validation_error(
            "Dictionary types are not supported",
            variable.span.clone(),
        )),
        ast::FieldType::Tuple(_, _, _) => Err(DatamodelError::new_validation_error(
            "Tuple types are not supported",
            variable.span.clone(),
        )),
        ast::FieldType::List(_, _, _) => Err(DatamodelError::new_validation_error(
            "List types are not yet indexable in the prompt",
            variable.span.clone(),
        )),
        ast::FieldType::Symbol(_, idn, _) => match db.find_type_by_str(idn) {
            Some(Either::Left(cls)) => {
                match cls
                    .static_fields()
                    .find(|f| f.name() == next_path_name.as_str())
                {
                    Some(field) => {
                        let t = field.r#type();
                        validate_variable_path(db, variable, next_index + 1, t)
                    }
                    None => match cls
                        .dynamic_fields()
                        .find(|f| f.name() == next_path_name.as_str())
                    {
                        Some(_) => {
                            // Throw an error if the next path is not the last path.
                            if next_index + 1 < variable.path.len() {
                                Err(DatamodelError::new_validation_error(
                                    "get properties must be the last path in the prompt",
                                    variable.span.clone(),
                                ))
                            } else {
                                Ok(())
                            }
                        }
                        None => Err(DatamodelError::new_validation_error(
                            &format!("Unknown field `{}` in class `{}`", next_path_name, idn),
                            variable.span.clone(),
                        )),
                    },
                }
            }
            Some(Either::Right(_)) => Err(DatamodelError::new_validation_error(
                "Enum values are not indexable in the prompt",
                variable.span.clone(),
            )),
            None => match idn {
                _ => Err(DatamodelError::new_validation_error(
                    &format!("Unknown type `{}`.", idn),
                    variable.span.clone(),
                )),
            },
        },
    }
}
