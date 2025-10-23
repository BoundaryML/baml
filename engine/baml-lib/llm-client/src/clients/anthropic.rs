use std::collections::HashSet;

use anyhow::Result;
use baml_derive::BamlHash;
use baml_types::{ApiKeyWithProvenance, EvaluationContext, StringOr, UnresolvedValue};
use indexmap::IndexMap;
use secrecy::SecretString;

use super::helpers::{Error, HttpConfig, PropertyHandler, UnresolvedUrl};
use crate::{
    AllowedRoleMetadata, FinishReasonFilter, MediaUrlHandler, RolesSelection,
    SupportedRequestModes, UnresolvedAllowedRoleMetadata, UnresolvedFinishReasonFilter,
    UnresolvedMediaUrlHandler, UnresolvedRolesSelection,
};

pub const DEFAULT_ANTHROPIC_VERSION: &str = "2023-06-01";
pub const DEFAULT_MAX_TOKENS: u32 = 4096;

#[derive(Debug, Clone, BamlHash)]
pub struct UnresolvedAnthropic<Meta> {
    base_url: UnresolvedUrl,
    api_key: StringOr,
    role_selection: UnresolvedRolesSelection,
    allowed_metadata: UnresolvedAllowedRoleMetadata,
    supported_request_modes: SupportedRequestModes,
    #[baml_safe_hash]
    headers: IndexMap<String, StringOr>,
    #[baml_safe_hash]
    properties: IndexMap<String, (Meta, UnresolvedValue<Meta>)>,
    finish_reason_filter: UnresolvedFinishReasonFilter,
    media_url_handler: UnresolvedMediaUrlHandler,
    http_config: HttpConfig,
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
            media_url_handler: self.media_url_handler.clone(),
            http_config: self.http_config.clone(),
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
    pub media_url_handler: MediaUrlHandler,
    pub http_config: HttpConfig,
}

impl ResolvedAnthropic {
    pub fn allowed_roles(&self) -> Vec<String> {
        self.role_selection.allowed_or_else(|| {
            vec![
                "system".to_string(),
                "user".to_string(),
                "assistant".to_string(),
            ]
        })
    }

    pub fn default_role(&self) -> String {
        self.role_selection.default_or_else(|| {
            let allowed_roles = self.allowed_roles();
            if allowed_roles.contains(&"user".to_string()) {
                "user".to_string()
            } else {
                allowed_roles
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "user".to_string())
            }
        })
    }

    pub fn remap_role(&self) -> std::collections::HashMap<String, String> {
        self.role_selection.remap().unwrap_or_default()
    }

    /// When using Vertex with Anthropic, we need to use a synthetic client that mimics the Anthropic API.
    /// This allows us to construct an Anthropic HTTP client from a baml client for vertex-ai.
    pub fn synthetic_for_vertex_anthropic(role_selection: RolesSelection) -> Self {
        Self {
            headers: IndexMap::new(),
            properties: IndexMap::new(),
            proxy_url: None,
            finish_reason_filter: FinishReasonFilter::All,
            base_url: "BAML-ANTHROPIC-PLACEHOLDER".to_string(),
            api_key: ApiKeyWithProvenance {
                api_key: SecretString::new("".into()),
                provenance: None,
            },
            role_selection,
            allowed_metadata: AllowedRoleMetadata::All,
            supported_request_modes: SupportedRequestModes { stream: Some(true) },
            media_url_handler: MediaUrlHandler::default(),
            http_config: HttpConfig::default(),
        }
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
            .or_insert_with(|| DEFAULT_ANTHROPIC_VERSION.to_string());

        let properties = {
            let mut properties = self
                .properties
                .iter()
                .map(|(k, (_, v))| Ok((k.clone(), v.resolve_serde::<serde_json::Value>(ctx)?)))
                .collect::<Result<IndexMap<_, _>>>()?;

            properties
                .entry("max_tokens".to_string())
                .or_insert(serde_json::json!(DEFAULT_MAX_TOKENS));

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
            media_url_handler: self.media_url_handler.resolve(ctx)?,
            http_config: self.http_config.clone(),
        })
    }

    pub fn create_from(mut properties: PropertyHandler<Meta>) -> Result<Self, Vec<Error<Meta>>> {
        let base_url = properties
            .ensure_base_url_with_default(UnresolvedUrl::new_static("https://api.anthropic.com"));
        let api_key = properties
            .ensure_string("api_key", false)
            .map(|(_, v, _)| v.clone())
            .unwrap_or(StringOr::EnvVar("ANTHROPIC_API_KEY".to_string()));

        let http_config = properties.ensure_http_config("anthropic");

        let role_selection = properties.ensure_roles_selection();
        let allowed_metadata = properties.ensure_allowed_metadata();
        let supported_request_modes = properties.ensure_supported_request_modes();
        let headers = properties.ensure_headers().unwrap_or_default();
        let finish_reason_filter = properties.ensure_finish_reason_filter();
        let media_url_handler = properties.ensure_media_url_handler();
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
            media_url_handler,
            http_config,
        })
    }
}
