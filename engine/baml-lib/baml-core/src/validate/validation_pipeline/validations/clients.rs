use internal_baml_diagnostics::{DatamodelError, DatamodelWarning};

use crate::validate::validation_pipeline::context::Context;

pub(super) fn validate(ctx: &mut Context<'_>) {
    // required props are already validated in visit_client. No other validations here.
    ctx.db.walk_clients().for_each(|f| {
        let (provider, span) = &f.properties().provider;
        let allowed_providers = [
            "baml-openai-chat",
            "openai",
            "baml-anthropic-chat",
            "anthropic",
            "baml-ollama-chat",
            "ollama",
            "baml-round-robin",
            "round-robin",
            "baml-fallback",
            "fallback",
        ];

        let suggestions: Vec<&str> = allowed_providers
            .iter()
            .filter(|&&p| !p.starts_with("baml-"))
            .cloned()
            .collect();

        if !allowed_providers.contains(&provider.as_str()) {
            ctx.push_warning(DatamodelWarning::new(
                format!(
                    "Unsupported provider: {}. Available ones are: {}",
                    provider,
                    suggestions.join(", ")
                ),
                span.clone(),
            ));
        }

        if let Some((retry_policy, span)) = &f.properties().retry_policy {
            if ctx.db.find_retry_policy(retry_policy).is_none() {
                ctx.push_error(DatamodelError::new_type_not_found_error(
                    retry_policy,
                    ctx.db.valid_retry_policy_names(),
                    span.clone(),
                ))
            }
        }
    })
}
