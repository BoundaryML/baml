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
        ctx.push_error(DatamodelError::new_validation_error(
            "Prompt cannot be empty",
            span.clone(),
        ));
    }

    let validated_prompt = internal_baml_prompt_parser::parse_prompt(
        &ctx.diagnostics.root_path,
        &span.file,
        prompt.clone(),
    );

    match validated_prompt {
        Ok((ast, d)) => {
            ctx.diagnostics.push(d);

            process_ast(ctx, walker, ast.clone(), span);
            let mut full_prompt_text = String::new();
            for (top_id, top) in ast.iter_tops() {
                match (top_id, top) {
                    (TopId::PromptText(_), Top::PromptText(prompt)) => {
                        full_prompt_text.push_str(&prompt.text);
                    }
                    (TopId::CodeBlock(_), Top::CodeBlock(code_block)) => {
                        full_prompt_text.push_str(&format!("{{{}}}", code_block.block.as_str()));
                    }
                    _ => (),
                }
            }

            Some(textwrap::dedent(&full_prompt_text).trim().to_string())
        }
        Err(diagnostics) => {
            ctx.diagnostics.push(diagnostics);
            None
        }
    }
}

fn process_ast(ctx: &mut Context<'_>, walker: FunctionWalker<'_>, ast: PromptAst, span: &Span) {
    for (top_id, top) in ast.iter_tops() {
        match (top_id, top) {
            (TopId::CodeBlock(_), Top::CodeBlock(code_block)) => {
                process_code_block(ctx, walker, code_block, span)
            }
            _ => (),
        }
    }
}

fn process_code_block(
    ctx: &mut Context<'_>,
    walker: FunctionWalker<'_>,
    code_block: &CodeBlock,
    span: &Span,
) {
    match code_block.code_type {
        CodeType::Variable => process_variable(ctx, walker, code_block, span),
        other => warn!("Code block type not supported {:?}", other),
    }
}

fn process_variable(
    ctx: &mut Context<'_>,
    walker: FunctionWalker<'_>,
    code_block: &CodeBlock,
    span: &Span,
) {
    if code_block.arguments.len() != 1 {
        ctx.push_error(DatamodelError::new_validation_error(
            "Empty block detected",
            code_block.span.clone(),
        ));
        return;
    }
    let variable = code_block.arguments.first().unwrap();
    if let Err(e) = process_input(ctx, walker, variable, span) {
        ctx.push_error(e);
    }
}

fn process_input(
    ctx: &mut Context<'_>,
    walker: FunctionWalker<'_>,
    variable: &Variable,
    span: &Span,
) -> Result<(), DatamodelError> {
    if variable.path.is_empty() {
        return Err(DatamodelError::new_validation_error(
            "Variable path cannot be empty",
            span.clone(),
        ));
    }

    if variable.path[0] != "input" {
        return Err(DatamodelError::new_validation_error(
            "Must start with `input`",
            span.clone(),
        ));
    }

    match walker.ast_function().input() {
        ast::FunctionArgs::Unnamed(arg) => {
            validate_variable_path(ctx, variable, 1, &arg.field_type)
        }
        ast::FunctionArgs::Named(args) => {
            if args.iter_args().len() <= 1 {
                return Err(DatamodelError::new_validation_error(
                    "Named arguments must have at least one argument (input.my_var_name)",
                    span.clone(),
                ));
            }
            let path_name = &variable.path[1];
            match args
                .iter_args()
                .find(|(_, (name, _))| name.name() == path_name)
            {
                Some((_, (_, arg))) => validate_variable_path(ctx, variable, 2, &arg.field_type),
                None => Err(DatamodelError::new_validation_error(
                    &format!(
                        "Unknown arg `{}`. Could be one of: {}",
                        path_name,
                        args.iter_args()
                            .map(|(_, (name, _))| name.name())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    span.clone(),
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
