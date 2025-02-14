use std::collections::HashSet;

use crate::{AllowedRoleMetadata, FinishReasonFilter, RolesSelection, SupportedRequestModes, UnresolvedAllowedRoleMetadata, UnresolvedFinishReasonFilter, UnresolvedRolesSelection};
use anyhow::Result;

use baml_types::{ApiKeyWithProvenance, EvaluationContext, StringOr, UnresolvedValue};
use indexmap::IndexMap;

use super::helpers::{Error, PropertyHandler, UnresolvedUrl};

#[derive(Debug)]
pub struct UnresolvedAnthropic<Meta> {
    base_url: UnresolvedUrl,
    api_key: StringOr,
    role_selection: UnresolvedRolesSelection,
    allowed_metadata: UnresolvedAllowedRoleMetadata,
    supported_request_modes: SupportedRequestModes,
    headers: IndexMap<String, StringOr>,
    properties: IndexMap<String, (Meta, UnresolvedValue<Meta>)>,
    finish_reason_filter: UnresolvedFinishReasonFilter,
}

impl<Meta> UnresolvedAnthropic<Meta> {
    pub fn without_meta(&self) -> UnresolvedAnthropic<()> {
        UnresolvedAnthropic {
            base_url: self.base_url.clone(),
            api_key: self.api_key.clone(),
            role_selection: self.role_selection.clone(),
            allowed_metadata: self.allowed_metadata.clone(),
            supported_request_modes: self.supported_request_modes.clone(),
            headers: self
                .headers
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            properties: self
                .properties
                .iter()
                .map(|(k, (_, v))| (k.clone(), ((), v.without_meta())))
                .collect(),
            finish_reason_filter: self.finish_reason_filter.clone(),
        }
    }
}

pub struct ResolvedAnthropic {
    pub base_url: String,
    pub api_key: ApiKeyWithProvenance,
    role_selection: RolesSelection,
    pub allowed_metadata: AllowedRoleMetadata,
    pub supported_request_modes: SupportedRequestModes,
    pub headers: IndexMap<String, String>,
    pub properties: IndexMap<String, serde_json::Value>,
    pub proxy_url: Option<String>,
    pub finish_reason_filter: FinishReasonFilter,
}

impl ResolvedAnthropic {
    pub fn allowed_roles(&self) -> Vec<String> {
        self.role_selection.allowed_or_else(|| {
            vec!["system".to_string(), "user".to_string(), "assistant".to_string()]
        })
    }

    pub fn default_role(&self) -> String {
        self.role_selection
            .default_or_else(|| {
                let allowed_roles = self.allowed_roles();
                if allowed_roles.contains(&"user".to_string()) {
                    "user".to_string()
                } else {
                    allowed_roles.first().cloned().unwrap_or_else(|| "user".to_string())
                }
            })
    }
}


impl<Meta: Clone> UnresolvedAnthropic<Meta> {
    pub fn required_env_vars(&self) -> HashSet<String> {
        let mut env_vars = HashSet::new();
        env_vars.extend(self.base_url.required_env_vars());
        env_vars.extend(self.api_key.required_env_vars());
        env_vars.extend(self.role_selection.required_env_vars());
        env_vars.extend(self.allowed_metadata.required_env_vars());
        env_vars.extend(self.supported_request_modes.required_env_vars());
        env_vars.extend(self.headers.values().flat_map(|v| v.required_env_vars()));
        env_vars.extend(
            self.properties
                .values()
                .flat_map(|(_, v)| v.required_env_vars()),
        );

        env_vars
    }

    pub fn resolve(&self, ctx: &EvaluationContext<'_>) -> Result<ResolvedAnthropic> {
        let base_url = self.base_url.resolve(ctx)?;

        let mut headers = self
            .headers
            .iter()
            .map(|(k, v)| Ok((k.clone(), v.resolve(ctx)?)))
            .collect::<Result<IndexMap<_, _>>>()?;

        // Add default Anthropic version header if not present
        headers
            .entry("anthropic-version".to_string())
            .or_insert_with(|| "2023-06-01".to_string());

        let properties = {
            let mut properties = self
                .properties
                .iter()
                .map(|(k, (_, v))| Ok((k.clone(), v.resolve_serde::<serde_json::Value>(ctx)?)))
                .collect::<Result<IndexMap<_, _>>>()?;

            properties
                .entry("max_tokens".to_string())
                .or_insert(serde_json::json!(4096));

            properties
        };

        Ok(ResolvedAnthropic {
            base_url,
            api_key: self.api_key.resolve_api_key(ctx)?,
            role_selection: self.role_selection.resolve(ctx)?,
            allowed_metadata: self.allowed_metadata.resolve(ctx)?,
            supported_request_modes: self.supported_request_modes.clone(),
            headers,
            properties,
            proxy_url: super::helpers::get_proxy_url(ctx),
            finish_reason_filter: self.finish_reason_filter.resolve(ctx)?,
        })
    }

    pub fn create_from(mut properties: PropertyHandler<Meta>) -> Result<Self, Vec<Error<Meta>>> {
        let base_url = properties
            .ensure_base_url_with_default(UnresolvedUrl::new_static("https://api.anthropic.com"));
        let api_key = properties
            .ensure_string("api_key", false)
            .map(|(_, v, _)| v.clone())
            .unwrap_or(StringOr::EnvVar("ANTHROPIC_API_KEY".to_string()));

        let role_selection = properties.ensure_roles_selection();
        let allowed_metadata = properties.ensure_allowed_metadata();
        let supported_request_modes = properties.ensure_supported_request_modes();
        let headers = properties.ensure_headers().unwrap_or_default();
        let finish_reason_filter = properties.ensure_finish_reason_filter();
        let (properties, errors) = properties.finalize();
        if !errors.is_empty() {
            return Err(errors);
        }

        Ok(Self {
            base_url,
            api_key,
            role_selection,
            allowed_metadata,
            supported_request_modes,
            headers,
            properties,
            finish_reason_filter,
        })
    }
}
