use std::collections::HashSet;

use crate::{
    AllowedRoleMetadata, FinishReasonFilter, RolesSelection, SupportedRequestModes,
    UnresolvedAllowedRoleMetadata, UnresolvedFinishReasonFilter, UnresolvedRolesSelection,
};
use anyhow::Result;

use baml_types::{ApiKeyWithProvenance, GetEnvVar, StringOr, UnresolvedValue};
use indexmap::IndexMap;

use super::helpers::{Error, PropertyHandler, UnresolvedUrl};

#[derive(Debug)]
pub struct UnresolvedOpenAI<Meta> {
    base_url: Option<either::Either<UnresolvedUrl, (StringOr, StringOr)>>,
    api_key: Option<StringOr>,
    role_selection: UnresolvedRolesSelection,
    allowed_role_metadata: UnresolvedAllowedRoleMetadata,
    supported_request_modes: SupportedRequestModes,
    headers: IndexMap<String, StringOr>,
    properties: IndexMap<String, (Meta, UnresolvedValue<Meta>)>,
    query_params: IndexMap<String, StringOr>,
    finish_reason_filter: UnresolvedFinishReasonFilter,
}

impl<Meta> UnresolvedOpenAI<Meta> {
    pub fn without_meta(&self) -> UnresolvedOpenAI<()> {
        UnresolvedOpenAI {
            base_url: self.base_url.clone(),
            api_key: self.api_key.clone(),
            role_selection: self.role_selection.clone(),
            allowed_role_metadata: self.allowed_role_metadata.clone(),
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
                .collect::<IndexMap<_, _>>(),
            query_params: self
                .query_params
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            finish_reason_filter: self.finish_reason_filter.clone(),
        }
    }
}

pub struct ResolvedOpenAI {
    pub base_url: String,
    pub api_key: Option<ApiKeyWithProvenance>,
    role_selection: RolesSelection,
    pub allowed_metadata: AllowedRoleMetadata,
    supported_request_modes: SupportedRequestModes,
    pub headers: IndexMap<String, String>,
    pub properties: IndexMap<String, serde_json::Value>,
    pub query_params: IndexMap<String, String>,
    pub proxy_url: Option<String>,
    pub finish_reason_filter: FinishReasonFilter,
}

impl ResolvedOpenAI {
    fn is_o1_model(&self) -> bool {
        self.properties.get("model").is_some_and(|model| {
            model
                .as_str()
                .map(|s| s.starts_with("o1-") || s.eq("o1"))
                .unwrap_or(false)
        })
    }

    pub fn supports_streaming(&self) -> bool {
        match self.supported_request_modes.stream {
            Some(v) => v,
            None => !self.is_o1_model(),
        }
    }

    pub fn allowed_roles(&self) -> Vec<String> {
        self.role_selection.allowed_or_else(|| {
            if self.is_o1_model() {
                vec!["user".to_string(), "assistant".to_string()]
            } else {
                vec![
                    "system".to_string(),
                    "user".to_string(),
                    "assistant".to_string(),
                ]
            }
        })
    }

    pub fn default_role(&self) -> String {
        self.role_selection.default_or_else(|| {
            // TODO: guard against empty allowed_roles
            // The compiler should already guarantee that this is non-empty
            self.allowed_roles().remove(0)
        })
    }
}

impl<Meta: Clone> UnresolvedOpenAI<Meta> {
    pub fn required_env_vars(&self) -> HashSet<String> {
        let mut env_vars = HashSet::new();

        if let Some(url) = self.base_url.as_ref() {
            match url {
                either::Either::Left(url) => {
                    env_vars.extend(url.required_env_vars());
                }
                either::Either::Right((resource_name, deployment_id)) => {
                    env_vars.extend(resource_name.required_env_vars());
                    env_vars.extend(deployment_id.required_env_vars());
                }
            }
        };
        if let Some(key) = self.api_key.as_ref() {
            env_vars.extend(key.required_env_vars())
        }
        env_vars.extend(self.role_selection.required_env_vars());
        env_vars.extend(self.allowed_role_metadata.required_env_vars());
        env_vars.extend(self.supported_request_modes.required_env_vars());
        self.headers
            .iter()
            .for_each(|(_, v)| env_vars.extend(v.required_env_vars()));
        self.properties
            .iter()
            .for_each(|(_, (_, v))| env_vars.extend(v.required_env_vars()));
        self.query_params
            .iter()
            .for_each(|(_, v)| env_vars.extend(v.required_env_vars()));

        env_vars
    }

    pub fn resolve(
        &self,
        provider: &crate::ClientProvider,
        ctx: &impl GetEnvVar,
    ) -> Result<ResolvedOpenAI> {
        let base_url = self
            .base_url
            .as_ref()
            .map(|url| match url {
                either::Either::Left(url) => url.resolve(ctx),
                either::Either::Right((resource_name, deployment_id)) => {
                    let resource_name = resource_name.resolve(ctx)?;
                    let deployment_id = deployment_id.resolve(ctx)?;
                    Ok(format!(
                        "https://{resource_name}.openai.azure.com/openai/deployments/{deployment_id}"
                    ))
                }
            })
            .transpose()?;

        let Some(base_url) = base_url else {
            return Err(anyhow::anyhow!("base_url is required"));
        };

        let api_key = self
            .api_key
            .as_ref()
            .map(|key| key.resolve_api_key(ctx))
            .transpose()?;

        let role_selection = self.role_selection.resolve(ctx)?;

        let headers = self
            .headers
            .iter()
            .map(|(k, v)| Ok((k.clone(), v.resolve(ctx)?)))
            .collect::<Result<IndexMap<_, _>>>()?;

        let properties = {
            let mut properties = self
                .properties
                .iter()
                .map(|(k, (_, v))| Ok((k.clone(), v.resolve_serde::<serde_json::Value>(ctx)?)))
                .collect::<Result<IndexMap<_, _>>>()?;

            // Set default max_tokens for Azure OpenAI if:
            // 1. It's an Azure client
            // 2. max_completion_tokens is not set
            // 3. max_tokens is not present
            if matches!(
                provider,
                crate::ClientProvider::OpenAI(crate::OpenAIClientProviderVariant::Azure)
            ) {
                if !properties.contains_key("max_completion_tokens")
                    && !properties.contains_key("max_tokens")
                {
                    properties.insert("max_tokens".into(), serde_json::json!(4096));
                } else if properties.get("max_tokens").map_or(false, |v| v.is_null()) {
                    properties.shift_remove("max_tokens");
                }
            }

            properties
        };

        let query_params = self
            .query_params
            .iter()
            .map(|(k, v)| Ok((k.clone(), v.resolve(ctx)?)))
            .collect::<Result<IndexMap<_, _>>>()?;

        Ok(ResolvedOpenAI {
            base_url,
            api_key,
            role_selection,
            allowed_metadata: self.allowed_role_metadata.resolve(ctx)?,
            supported_request_modes: self.supported_request_modes.clone(),
            headers,
            properties,
            query_params,
            proxy_url: super::helpers::get_proxy_url(ctx),
            finish_reason_filter: self.finish_reason_filter.resolve(ctx)?,
        })
    }

    pub fn create_standard(
        mut properties: PropertyHandler<Meta>,
    ) -> Result<Self, Vec<Error<Meta>>> {
        let base_url = properties
            .ensure_base_url_with_default(UnresolvedUrl::new_static("https://api.openai.com/v1"));

        let api_key = Some(
            properties
                .ensure_api_key()
                .unwrap_or_else(|| StringOr::EnvVar("OPENAI_API_KEY".to_string())),
        );

        Self::create_common(properties, Some(either::Either::Left(base_url)), api_key)
    }

    pub fn create_azure(mut properties: PropertyHandler<Meta>) -> Result<Self, Vec<Error<Meta>>> {
        let base_url = {
            let base_url = properties.ensure_base_url(false);
            let resource_name = properties
                .ensure_string("resource_name", false)
                .map(|(key_span, v, _)| (key_span, v.clone()));
            let deployment_id = properties
                .ensure_string("deployment_id", false)
                .map(|(key_span, v, _)| (key_span, v.clone()));

            match (base_url, resource_name, deployment_id) {
                (Some(url), None, None) => Some(either::Either::Left(url.1)),
                (None, Some(name), Some(id)) => Some(either::Either::Right((name.1, id.1))),
                (_, None, Some((key_span, _))) => {
                    properties.push_error(
                        "resource_name must be provided when deployment_id is provided",
                        key_span,
                    );
                    None
                }
                (_, Some((key_span, _)), None) => {
                    properties.push_error(
                        "deployment_id must be provided when resource_name is provided",
                        key_span,
                    );
                    None
                }
                (Some((key_1_span, ..)), Some((key_2_span, _)), Some((key_3_span, _))) => {
                    for key in [key_1_span, key_2_span, key_3_span] {
                        properties.push_error(
                            "Only one of base_url or both (resource_name, deployment_id) must be provided",
                            key
                        );
                    }
                    None
                }
                (None, None, None) => {
                    properties.push_option_error(
                        "Missing either base_url or both (resource_name, deployment_id)",
                    );
                    None
                }
            }
        };

        let api_key = properties
            .ensure_api_key()
            .map(|v| v.clone())
            .unwrap_or_else(|| StringOr::EnvVar("AZURE_OPENAI_API_KEY".to_string()));

        let mut query_params = IndexMap::new();
        if let Some((_, v, _)) = properties.ensure_string("api_version", false) {
            query_params.insert("api-version".to_string(), v.clone());
        }

        let mut instance = Self::create_common(properties, base_url, None)?;
        instance.query_params = query_params;
        instance
            .headers
            .entry("api-key".to_string())
            .or_insert(api_key);

        Ok(instance)
    }

    pub fn create_generic(mut properties: PropertyHandler<Meta>) -> Result<Self, Vec<Error<Meta>>> {
        let base_url = properties.ensure_base_url(true);

        let api_key = properties.ensure_api_key();

        Self::create_common(
            properties,
            base_url.map(|url| either::Either::Left(url.1)),
            api_key,
        )
    }

    pub fn create_ollama(mut properties: PropertyHandler<Meta>) -> Result<Self, Vec<Error<Meta>>> {
        let base_url = properties
            .ensure_base_url_with_default(UnresolvedUrl::new_static("http://localhost:11434/v1"));

        let api_key = properties.ensure_api_key();

        let mut instance =
            Self::create_common(properties, Some(either::Either::Left(base_url)), api_key)?;
        // Ollama uses smaller models many of which prefer the user role
        if instance.role_selection.default.is_none() {
            instance.role_selection.default = Some(StringOr::Value("user".to_string()));
        }

        Ok(instance)
    }

    fn create_common(
        mut properties: PropertyHandler<Meta>,
        base_url: Option<either::Either<UnresolvedUrl, (StringOr, StringOr)>>,
        api_key: Option<StringOr>,
    ) -> Result<Self, Vec<Error<Meta>>> {
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
            allowed_role_metadata: allowed_metadata,
            supported_request_modes,
            headers,
            properties,
            query_params: IndexMap::new(),
            finish_reason_filter,
        })
    }
}
