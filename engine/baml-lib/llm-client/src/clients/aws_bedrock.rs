use std::collections::HashSet;

use anyhow::Result;
use baml_derive::BamlHash;
use baml_types::{ApiKeyWithProvenance, EvaluationContext, GetEnvVar, StringOr, UnresolvedValue};
use indexmap::IndexMap;
use secrecy::SecretString;
use serde::Serialize;
use serde_json::Value;

use super::helpers::{Error, HttpConfig, PropertyHandler};
use crate::{
    AllowedRoleMetadata, FinishReasonFilter, MediaUrlHandler, RolesSelection,
    SupportedRequestModes, UnresolvedAllowedRoleMetadata, UnresolvedFinishReasonFilter,
    UnresolvedMediaUrlHandler, UnresolvedRolesSelection,
};

#[derive(Debug, Clone, BamlHash)]
pub struct UnresolvedAwsBedrock<Meta> {
    model: Option<StringOr>,
    region: Option<StringOr>,
    access_key_id: Option<StringOr>,
    secret_access_key: Option<StringOr>,
    session_token: Option<StringOr>,
    profile: Option<StringOr>,
    endpoint_url: Option<StringOr>,
    role_selection: UnresolvedRolesSelection,
    allowed_role_metadata: UnresolvedAllowedRoleMetadata,
    supported_request_modes: SupportedRequestModes,
    inference_config: Option<UnresolvedInferenceConfiguration>,
    finish_reason_filter: UnresolvedFinishReasonFilter,
    #[baml_safe_hash]
    additional_model_request_fields: IndexMap<String, (Meta, UnresolvedValue<Meta>)>,
    media_url_handler: UnresolvedMediaUrlHandler,
    http_config: HttpConfig,
}

#[derive(Debug, Clone, BamlHash)]
struct UnresolvedInferenceConfiguration {
    max_tokens: Option<i32>,
    #[baml_safe_hash]
    temperature: Option<f32>,
    #[baml_safe_hash]
    top_p: Option<f32>,
    stop_sequences: Option<Vec<StringOr>>,
}

impl UnresolvedInferenceConfiguration {
    pub fn resolve(&self, ctx: &EvaluationContext<'_>) -> Result<InferenceConfiguration> {
        Ok(InferenceConfiguration {
            max_tokens: self.max_tokens,
            temperature: self.temperature,
            top_p: self.top_p,
            stop_sequences: self
                .stop_sequences
                .as_ref()
                .map(|s| s.iter().map(|s| s.resolve(ctx)).collect::<Result<Vec<_>>>())
                .transpose()?,
        })
    }

    pub fn required_env_vars(&self) -> HashSet<String> {
        self.stop_sequences
            .as_ref()
            .map(|s| s.iter().flat_map(|s| s.required_env_vars()).collect())
            .unwrap_or_default()
    }
}

#[derive(Debug, Serialize)]
pub struct InferenceConfiguration {
    pub max_tokens: Option<i32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub stop_sequences: Option<Vec<String>>,
}

pub struct ResolvedAwsBedrock {
    pub model: String,
    pub region: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<ApiKeyWithProvenance>,
    pub session_token: Option<String>,
    pub profile: Option<String>,
    pub endpoint_url: Option<String>,
    pub inference_config: Option<InferenceConfiguration>,
    role_selection: RolesSelection,
    pub allowed_role_metadata: AllowedRoleMetadata,
    pub supported_request_modes: SupportedRequestModes,
    pub finish_reason_filter: FinishReasonFilter,
    pub additional_model_request_fields: IndexMap<String, Value>,
    pub media_url_handler: MediaUrlHandler,
    pub http_config: HttpConfig,
}

impl std::fmt::Debug for ResolvedAwsBedrock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedAwsBedrock")
            .field("model", &self.model)
            .field("region", &self.region)
            .field("access_key_id", &self.access_key_id)
            .field("secret_access_key", &"<no-repr-available>")
            .field("session_token", &self.session_token)
            .field("profile", &self.profile)
            .field("endpoint_url", &self.endpoint_url)
            .field("inference_config", &"<no-repr-available>")
            .field("role_selection", &self.role_selection)
            .field("allowed_role_metadata", &self.allowed_role_metadata)
            .field("supported_request_modes", &self.supported_request_modes)
            .field("finish_reason_filter", &self.finish_reason_filter)
            .field(
                "additional_model_request_fields",
                &self.additional_model_request_fields,
            )
            .finish()
    }
}

impl ResolvedAwsBedrock {
    pub fn client_options(&self) -> IndexMap<String, serde_json::Value> {
        let mut options = indexmap::IndexMap::new();
        options.insert(
            "model".to_string(),
            serde_json::Value::String(self.model.clone()),
        );
        if let Some(region) = &self.region {
            options.insert(
                "region".to_string(),
                serde_json::Value::String(region.clone()),
            );
        }
        if let Some(endpoint_url) = &self.endpoint_url {
            options.insert(
                "endpoint_url".to_string(),
                serde_json::Value::String(endpoint_url.clone()),
            );
        }
        options
    }

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
}

impl<Meta: Clone> UnresolvedAwsBedrock<Meta> {
    pub fn without_meta(&self) -> UnresolvedAwsBedrock<()> {
        UnresolvedAwsBedrock {
            model: self.model.clone(),
            region: self.region.clone(),
            access_key_id: self.access_key_id.clone(),
            secret_access_key: self.secret_access_key.clone(),
            session_token: self.session_token.clone(),
            profile: self.profile.clone(),
            endpoint_url: self.endpoint_url.clone(),
            role_selection: self.role_selection.clone(),
            allowed_role_metadata: self.allowed_role_metadata.clone(),
            supported_request_modes: self.supported_request_modes.clone(),
            inference_config: self.inference_config.clone(),
            finish_reason_filter: self.finish_reason_filter.clone(),
            additional_model_request_fields: self
                .additional_model_request_fields
                .iter()
                .map(|(k, (_, v))| (k.clone(), ((), v.without_meta())))
                .collect::<IndexMap<_, _>>(),
            media_url_handler: self.media_url_handler.clone(),
            http_config: self.http_config.clone(),
        }
    }
}

impl<Meta: Clone> UnresolvedAwsBedrock<Meta> {
    #[allow(clippy::single_match)]
    pub fn required_env_vars(&self) -> HashSet<String> {
        let mut env_vars = HashSet::new();
        if let Some(m) = self.model.as_ref() {
            env_vars.extend(m.required_env_vars())
        }

        match self.region.as_ref() {
            Some(region) => env_vars.extend(region.required_env_vars()),
            None => {
                #[cfg(target_arch = "wasm32")]
                env_vars.insert("AWS_REGION".into());
            }
        }

        match self.access_key_id.as_ref() {
            Some(access_key_id) => env_vars.extend(access_key_id.required_env_vars()),
            None => {}
        }

        match self.secret_access_key.as_ref() {
            Some(secret_access_key) => env_vars.extend(secret_access_key.required_env_vars()),
            None => {}
        }

        match self.session_token.as_ref() {
            Some(session_token) => env_vars.extend(session_token.required_env_vars()),
            None => {}
        }

        match self.profile.as_ref() {
            Some(profile) => env_vars.extend(profile.required_env_vars()),
            None => {
                #[cfg(target_arch = "wasm32")]
                env_vars.insert("AWS_PROFILE".into());
            }
        }

        match self.endpoint_url.as_ref() {
            Some(endpoint_url) => env_vars.extend(endpoint_url.required_env_vars()),
            None => {}
        }

        env_vars.extend(self.role_selection.required_env_vars());
        env_vars.extend(self.allowed_role_metadata.required_env_vars());
        env_vars.extend(self.supported_request_modes.required_env_vars());
        if let Some(c) = self.inference_config.as_ref() {
            env_vars.extend(c.required_env_vars())
        }
        env_vars
    }

    pub fn resolve(&self, ctx: &EvaluationContext<'_>) -> Result<ResolvedAwsBedrock> {
        let Some(model) = self.model.as_ref() else {
            return Err(anyhow::anyhow!("model must be provided"));
        };

        let role_selection = self.role_selection.resolve(ctx)?;

        let region = match self.region.as_ref() {
            Some(region) => {
                let region = region.resolve(ctx)?;
                if region.is_empty() {
                    return Err(anyhow::anyhow!("region cannot be empty"));
                }
                Some(region)
            }
            None => match ctx.get_env_var("AWS_REGION") {
                Ok(region) if !region.is_empty() => Some(region),
                _ => match ctx.get_env_var("AWS_DEFAULT_REGION") {
                    Ok(region) if !region.is_empty() => Some(region),
                    _ => None,
                },
            },
        };

        let access_key_id = match self.access_key_id.as_ref() {
            Some(access_key_id) => Some(access_key_id.resolve(ctx)?),
            None => None,
        };

        let secret_access_key = self
            .secret_access_key
            .as_ref()
            .map(|key| key.resolve_api_key(ctx))
            .transpose()?;

        let session_token = match self.session_token.as_ref() {
            Some(session_token) => {
                let token = session_token.resolve(ctx)?;
                if !token.is_empty() {
                    Some(token)
                } else {
                    None
                }
            }
            None => None,
        };

        let (access_key_id, secret_access_key, session_token) =
            match (access_key_id, secret_access_key, session_token) {
                (None, None, None) => {
                    // If no credentials provided, get them all from env vars
                    let access_key_id = match ctx.get_env_var("AWS_ACCESS_KEY_ID") {
                        Ok(key) if !key.is_empty() => {
                            #[cfg(target_arch = "wasm32")]
                            {
                                if key == "$AWS_ACCESS_KEY_ID" {
                                    None
                                } else {
                                    Some(key)
                                }
                            }

                            #[cfg(not(target_arch = "wasm32"))]
                            Some(key)
                        }
                        _ => None,
                    };
                    let secret_access_key = match ctx.get_env_var("AWS_SECRET_ACCESS_KEY") {
                        Ok(key) if !key.is_empty() => {
                            #[cfg(target_arch = "wasm32")]
                            {
                                if key == "$AWS_SECRET_ACCESS_KEY" {
                                    None
                                } else {
                                    Some(ApiKeyWithProvenance {
                                        api_key: SecretString::from(key),
                                        provenance: Some("AWS_SECRET_ACCESS_KEY".to_string()),
                                    })
                                }
                            }

                            #[cfg(not(target_arch = "wasm32"))]
                            Some(ApiKeyWithProvenance {
                                api_key: SecretString::from(key),
                                provenance: Some("AWS_SECRET_ACCESS_KEY".to_string()),
                            })
                        }
                        _ => None,
                    };
                    let session_token = match ctx.get_env_var("AWS_SESSION_TOKEN") {
                        Ok(token) if !token.is_empty() => {
                            #[cfg(target_arch = "wasm32")]
                            {
                                if token == "$AWS_SESSION_TOKEN" {
                                    None
                                } else {
                                    Some(token)
                                }
                            }

                            #[cfg(not(target_arch = "wasm32"))]
                            Some(token)
                        }
                        _ => None,
                    };
                    (access_key_id, secret_access_key, session_token)
                }
                // If any credentials are explicitly provided, use those
                (access_key_id, secret_access_key, session_token) => {
                    (access_key_id, secret_access_key, session_token)
                }
            };

        let profile = match self.profile.as_ref() {
            Some(profile) => Some(profile.resolve(ctx)?),
            None => match ctx.get_env_var("AWS_PROFILE") {
                Ok(profile) if !profile.is_empty() => {
                    #[cfg(target_arch = "wasm32")]
                    {
                        if profile == "$AWS_PROFILE" {
                            None
                        } else {
                            Some(profile)
                        }
                    }

                    #[cfg(not(target_arch = "wasm32"))]
                    Some(profile)
                }
                _ => None,
            },
        };

        let endpoint_url = match self.endpoint_url.as_ref() {
            Some(endpoint_url) => {
                let url = endpoint_url.resolve(ctx)?;
                if url.is_empty() {
                    None
                } else {
                    Some(url)
                }
            }
            None => None,
        };

        #[cfg(target_arch = "wasm32")]
        {
            if region.is_none() {
                return Err(anyhow::anyhow!("region must be provided"));
            }
        }

        let additional_model_request_fields = self
            .additional_model_request_fields
            .iter()
            .map(|(k, (_, v))| Ok((k.clone(), v.resolve_serde::<serde_json::Value>(ctx)?)))
            .collect::<Result<IndexMap<_, _>>>()?;

        Ok(ResolvedAwsBedrock {
            model: model.resolve(ctx)?,
            region,
            access_key_id,
            secret_access_key,
            session_token,
            profile,
            endpoint_url,
            role_selection,
            allowed_role_metadata: self.allowed_role_metadata.resolve(ctx)?,
            supported_request_modes: self.supported_request_modes.clone(),
            inference_config: self
                .inference_config
                .as_ref()
                .map(|c| c.resolve(ctx))
                .transpose()?,
            finish_reason_filter: self.finish_reason_filter.resolve(ctx)?,
            additional_model_request_fields,
            media_url_handler: self.media_url_handler.resolve(ctx)?,
            http_config: self.http_config.clone(),
        })
    }

    pub fn create_from(mut properties: PropertyHandler<Meta>) -> Result<Self, Vec<Error<Meta>>> {
        let model = {
            // Add AWS Bedrock-specific validation logic here
            let model_id = properties.ensure_string("model_id", false);
            let model = properties.ensure_string("model", false);

            match (model_id, model) {
                (Some((model_id_key_meta, _, _)), Some((model_key_meta, _, _))) => {
                    properties.push_error(
                        "model_id and model cannot both be provided",
                        model_id_key_meta,
                    );
                    properties
                        .push_error("model_id and model cannot both be provided", model_key_meta);
                    None
                }
                (Some((_, model, _)), None) | (None, Some((_, model, _))) => Some(model),
                (None, None) => {
                    properties.push_option_error("model_id is required");
                    None
                }
            }
        };

        let region = properties
            .ensure_string("region", false)
            .map(|(_, v, _)| v.clone());
        let access_key_id = properties
            .ensure_string("access_key_id", false)
            .map(|(_, v, _)| v.clone());

        let secret_access_key = properties
            .ensure_string("secret_access_key", false)
            .map(|(_, v, _)| v.clone());
        let session_token = properties
            .ensure_string("session_token", false)
            .map(|(_, v, _)| v.clone());
        let profile = properties
            .ensure_string("profile", false)
            .map(|(_, v, _)| v.clone());
        let endpoint_url = properties
            .ensure_string("endpoint_url", false)
            .map(|(_, v, _)| v.clone());

        let role_selection = properties.ensure_roles_selection();
        let allowed_metadata = properties.ensure_allowed_metadata();
        let supported_request_modes = properties.ensure_supported_request_modes();
        let additional_model_request_fields = properties
            .ensure_map("additional_model_request_fields", false)
            .map(|(_, map, _)| map)
            .unwrap_or_default();

        let inference_config = {
            let mut inference_config = UnresolvedInferenceConfiguration {
                max_tokens: None,
                temperature: None,
                top_p: None,
                stop_sequences: None,
            };
            let raw = properties.ensure_map("inference_configuration", false);
            if let Some((_, map, _)) = raw {
                for (k, (key_span, v)) in map.into_iter() {
                    match k.as_str() {
                        "max_tokens" => inference_config.max_tokens = v.as_numeric().and_then(|val| match val.parse() {
                            Ok(v) => Some(v),
                            Err(e) => {
                                properties.push_error(format!("max_tokens must be a number: {e}"), v.meta().clone());
                                None
                            }
                        }),
                        "temperature" => inference_config.temperature = v.as_numeric().and_then(|val| match val.parse() {
                            Ok(v) => Some(v),
                            Err(e) => {
                                properties.push_error(format!("temperature must be a number: {e}"), v.meta().clone());
                                None
                            }
                        }),
                        "top_p" => inference_config.top_p = v.as_numeric().and_then(|val| match val.parse() {
                            Ok(v) => Some(v),
                            Err(e) => {
                                properties.push_error(format!("top_p must be a number: {e}"), v.meta().clone());
                                None
                            }
                        }),
                        "stop_sequences" => inference_config.stop_sequences = match v.into_array() {
                            Ok((stop_sequences, _)) => Some(stop_sequences.into_iter().filter_map(|s| match s.into_str() {
                                    Ok((s, _)) => Some(s),
                                    Err(e) => {
                                        properties.push_error(format!("stop_sequences values must be a string: got {}", e.r#type()), e.meta().clone());
                                        None
                                    }
                                })
                                .collect::<Vec<_>>()),
                            Err(e) => {
                                properties.push_error(
                                    format!("stop_sequences must be an array: {}", e.r#type()),
                                    e.meta().clone(),
                                );
                                None
                            }
                        },
                        _ => {
                            properties.push_error(format!("unknown inference_config key: {k}"), key_span.clone());
                        },
                    }
                }
            }
            Some(inference_config)
        };
        let finish_reason_filter = properties.ensure_finish_reason_filter();
        let media_url_handler = properties.ensure_media_url_handler();
        let http_config = properties.ensure_http_config("aws");

        // TODO: Handle inference_configuration
        let errors = properties.finalize_empty();
        if !errors.is_empty() {
            return Err(errors);
        }

        Ok(Self {
            model,
            region,
            access_key_id,
            secret_access_key,
            session_token,
            profile,
            endpoint_url,
            role_selection,
            allowed_role_metadata: allowed_metadata,
            supported_request_modes,
            inference_config,
            finish_reason_filter,
            additional_model_request_fields,
            media_url_handler,
            http_config,
        })
    }
}
