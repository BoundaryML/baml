use internal_baml_diagnostics::{DatamodelError, DatamodelWarning};

use crate::validate::validation_pipeline::context::Context;

pub(super) fn validate(ctx: &mut Context<'_>) {
    // required props are already validated in visit_client. No other validations here.
    ctx.db.walk_clients().for_each(|f| {
        let (provider, span) = &f.properties().provider;
        let allowed_providers = [
            "baml-openai-chat",
            "openai",
            "baml-azure-chat",
            "azure-openai",
            "baml-anthropic-chat",
            "anthropic",
            "baml-ollama-chat",
            "ollama",
            "baml-round-robin",
            "round-robin",
            "baml-fallback",
            "fallback",
            "baml-google-chat",
            "google",
        ];

        let suggestions: Vec<String> = allowed_providers
            .iter()
            .filter(|&&p| !p.starts_with("baml-"))
            .map(|&p| p.to_string())
            .collect();

        if !allowed_providers.contains(&provider.as_str()) {
            ctx.push_error(DatamodelError::not_found_error(
                "client provider",
                provider,
                span.clone(),
                suggestions,
            ));
        }

        log::info!("Validating client: {}", provider.as_str());

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
