use internal_baml_diagnostics::DatamodelError;
use internal_baml_prompt_parser::ast::{CodeBlock, PromptAst, Top, Variable};
use internal_baml_schema_ast::ast::RawString;

use crate::context::Context;

use super::PromptVariable;

/// Function returns the raw_string without any comments.
pub(super) fn validate_prompt(
    ctx: &mut Context<'_>,
    raw_string: &RawString,
) -> Option<(String, Vec<PromptVariable>)> {
    if raw_string.value().is_empty() {
        // Return an empty string if the prompt is empty.
        return Some(Default::default());
    }

    let parsed_prompt =
        internal_baml_prompt_parser::parse_prompt(&ctx.diagnostics.root_path, raw_string);

    match parsed_prompt {
        Ok((ast, d)) => {
            ctx.diagnostics.push(d);
            Some(process_prompt_ast(ctx, ast))
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
                        Some(existing_ws) => Some(format!("{existing_ws}{ws}")),
                        None => Some(ws.to_string()),
                    };
                } else {
                    prev_white_space = match prev_white_space {
                        Some(existing_ws) => Some(format!("{existing_ws}{ws}")),
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

                let raw_string = code_block.as_str();
                let replacement = process_code_block(ctx, code_block.to_owned());
                match replacement {
                    Some(replacement) => {
                        full_prompt_text.push_str(&replacement.key().to_string());
                        replacers.push(replacement);
                    }
                    None => full_prompt_text.push_str(&format!("{{{raw_string}}}")),
                }
            }
        }
    }

    (full_prompt_text, replacers)
}

fn process_code_block(ctx: &mut Context<'_>, code_block: CodeBlock) -> Option<PromptVariable> {
    match code_block {
        CodeBlock::Chat(c) => Some(PromptVariable::Chat(c)),
        CodeBlock::Variable(var) => match process_input(ctx, &var) {
            true => Some(PromptVariable::Input(var)),
            false => None,
        },
        CodeBlock::PrintEnum(blk) => match process_print_enum(ctx, &blk.target) {
            true => Some(PromptVariable::Enum(blk)),
            false => None,
        },
        CodeBlock::PrintType(blk) => match process_print_type(ctx, &blk.target) {
            true => Some(PromptVariable::Type(blk)),
            false => None,
        },
    }
}

fn process_print_enum(ctx: &mut Context<'_>, variable: &Variable) -> bool {
    if variable.text == "output" {
        ctx.push_error(DatamodelError::new_validation_error(
            "output can only be used with print_type()",
            variable.span.clone(),
        ));
        false
    } else {
        true
    }
}

fn process_print_type(_ctx: &mut Context<'_>, _variable: &Variable) -> bool {
    true
}

fn process_input(ctx: &mut Context<'_>, variable: &Variable) -> bool {
    if variable.path[0] != "input" {
        ctx.push_error(DatamodelError::new_validation_error(
            "Must start with `input`",
            variable.span.clone(),
        ));
        false
    } else {
        true
    }
}
