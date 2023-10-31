use either::Either;
use internal_baml_diagnostics::DatamodelError;
use internal_baml_prompt_parser::ast::{PrinterBlock, Variable};
use internal_baml_schema_ast::ast::{self, WithName};

use crate::{
    walkers::{FunctionWalker, VariantWalker},
    ParserDatabase, WithSerialize,
};

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

    match walker.ast_function().input() {
        ast::FunctionArgs::Unnamed(arg) => {
            validate_variable_path(db, variable, 1, &arg.field_type)?;
            let mut new_path = variable.path.clone();
            new_path[0] = "arg".to_string();
            return Ok(new_path.join("."));
        }
        ast::FunctionArgs::Named(args) => {
            if args.iter_args().len() <= 1 {
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
                    return Ok(variable.path[1..].join("."));
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
    }
}

pub(crate) fn process_print_enum(
    db: &ParserDatabase,
    walker: VariantWalker<'_>,
    fn_walker: FunctionWalker<'_>,
    blk: &PrinterBlock,
) -> Result<String, DatamodelError> {
    let variable = &blk.target;
    if variable.text == "output" {
        return Err(DatamodelError::new_validation_error(
            "output can only be used with print_type()",
            variable.span.clone(),
        ));
    }

    match db.find_type_by_str(&variable.text) {
        Some(Either::Right(enum_walker)) => {
            match fn_walker
                .walk_output_args()
                .map(|f| {
                    f.required_enums()
                        .any(|idn| idn.name() == enum_walker.name())
                })
                .any(|f| f)
            {
                true => enum_walker.serialize(&walker, blk),
                false => Err(DatamodelError::type_not_used_in_prompt_error(
                    true,
                    fn_walker.name(),
                    &variable.text,
                    fn_walker
                        .walk_output_args()
                        .map(|f| f.required_enums())
                        .flatten()
                        .map(|f| f.name().to_string())
                        .collect::<Vec<_>>(),
                    variable.span.clone(),
                )),
            }
        }
        Some(Either::Left(_)) => Err(DatamodelError::new_validation_error(
            "Expected enum, found class",
            variable.span.clone(),
        )),
        None => Err(DatamodelError::new_validation_error(
            &format!("Unknown enum `{}`", variable.text),
            variable.span.clone(),
        )),
    }
}

pub(crate) fn process_print_type(
    db: &ParserDatabase,
    walker: VariantWalker<'_>,
    fn_walker: FunctionWalker<'_>,
    blk: &PrinterBlock,
) -> Result<String, DatamodelError> {
    let variable = &blk.target;
    if variable.text == "output" {
        return fn_walker.serialize(&walker, blk);
    }

    match db.find_type_by_str(&variable.text) {
        Some(Either::Left(cls_walker)) => {
            // Also validate the function uses the enum.
            match fn_walker.walk_output_args().any(|f| {
                f.required_classes()
                    .any(|idn| idn.name() == cls_walker.name())
            }) {
                true => cls_walker.serialize(&walker, blk),
                false => Err(DatamodelError::new_validation_error(
                    &format!(
                        "Class `{}` is not used in in the output of function `{}`",
                        variable.text,
                        fn_walker.name()
                    ),
                    variable.span.clone(),
                )),
            }
        }
        Some(Either::Right(_)) => Err(DatamodelError::new_validation_error(
            "Expected class, found enum",
            variable.span.clone(),
        )),
        None => {
            let mut candidates = fn_walker
                .walk_output_args()
                .map(|f| f.required_enums())
                .flatten()
                .map(|f| f.name().to_string())
                .collect::<Vec<_>>();
            candidates.push("output".to_string());
            Err(DatamodelError::type_not_used_in_prompt_error(
                true,
                fn_walker.name(),
                &variable.text,
                candidates,
                variable.span.clone(),
            ))
        }
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
            .into_iter()
            .any(|ft| validate_variable_path(db, variable, next_index, ft).is_ok())
        {
            true => Ok(()),
            false => Err(DatamodelError::new_validation_error(
                &format!("Unknown field `{}` in Union", next_path_name),
                variable.span.clone(),
            )),
        },
        ast::FieldType::Dictionary(_, _) => Err(DatamodelError::new_validation_error(
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
        ast::FieldType::Identifier(_, idn) => match db.find_type(&idn) {
            Some(Either::Left(cls)) => {
                match cls
                    .static_fields()
                    .find(|f| f.name() == next_path_name.as_str())
                {
                    Some(field) => {
                        let t = field.r#type();
                        validate_variable_path(db, variable, next_index + 1, t)
                    }
                    None => Err(DatamodelError::new_validation_error(
                        &format!(
                            "Unknown field `{}` in class `{}`",
                            next_path_name,
                            idn.name()
                        ),
                        variable.span.clone(),
                    )),
                }
            }
            Some(Either::Right(_)) => Err(DatamodelError::new_validation_error(
                "Enum values are not indexable in the prompt",
                variable.span.clone(),
            )),
            None => match idn {
                ast::Identifier::Primitive(_p, _) => Err(DatamodelError::new_validation_error(
                    &format!(
                        "{0} has no field {1}. {0} is of type: {2}",
                        variable.path[..next_index].join("."),
                        next_path_name,
                        idn.name()
                    ),
                    variable.span.clone(),
                )),
                ast::Identifier::Ref(_, _) => Err(DatamodelError::new_validation_error(
                    "Namespace imports (using '.') are not yet supported.",
                    variable.span.clone(),
                )),
                ast::Identifier::ENV(_, _) => Err(DatamodelError::new_validation_error(
                    "Environment variables are not indexable in the prompt",
                    variable.span.clone(),
                )),
                _ => Err(DatamodelError::new_validation_error(
                    &format!("Unknown type `{}`.", idn.name()),
                    variable.span.clone(),
                )),
            },
        },
    }
}
