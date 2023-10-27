use internal_baml_diagnostics::{DatamodelError, Span};
use internal_baml_parser_database::walkers::FunctionWalker;
use internal_baml_schema_ast::ast::{Class, FieldType, FunctionId};
use serde::de;

use crate::{ast::WithSpan, validate::validation_pipeline::context::Context};
use internal_baml_prompt_parser::ast::{CodeBlock, CodeType, PromptAst, Top, TopId, Variable};

pub(super) fn validate(ctx: &mut Context<'_>) {
    for variant in ctx.db.walk_variants() {
        let mut fn_walker = None;
        if let Some(function) = variant.walk_function() {
            // Function exists, do something with it
            fn_walker = Some(function);
        } else {
            ctx.push_error(DatamodelError::new_validation_error(
                &format!("Function not found: {}", variant.function_name()),
                variant.ast_variant().span().clone(),
            ));
        }

        let client = &variant.properties().client;

        if ctx.db.find_client(client).is_none() {
            ctx.push_error(DatamodelError::new_validation_error(
                &format!("Unknown client `{}`", client.as_str()),
                variant.ast_variant().span().clone(),
            ));
        }

        validate_prompt(
            ctx,
            fn_walker,
            variant.properties().prompt.clone(),
            &variant.ast_variant().span(),
        );
    }
}

fn max_leading_whitespace_to_remove(input: &str) -> usize {
    input
        .lines()
        .filter(|line| {
            let is_valid = !line.trim().is_empty() && !line.trim().eq("\n");
            if is_valid {
                println!("{}", line); // Print the line if it made it past the filter
            }
            is_valid
        }) // Filter out empty lines and lines that are only newlines
        .map(|line| line.chars().take_while(|c| c.is_whitespace()).count()) // Count leading whitespaces for each line
        .min()
        .unwrap_or(0) // Return the minimum count or 0 if there are no lines
}

// TODO: add a database of attributes, types, etc to each of the code blocks etc so we can access everything easily. E.g. store the field type of each codeblock variable path, etc.
fn validate_prompt(
    ctx: &mut Context<'_>,
    walker: Option<FunctionWalker<'_>>,
    prompt: (String, Span),
    span: &Span,
) {
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
        Ok((ast, _)) => {
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

            full_prompt_text = textwrap::dedent(&full_prompt_text).trim().to_string();
            println!("\nfull prompt text:--------\n{}\n----", full_prompt_text)
        }
        Err(diagnostics) => println!("error {:?}", diagnostics.to_pretty_string()),
    }
}

fn indent_unindented_lines(full_prompt_text: &str, dedent: usize) -> String {
    let mut result = String::new();
    let indent_str = " ".repeat(dedent);
    for line in full_prompt_text.lines() {
        if line.chars().take(dedent).all(|c| c.is_whitespace()) {
            result.push_str(line);
        } else {
            result.push_str(&indent_str);
            result.push_str(line);
        }
        result.push('\n');
    }
    result
}

fn process_ast(
    ctx: &mut Context<'_>,
    walker: Option<FunctionWalker<'_>>,
    ast: PromptAst,
    span: &Span,
) {
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
    walker: Option<FunctionWalker<'_>>,
    code_block: &CodeBlock,
    span: &Span,
) {
    if let CodeType::Variable = code_block.code_type {
        process_variable(ctx, walker, code_block, span);
    }
}

fn process_variable(
    ctx: &mut Context<'_>,
    walker: Option<FunctionWalker<'_>>,
    code_block: &CodeBlock,
    span: &Span,
) {
    if let Some(variable) = code_block.arguments.first() {
        let var_name = variable.path.first().unwrap();
        if var_name == "input" && variable.path.len() > 1 {
            if let Some(walker) = walker {
                process_input(ctx, walker, variable, span);
            }
        }
    } else {
        ctx.push_error(DatamodelError::new_validation_error(
            "Variable does not exist",
            code_block.span.clone(),
        ));
    }
}

fn process_input(
    ctx: &mut Context<'_>,
    walker: FunctionWalker<'_>,
    variable: &Variable,
    span: &Span,
) {
    let first_input_arg_class = get_first_input_arg_class(&walker);
    if let Some(name) = first_input_arg_class {
        if let Some(class) = ctx.db.find_class(name.as_str()) {
            validate_variable_path(
                ctx,
                variable.path.clone(),
                &variable.span,
                1,
                class.ast_class().clone(),
            );
        }
    }
}

fn get_first_input_arg_class(walker: &FunctionWalker<'_>) -> Option<String> {
    walker
        .walk_input_args()
        .next()
        .and_then(|arg| Some(arg.ast_arg().1))
        .and_then(|f| match &f.field_type {
            FieldType::Supported(s) => Some(s.name.clone()),

            _ => None,
        })
}
fn validate_variable_path(
    ctx: &mut Context<'_>,
    path: Vec<String>,
    span: &Span,
    index: usize,
    current_class: Class,
) {
    if index >= path.clone().len() {
        return;
    }

    let part = &path.clone()[index];

    let field = current_class
        .fields()
        .into_iter()
        .find(|field| field.name() == part.as_str());
    if let Some(field) = field {
        let field_type = field.clone().field_type;
        match field_type {
            FieldType::Union(s, span) => {
                // TODO: traverse recursively and gather all root types?
                // for union in s {
                //     let curr_class = ctx.db.find_class(union.name.as_str());
                //     if curr_class.is_none() {
                //         ctx.diagnostics
                //             .push_error(DatamodelError::new_validation_error(
                //                 &format!(
                //                     "Unknown attribute `{}` in class `{}`",
                //                     part.as_str(),
                //                     current_class.name.name.as_str()
                //                 ),
                //                 span.clone(),
                //             ));
                //     } else {
                //         validate_variable_path(
                //             ctx,
                //             path.clone(),
                //             span,
                //             index + 1,
                //             Some(current_class),
                //         )
                //     }
                // }
                return;
            }
            FieldType::Supported(s) => {
                let curr_class = ctx.db.find_class(s.name.as_str());

                if let Some(curr_class) = curr_class {
                    validate_variable_path(
                        ctx,
                        path.clone(),
                        span,
                        index + 1,
                        curr_class.ast_class().clone(),
                    )
                } else {
                    ctx.diagnostics
                        .push_error(DatamodelError::new_validation_error(
                            &format!(
                                "Unknown attributee `{}` in class `{}`.",
                                part.as_str(),
                                s.name.as_str()
                            ),
                            span.clone(),
                        ));
                    return;
                }
            }
            FieldType::PrimitiveType(s, field_span) => {
                if (path.len() > index + 1) {
                    ctx.diagnostics
                        .push_error(DatamodelError::new_validation_error(
                            &format!("Attribute `{}` does not exist", path[index + 1].as_str(),),
                            span.clone(),
                        ));
                }
            }
            _ => {}
        }
        // If it is a field, validate the next part in the path
        //validate_variable_path(ctx, path, span, index + 1, current_class);
    } else {
        ctx.diagnostics
            .push_error(DatamodelError::new_validation_error(
                &format!(
                    "Unknown attribute `{}` in class `{}`",
                    part.as_str(),
                    current_class.name.name.as_str()
                ),
                span.clone(),
            ));
    }
}
