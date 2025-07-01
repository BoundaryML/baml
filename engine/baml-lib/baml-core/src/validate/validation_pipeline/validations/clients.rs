use anyhow::Result;
use baml_types::StringOr;
use internal_baml_ast::ast::Expression;
use internal_baml_diagnostics::{DatamodelError, Span};
use internal_llm_client::{ClientProvider, ClientSpec, PropertyHandler, StrategyClientProperty};

use crate::validate::validation_pipeline::context::Context;

pub(super) fn validate(ctx: &mut Context<'_>) {
    let valid_clients = ctx.db.valid_client_names();

    // required props are already validated in visit_client. No other validations here.
    for f in ctx.db.walk_clients() {
        if let Some((retry_policy, span)) = &f.properties().retry_policy {
            if ctx.db.find_retry_policy(retry_policy).is_none() {
                ctx.push_error(DatamodelError::new_type_not_found_error(
                    retry_policy,
                    ctx.db.valid_retry_policy_names(),
                    span.clone(),
                ));
            }
        }

        // Do any additional validation here for providers that need it.
        match &f.properties().options {
            internal_llm_client::UnresolvedClientProperty::OpenAI(_)
            | internal_llm_client::UnresolvedClientProperty::Anthropic(_)
            | internal_llm_client::UnresolvedClientProperty::AWSBedrock(_)
            | internal_llm_client::UnresolvedClientProperty::Vertex(_)
            | internal_llm_client::UnresolvedClientProperty::GoogleAI(_) => {}
            internal_llm_client::UnresolvedClientProperty::RoundRobin(options) => {
                validate_strategy(options, ctx);
            }
            internal_llm_client::UnresolvedClientProperty::Fallback(options) => {
                validate_strategy(options, ctx);
            }
        }
    }
}

fn validate_strategy(options: &impl StrategyClientProperty<Span>, ctx: &mut Context<'_>) {
    let valid_clients = ctx.db.valid_client_names();

    for (client, span) in options.strategy() {
        if let either::Either::Right(ClientSpec::Named(s)) = client {
            if !valid_clients.contains(s) {
                ctx.push_error(DatamodelError::new_client_not_found_error(
                    s,
                    span.clone(),
                    &valid_clients,
                ));
            }
        }
    }
}
