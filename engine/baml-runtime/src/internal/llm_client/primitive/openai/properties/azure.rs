use std::collections::HashMap;

use anyhow::{Context, Result};

use crate::{
    internal::llm_client::{properties_hander::PropertiesHandler, AllowedMetadata},
    RuntimeContext,
};

use super::PostRequestProperties;

pub fn resolve_properties(
    mut properties: PropertiesHandler,
    ctx: &RuntimeContext,
) -> Result<PostRequestProperties> {
    // POST https://{your-resource-name}.openai.azure.com/openai/deployments/{deployment-id}/chat/completions?api-version={api-version}

    let default_role = properties.pull_default_role("system")?;
    let allowed_metadata = properties.pull_allowed_role_metadata()?;

    let base_url = properties.pull_base_url()?;
    let resource_name = properties.remove_str("resource_name")?;
    let deployment_id = properties.remove_str("deployment_id")?;
    let api_version = properties.remove_str("api_version")?;

    // Ensure that either (resource_name, deployment_id) or base_url is provided
    let base_url = match (base_url, resource_name, deployment_id) {
        (Some(base_url), None, None) => base_url,
        (None, Some(resource_name), Some(deployment_id)) => {
            format!("https://{resource_name}.openai.azure.com/openai/deployments/{deployment_id}")
        }
        _ => {
            anyhow::bail!("Either base_url or both (resource_name, deployment_id) must be provided")
        }
    };

    let api_key = properties
        .pull_api_key()?
        .or_else(|| ctx.env.get("AZURE_OPENAI_API_KEY").map(|s| s.to_string()));
    let mut headers = properties.pull_headers()?;
    if let Some(api_key) = &api_key {
        headers.insert("API-KEY".to_string(), api_key.clone());
    }
    let headers = headers;

    let mut query_params = HashMap::new();
    if let Some(v) = api_version {
        query_params.insert("api-version".to_string(), v.to_string());
    };

    let finish_reason = properties.pull_finish_reason_options()?;

    let mut properties = properties.finalize();
    properties
        .entry("max_tokens".into())
        .or_insert_with(|| 4096.into());
    let properties = properties;

    Ok(PostRequestProperties {
        default_role,
        base_url,
        api_key: None,
        headers,
        properties,
        allowed_metadata,
        finish_reason,
        // Replace proxy_url with code below to disable proxying
        // proxy_url: None,
        proxy_url: ctx.env.get("BOUNDARY_PROXY_URL").map(|s| s.to_string()),
        query_params,
    })
}
