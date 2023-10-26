use internal_baml_diagnostics::{DatamodelError, Span};

use crate::{ast::WithSpan, validate::validation_pipeline::context::Context};
use internal_baml_prompt_parser::ast::{Top, TopId};

pub(super) fn validate(ctx: &mut Context<'_>) {
    for variant in ctx.db.walk_variants() {
        if variant.walk_function().is_none() {
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
            &variant.properties().prompt,
            &variant.ast_variant().span(),
        );
    }
}

fn validate_prompt(ctx: &mut Context<'_>, prompt: &str, span: &Span) {
    if prompt.is_empty() {
        ctx.push_error(DatamodelError::new_validation_error(
            "Prompt cannot be empty",
            span.clone(),
        ));
    }
    let validated_prompt =
        internal_baml_prompt_parser::parse_prompt(&ctx.diagnostics.root_path, &span.file, prompt);
    println!("prompt:");
    match validated_prompt {
        Ok((ast, _)) => {
            for (top_id, top) in ast.iter_tops() {
                match (top_id, top) {
                    (TopId::PromptText(_), Top::PromptText(prompt)) => {
                        print!("{}", prompt.text);
                    }
                    (TopId::CodeBlock(_), Top::CodeBlock(code_block)) => {
                        print!("`{}`", code_block.block);
                    }
                    _ => {
                        // print!("{:?}", top_id);
                    }
                }
            }
        }
        Err(diagnostics) => {
            println!("error {:?}", diagnostics.to_pretty_string());
        }
    }
}
