use std::{env::var, fmt::format};

use either::Either;
use internal_baml_diagnostics::{DatamodelError, Span};
use internal_baml_parser_database::walkers::FunctionWalker;
use internal_baml_schema_ast::ast::{self, FieldType, WithName};
use log::{info, warn};

use crate::{ast::WithSpan, validate::validation_pipeline::context::Context};
use internal_baml_prompt_parser::ast::{CodeBlock, CodeType, PromptAst, Top, TopId, Variable};

pub(super) fn validate(ctx: &mut Context<'_>) {
    for variant in ctx.db.walk_variants() {
        let client = &variant.properties().client;

        if ctx.db.find_client(client).is_none() {
            ctx.push_error(DatamodelError::new_validation_error(
                &format!("Unknown client `{}`", client),
                variant.ast_variant().span().clone(),
            ));
        }

        if let Some(fn_walker) = variant.walk_function() {
            // Function exists, do something with it
            match validate_prompt(
                ctx,
                fn_walker,
                variant.properties().prompt.clone(),
                &variant.ast_variant().span(),
            ) {
                Some(prompt) => {
                    info!(
                        "Prompt: for {}:{}\n---\n{}\n---\n",
                        fn_walker.name(),
                        variant.identifier().name(),
                        prompt
                    );
                }
                None => warn!(
                    "Prompt: for {}:{}\n---\n{}\n---\n",
                    fn_walker.name(),
                    variant.identifier().name(),
                    "Prompt could not be validated"
                ),
            }
        } else {
            ctx.push_error(DatamodelError::new_validation_error(
                "Function not found",
                variant.function_identifier().span().clone(),
            ));
        }
    }
}

// TODO: add a database of attributes, types, etc to each of the code blocks etc so we can access everything easily. E.g. store the field type of each codeblock variable path, etc.
fn validate_prompt(
    ctx: &mut Context<'_>,
    walker: FunctionWalker<'_>,
    prompt: (String, Span),
    span: &Span,
) -> Option<String> {
    if prompt.0.is_empty() {
        // Return an empty string if the prompt is empty.
        return Some(String::new());
    }

    let parsed_prompt =
        internal_baml_prompt_parser::parse_prompt(&ctx.diagnostics.root_path, &span.file, prompt);

    match parsed_prompt {
        Ok((ast, d)) => {
            ctx.diagnostics.push(d);
            let processed_prompt = process_ast(ctx, walker, ast.clone(), span);
            Some(textwrap::dedent(&processed_prompt).trim().to_string())
        }
        Err(diagnostics) => {
            ctx.diagnostics.push(diagnostics);
            None
        }
    }
}

fn handle_comment(
    prev_white_space: &mut Option<String>,
    post_white_space: &mut Option<String>,
) -> Option<String> {
    // Determine if post_white_space or prev_white_space should be used
    // based on whichever one is longer.
    let ws = match (&prev_white_space, &post_white_space) {
        (Some(prev), Some(post)) => {
            let prev_score: u32 = prev
                .chars()
                .map(|c| match c.to_string().as_str() {
                    " " => 1,
                    "\t" => 10,
                    "\n" | "\r" | "\r\n" => 100,
                    _ => 1000,
                })
                .sum();
            let post_score = post
                .chars()
                .map(|c| match c.to_string().as_str() {
                    " " => 1,
                    "\t" => 10,
                    "\n" | "\r" | "\r\n" => 100,
                    _ => 1000,
                })
                .sum();
            if prev_score > post_score {
                prev.clone()
            } else {
                post.clone()
            }
        }
        (Some(prev), None) => prev.clone(),
        (None, Some(post)) => post.clone(),
        (None, None) => return None,
    };
    *prev_white_space = None;
    *post_white_space = None;

    Some(ws.clone())
}

fn process_ast(
    ctx: &mut Context<'_>,
    walker: FunctionWalker<'_>,
    ast: PromptAst,
    span: &Span,
) -> String {
    let mut prev_white_space: Option<String> = None;
    let mut post_white_space: Option<String> = None;
    let mut is_comment = false;
    let mut full_prompt_text = String::new();

    for (top_id, top) in ast.iter_tops() {
        match (top_id, top) {
            (_, Top::PromptText(prompt)) => {
                is_comment = false;
                match handle_comment(&mut prev_white_space, &mut post_white_space) {
                    Some(ws) => full_prompt_text.push_str(&ws),
                    None => (),
                }
                full_prompt_text.push_str(&prompt.text);
            }
            (_, Top::WhiteSpace(ws, _)) => {
                if is_comment {
                    post_white_space = match post_white_space {
                        Some(existing_ws) => Some(format!("{}{}", existing_ws, ws.to_string())),
                        None => Some(ws.to_string()),
                    };
                } else {
                    prev_white_space = match prev_white_space {
                        Some(existing_ws) => Some(format!("{}{}", existing_ws, ws.to_string())),
                        None => Some(ws.to_string()),
                    };
                }
            }
            (_, Top::CommentBlock(_)) => {
                if is_comment {
                    // Already in a comment and finding another comment block
                    // subsequently.
                    // This resets the after comment white space.
                    match handle_comment(&mut prev_white_space, &mut post_white_space) {
                        Some(ws) => prev_white_space = Some(ws),
                        None => (),
                    }
                } else {
                    is_comment = true;
                }
            }
            (_, Top::CodeBlock(code_block)) => {
                is_comment = false;
                match handle_comment(&mut prev_white_space, &mut post_white_space) {
                    Some(ws) => full_prompt_text.push_str(&ws),
                    None => (),
                }
                let replacement = process_code_block(ctx, walker, code_block, span);
                match replacement {
                    Some(replacement) => {
                        full_prompt_text.push_str(&format!("{{{}}}", &replacement))
                    }
                    None => {
                        info!(
                            "Failed to find replacement for code block: {:?}",
                            code_block.block.as_str()
                        );
                        full_prompt_text.push_str(&format!("{{{}}}", code_block.block.as_str()))
                    }
                }
            }
        }
    }

    full_prompt_text
}

fn process_code_block(
    ctx: &mut Context<'_>,
    walker: FunctionWalker<'_>,
    code_block: &CodeBlock,
    span: &Span,
) -> Option<String> {
    if code_block.arguments.len() != 1 {
        ctx.push_error(DatamodelError::new_validation_error(
            "Must specify exactly one argument",
            code_block.span.clone(),
        ));
        return None;
    }
    let variable = code_block.arguments.first().unwrap();

    if variable.text.is_empty() || variable.path.is_empty() {
        ctx.push_error(DatamodelError::new_validation_error(
            "Variable path cannot be empty",
            variable.span.clone(),
        ));
        return None;
    }

    match code_block.code_type {
        CodeType::Variable => process_variable(ctx, walker, variable),
        CodeType::PrintEnum => process_print_enum(ctx, walker, variable),
        CodeType::PrintType => process_print_type(ctx, walker, variable),
    }
}

fn process_print_enum(
    ctx: &mut Context<'_>,
    walker: FunctionWalker<'_>,
    variable: &Variable,
) -> Option<String> {
    if variable.text == "output" {
        ctx.push_error(DatamodelError::new_validation_error(
            "output can only be used with print_type()",
            variable.span.clone(),
        ));
        return None;
    }

    match ctx.db.find_type_by_str(&variable.text) {
        Some(Either::Right(enum_walker)) => {
            match walker
                .walk_output_args()
                .map(|f| {
                    f.required_enums()
                        .any(|idn| idn.name() == enum_walker.name())
                })
                .any(|f| f)
            {
                true => Some(format!("BamlClientString__{}", variable.text)),
                false => {
                    ctx.push_error(DatamodelError::new_validation_error(
                        &format!(
                            "Enum `{}` is not used in in the output of function `{}`.{}",
                            variable.text,
                            walker.name(),
                            {
                                let enum_options = walker
                                    .walk_output_args()
                                    .map(|f| f.required_enums())
                                    .flatten()
                                    .map(|idn| idn.name())
                                    .collect::<Vec<_>>();
                                if enum_options.is_empty() {
                                    "".to_string()
                                } else {
                                    format!("Options are: {}", enum_options.join(", "))
                                }
                            }
                        ),
                        variable.span.clone(),
                    ));
                    None
                }
            }
        }
        Some(Either::Left(_)) => {
            ctx.push_error(DatamodelError::new_validation_error(
                "Expected enum, found class",
                variable.span.clone(),
            ));
            return None;
        }
        None => {
            ctx.push_error(DatamodelError::new_validation_error(
                &format!("Unknown enum `{}`", variable.text),
                variable.span.clone(),
            ));
            return None;
        }
    }
}

fn process_print_type(
    ctx: &mut Context<'_>,
    walker: FunctionWalker<'_>,
    variable: &Variable,
) -> Option<String> {
    if variable.text == "output" {
        return Some(format!("BamlClientString__{}__Output", walker.name()));
    }

    match ctx.db.find_type_by_str(&variable.text) {
        Some(Either::Left(cls_walker)) => {
            // Also validate the function uses the enum.
            match walker.walk_output_args().any(|f| {
                f.required_classes()
                    .any(|idn| idn.name() == cls_walker.name())
            }) {
                true => Some(format!("BamlClientString__{}", variable.text)),
                false => {
                    ctx.push_error(DatamodelError::new_validation_error(
                        &format!(
                            "Class `{}` is not used in in the output of function `{}`",
                            variable.text,
                            walker.name()
                        ),
                        variable.span.clone(),
                    ));
                    None
                }
            }
        }
        Some(Either::Right(_)) => {
            ctx.push_error(DatamodelError::new_validation_error(
                "Expected class, found enum",
                variable.span.clone(),
            ));
            return None;
        }
        None => {
            ctx.push_error(DatamodelError::new_validation_error(
                &format!("Unknown enum `{}`", variable.text),
                variable.span.clone(),
            ));
            return None;
        }
    }
}

fn process_variable(
    ctx: &mut Context<'_>,
    walker: FunctionWalker<'_>,
    variable: &Variable,
) -> Option<String> {
    match process_input(ctx, walker, variable) {
        Ok(p) => Some(p),
        Err(err) => {
            ctx.push_error(err);
            None
        }
    }
}

fn process_input(
    ctx: &mut Context<'_>,
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
            validate_variable_path(ctx, variable, 1, &arg.field_type)?;
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
                    validate_variable_path(ctx, variable, 2, &arg.field_type)?;
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

fn validate_variable_path(
    ctx: &mut Context<'_>,
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
        FieldType::Union(_, ft, _) => match ft
            .into_iter()
            .any(|ft| validate_variable_path(ctx, variable, next_index, ft).is_ok())
        {
            true => Ok(()),
            false => Err(DatamodelError::new_validation_error(
                &format!("Unknown field `{}` in Union", next_path_name),
                variable.span.clone(),
            )),
        },
        FieldType::Dictionary(_, _) => Err(DatamodelError::new_validation_error(
            "Dictionary types are not supported",
            variable.span.clone(),
        )),
        FieldType::Tuple(_, _, _) => Err(DatamodelError::new_validation_error(
            "Tuple types are not supported",
            variable.span.clone(),
        )),
        FieldType::List(_, _, _) => Err(DatamodelError::new_validation_error(
            "List types are not yet indexable in the prompt",
            variable.span.clone(),
        )),
        FieldType::Identifier(_, idn) => match ctx.db.find_type(&idn) {
            Some(Either::Left(cls)) => {
                match cls
                    .static_fields()
                    .find(|f| f.name() == next_path_name.as_str())
                {
                    Some(field) => {
                        validate_variable_path(ctx, variable, next_index + 1, field.r#type())
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
