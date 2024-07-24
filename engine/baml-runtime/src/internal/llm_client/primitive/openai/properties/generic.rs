use std::collections::HashMap;

use anyhow::{Context, Result};

use crate::RuntimeContext;

use super::PostRequestProperties;

pub fn resolve_properties(
    mut properties: HashMap<String, serde_json::Value>,
    ctx: &RuntimeContext,
) -> Result<PostRequestProperties> {
    let default_role = properties
        .remove("default_role")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "system".to_string());

    let base_url = properties
        .remove("base_url")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .context("When using 'openai-generic', you must specify a base_url")?;

    let headers = properties.remove("headers").map(|v| {
        if let Some(v) = v.as_object() {
            v.iter()
                .map(|(k, v)| {
                    Ok((
                        k.to_string(),
                        match v {
                            serde_json::Value::String(s) => s.to_string(),
                            _ => anyhow::bail!("Header '{k}' must be a string"),
                        },
                    ))
                })
                .collect::<Result<HashMap<String, String>>>()
        } else {
            Ok(Default::default())
        }
    });
    let headers = match headers {
        Some(h) => h?,
        None => Default::default(),
    };

    Ok(PostRequestProperties {
        default_role,
        base_url,
        api_key: None,
        headers,
        properties,
        proxy_url: ctx
            .env
            .get("BOUNDARY_PROXY_URL")
            .map(|s| Some(s.to_string()))
            .unwrap_or(None),
        query_params: Default::default(),
    })
}
