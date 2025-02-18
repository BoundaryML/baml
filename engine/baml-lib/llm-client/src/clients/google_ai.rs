use std::collections::HashSet;

use crate::{AllowedRoleMetadata, SupportedRequestModes, UnresolvedAllowedRoleMetadata};
use crate::{
    FinishReasonFilter, RolesSelection, UnresolvedFinishReasonFilter, UnresolvedRolesSelection,
};
use anyhow::Result;

use baml_types::{ApiKeyWithProvenance, EvaluationContext, StringOr, UnresolvedValue};
use indexmap::IndexMap;

use super::helpers::{Error, PropertyHandler, UnresolvedUrl};

#[derive(Debug, Clone)]
pub struct UnresolvedGoogleAI<Meta> {
    api_key: StringOr,
    base_url: UnresolvedUrl,
    headers: IndexMap<String, StringOr>,
    role_selection: UnresolvedRolesSelection,
    model: Option<StringOr>,
    allowed_metadata: UnresolvedAllowedRoleMetadata,
    supported_request_modes: SupportedRequestModes,
    finish_reason_filter: UnresolvedFinishReasonFilter,
    properties: IndexMap<String, (Meta, UnresolvedValue<Meta>)>,
}

impl<Meta> UnresolvedGoogleAI<Meta> {
    pub fn without_meta(&self) -> UnresolvedGoogleAI<()> {
        UnresolvedGoogleAI {
            role_selection: self.role_selection.clone(),
            api_key: self.api_key.clone(),
            model: self.model.clone(),
            base_url: self.base_url.clone(),
            headers: self
                .headers
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            allowed_metadata: self.allowed_metadata.clone(),
            supported_request_modes: self.supported_request_modes.clone(),
            properties: self
                .properties
                .iter()
                .map(|(k, (_, v))| (k.clone(), ((), v.without_meta())))
                .collect::<IndexMap<_, _>>(),
            finish_reason_filter: self.finish_reason_filter.clone(),
        }
    }
}

pub struct ResolvedGoogleAI {
    role_selection: RolesSelection,
    pub api_key: ApiKeyWithProvenance,
    pub model: String,
    pub base_url: String,
    pub headers: IndexMap<String, String>,
    pub allowed_metadata: AllowedRoleMetadata,
    pub supported_request_modes: SupportedRequestModes,
    pub properties: IndexMap<String, serde_json::Value>,
    pub proxy_url: Option<String>,
    pub finish_reason_filter: FinishReasonFilter,
}

impl ResolvedGoogleAI {
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
}

impl<Meta: Clone> UnresolvedGoogleAI<Meta> {
    pub fn required_env_vars(&self) -> HashSet<String> {
        let mut env_vars = HashSet::new();
        env_vars.extend(self.api_key.required_env_vars());
        env_vars.extend(self.base_url.required_env_vars());
        env_vars.extend(self.headers.values().flat_map(StringOr::required_env_vars));
        if let Some(m) = self.model.as_ref() {
            env_vars.extend(m.required_env_vars())
        }
        env_vars.extend(self.role_selection.required_env_vars());
        env_vars.extend(self.allowed_metadata.required_env_vars());
        env_vars.extend(self.supported_request_modes.required_env_vars());
        env_vars.extend(
            self.properties
                .values()
                .flat_map(|(_, v)| v.required_env_vars()),
        );
        env_vars
    }

    pub fn resolve(&self, ctx: &EvaluationContext<'_>) -> Result<ResolvedGoogleAI> {
        let api_key = self.api_key.resolve_api_key(ctx)?;
        let role_selection = self.role_selection.resolve(ctx)?;

        let model = self
            .model
            .as_ref()
            .map(|m| m.resolve(ctx))
            .transpose()?
            .unwrap_or_else(|| "gemini-1.5-flash".to_string());

        let base_url = self.base_url.resolve(ctx)?;

        let headers = self
            .headers
            .iter()
            .map(|(k, v)| Ok((k.clone(), v.resolve(ctx)?)))
            .collect::<Result<IndexMap<_, _>>>()?;

        Ok(ResolvedGoogleAI {
            role_selection,
            api_key,
            model,
            base_url,
            headers,
            allowed_metadata: self.allowed_metadata.resolve(ctx)?,
            supported_request_modes: self.supported_request_modes.clone(),
            properties: self
                .properties
                .iter()
                .map(|(k, (_, v))| Ok((k.clone(), v.resolve_serde::<serde_json::Value>(ctx)?)))
                .collect::<Result<IndexMap<_, _>>>()?,
            proxy_url: super::helpers::get_proxy_url(ctx),
            finish_reason_filter: self.finish_reason_filter.resolve(ctx)?,
        })
    }

    pub fn create_from(mut properties: PropertyHandler<Meta>) -> Result<Self, Vec<Error<Meta>>> {
        let role_selection = properties.ensure_roles_selection();
        let api_key = properties
            .ensure_api_key()
            .map(|v| v.clone())
            .unwrap_or(StringOr::EnvVar("GOOGLE_API_KEY".to_string()));

        let model = properties
            .ensure_string("model", false)
            .map(|(_, v, _)| v.clone());

        let base_url = properties.ensure_base_url_with_default(UnresolvedUrl::new_static(
            "https://generativelanguage.googleapis.com/v1beta",
        ));

        let allowed_metadata = properties.ensure_allowed_metadata();
        let supported_request_modes = properties.ensure_supported_request_modes();
        let headers = properties.ensure_headers().unwrap_or_default();
        let finish_reason_filter = properties.ensure_finish_reason_filter();
        let (properties, errors) = properties.finalize();

        if !errors.is_empty() {
            return Err(errors);
        }

        Ok(Self {
            role_selection,
            api_key,
            model,
            base_url,
            headers,
            allowed_metadata,
            supported_request_modes,
            properties,
            finish_reason_filter,
        })
    }
}
