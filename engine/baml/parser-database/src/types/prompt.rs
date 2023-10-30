use internal_baml_diagnostics::{DatamodelError, Span};
use internal_baml_prompt_parser::ast::{CodeBlock, CodeType, PromptAst, Top, Variable};

use crate::context::Context;

use super::PromptVariable;

// TODO: add a database of attributes, types, etc to each of the code blocks etc so we can access everything easily. E.g. store the field type of each codeblock variable path, etc.
pub(super) fn validate_prompt(
    ctx: &mut Context<'_>,
    prompt: (&str, Span),
    span: &Span,
) -> Option<(String, Vec<PromptVariable>)> {
    if prompt.0.is_empty() {
        // Return an empty string if the prompt is empty.
        return Some((String::new(), Default::default()));
    }

    let parsed_prompt =
        internal_baml_prompt_parser::parse_prompt(&ctx.diagnostics.root_path, &span.file, prompt);

    match parsed_prompt {
        Ok((ast, d)) => {
            ctx.diagnostics.push(d);
            let (processed_prompt, replacers) = process_prompt_ast(ctx, ast);
            Some((
                textwrap::dedent(&processed_prompt).trim().to_string(),
                replacers,
            ))
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

fn process_prompt_ast(ctx: &mut Context<'_>, ast: PromptAst) -> (String, Vec<PromptVariable>) {
    let mut prev_white_space: Option<String> = None;
    let mut post_white_space: Option<String> = None;
    let mut is_comment = false;
    let mut full_prompt_text = String::new();

    let mut replacers = Vec::default();

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
                let replacement = process_code_block(ctx, code_block);
                match replacement {
                    Some(replacement) => {
                        full_prompt_text.push_str(&format!("{}", replacement.key()));
                        replacers.push(replacement);
                    }
                    None => {
                        full_prompt_text.push_str(&format!("{{{}}}", code_block.block.as_str()))
                    }
                }
            }
        }
    }

    (full_prompt_text, replacers)
}

fn process_code_block(ctx: &mut Context<'_>, code_block: &CodeBlock) -> Option<PromptVariable> {
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
        CodeType::Variable => match process_input(ctx, variable) {
            Some(p) => Some(PromptVariable::Input(p)),
            None => None,
        },
        CodeType::PrintEnum => match process_print_enum(ctx, variable) {
            Some(p) => Some(PromptVariable::Enum(p)),
            None => None,
        },
        CodeType::PrintType => match process_print_type(ctx, variable) {
            Some(p) => Some(PromptVariable::Type(p)),
            None => None,
        },
    }
}

fn process_print_enum(ctx: &mut Context<'_>, variable: &Variable) -> Option<Variable> {
    if variable.text == "output" {
        ctx.push_error(DatamodelError::new_validation_error(
            "output can only be used with print_type()",
            variable.span.clone(),
        ));
        return None;
    }

    return Some(variable.clone());
}

fn process_print_type(ctx: &mut Context<'_>, variable: &Variable) -> Option<Variable> {
    return Some(variable.clone());
}

fn process_input(ctx: &mut Context<'_>, variable: &Variable) -> Option<Variable> {
    if variable.path[0] != "input" {
        ctx.push_error(DatamodelError::new_validation_error(
            "Must start with `input`",
            variable.span.clone(),
        ));
        None
    } else {
        Some(variable.clone())
    }
}
