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
    let default_role = properties.pull_default_role("system")?;
    let base_url = properties
        .pull_base_url()?
        .unwrap_or_else(|| "http://localhost:11434/v1".to_string());
    let allowed_metadata = properties.pull_allowed_role_metadata()?;
    let headers = properties.pull_headers()?;
    let finish_reason = properties.pull_finish_reason_options()?;

    Ok(PostRequestProperties {
        default_role,
        base_url,
        api_key: None,
        headers,
        properties: properties.finalize(),
        allowed_metadata,
        finish_reason,
        proxy_url: ctx
            .env
            .get("BOUNDARY_PROXY_URL")
            .map(|s| Some(s.to_string()))
            .unwrap_or(None),
        query_params: Default::default(),
    })
}
