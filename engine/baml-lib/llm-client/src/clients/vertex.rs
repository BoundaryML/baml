use std::collections::HashSet;

use anyhow::Result;
use baml_derive::BamlHash;
use baml_types::{GetEnvVar, StringOr, UnresolvedValue};
use either::Either;
use indexmap::IndexMap;

use super::helpers::{Error, PropertyHandler, UnresolvedUrl};
use crate::{
    AllowedRoleMetadata, FinishReasonFilter, MediaUrlHandler, RolesSelection,
    SupportedRequestModes, UnresolvedAllowedRoleMetadata, UnresolvedFinishReasonFilter,
    UnresolvedMediaUrlHandler, UnresolvedRolesSelection,
};

#[derive(Debug, Clone, BamlHash)]
enum UnresolvedGcpAuthStrategy<Meta> {
    /// This can be resolved as either FilePath or JsonString
    CredentialsString(StringOr),
    /// This will always be resolved as JsonObject
    CredentialsJsonObject(#[baml_safe_hash] IndexMap<String, (Meta, UnresolvedValue<Meta>)>),
    /// This will always be resolved as JsonString
    CredentialsContentString(StringOr),
    /// This will always be resolved as UseSystemDefault
    SystemDefault,
}

pub enum ResolvedGcpAuthStrategy {
    /// GCP SDKs usually support passing in GOOGLE_APPLICATION_CREDENTIALS as a file path
    /// In WASM, however, we treat both StringContainingJson and MaybeFilePath as a string
    MaybeFilePath(String),
    /// Because the WASM playground needs a way to pass in credentials.
    StringContainingJson(String),
    /// JsonObject was implemented for a user: https://github.com/BoundaryML/baml/issues/1001
    JsonObject(IndexMap<String, String>),
    /// The normal GCP application default credentials flow, after checking
    /// GOOGLE_APPLICATION_CREDENTIALS (since we have to intercept that), with
    /// an additional gcloud-based fallback
    ///
    /// See:
    ///   - https://cloud.google.com/docs/authentication/application-default-credentials
    ///   - https://docs.rs/gcp_auth/latest/gcp_auth/fn.provider.html
    SystemDefault,
}

impl<Meta> UnresolvedGcpAuthStrategy<Meta> {
    fn without_meta(&self) -> UnresolvedGcpAuthStrategy<()> {
        match self {
            UnresolvedGcpAuthStrategy::CredentialsString(s) => {
                UnresolvedGcpAuthStrategy::CredentialsString(s.clone())
            }
            UnresolvedGcpAuthStrategy::CredentialsJsonObject(m) => {
                UnresolvedGcpAuthStrategy::CredentialsJsonObject(
                    m.iter()
                        .map(|(k, v)| (k.clone(), ((), v.1.without_meta())))
                        .collect(),
                )
            }
            UnresolvedGcpAuthStrategy::CredentialsContentString(s) => {
                UnresolvedGcpAuthStrategy::CredentialsContentString(s.clone())
            }
            UnresolvedGcpAuthStrategy::SystemDefault => UnresolvedGcpAuthStrategy::SystemDefault,
        }
    }

    fn required_env_vars(&self) -> HashSet<String> {
        match self {
            UnresolvedGcpAuthStrategy::CredentialsString(s) => s.required_env_vars(),
            UnresolvedGcpAuthStrategy::CredentialsJsonObject(m) => m
                .values()
                .flat_map(|(_, v)| v.required_env_vars())
                .collect(),
            UnresolvedGcpAuthStrategy::CredentialsContentString(s) => s.required_env_vars(),
            // required_env_vars() is only used for the playground list of "you
            // should set these env vars", I think, so this should be fine
            UnresolvedGcpAuthStrategy::SystemDefault => {
                vec!["GOOGLE_APPLICATION_CREDENTIALS".to_string()]
                    .into_iter()
                    .collect()
            }
        }
    }

    fn resolve(&self, ctx: &impl GetEnvVar) -> Result<ResolvedGcpAuthStrategy> {
        Ok(match self {
            UnresolvedGcpAuthStrategy::CredentialsString(s) => {
                let s = s.resolve(ctx)?;
                match serde_json::from_str::<serde_json::Value>(&s) {
                    Ok(_) => ResolvedGcpAuthStrategy::StringContainingJson(s),
                    Err(_) => ResolvedGcpAuthStrategy::MaybeFilePath(s),
                }
            }
            UnresolvedGcpAuthStrategy::CredentialsJsonObject(m) => {
                let m = m
                    .iter()
                    .map(|(k, (_, v))| Ok((k.clone(), v.resolve_string(ctx)?)))
                    .collect::<Result<IndexMap<_, _>>>()?;
                ResolvedGcpAuthStrategy::JsonObject(m)
            }
            UnresolvedGcpAuthStrategy::CredentialsContentString(s) => {
                let s = s.resolve(ctx)?;
                ResolvedGcpAuthStrategy::StringContainingJson(s)
            }
            UnresolvedGcpAuthStrategy::SystemDefault => {
                log::debug!("Neither options.credentials nor options.credentials_content are set, falling back to env vars");
                // Without this, for some reason get_env_var() comes back as "$BASH_STYLE_SUBSTITUTION"
                // I'm sure there's a reason for this, but it doesn't make sense to me right now.
                let ctx = ctx.set_allow_missing_env_var(false);
                match (
                    ctx.get_env_var("GOOGLE_APPLICATION_CREDENTIALS").ok(),
                    ctx.get_env_var("GOOGLE_APPLICATION_CREDENTIALS_CONTENT")
                        .ok(),
                ) {
                    (Some(credentials), _) => {
                        log::debug!("Using GOOGLE_APPLICATION_CREDENTIALS from env");
                        if credentials.is_empty() {
                            log::warn!("Resolving GOOGLE_APPLICATION_CREDENTIALS from env, but it is an empty string");
                        }
                        match serde_json::from_str::<serde_json::Value>(&credentials) {
                            Ok(_) => ResolvedGcpAuthStrategy::StringContainingJson(credentials),
                            Err(_) => ResolvedGcpAuthStrategy::MaybeFilePath(credentials),
                        }
                    }
                    (None, Some(credentials_content)) => {
                        log::debug!("Using GOOGLE_APPLICATION_CREDENTIALS_CONTENT from env");
                        if credentials_content.is_empty() {
                            log::warn!("Resolving GOOGLE_APPLICATION_CREDENTIALS_CONTENT from env, but it is an empty string");
                        }
                        ResolvedGcpAuthStrategy::StringContainingJson(credentials_content)
                    }
                    (None, None) => {
                        log::debug!("Using UseSystemDefault strategy");
                        ResolvedGcpAuthStrategy::SystemDefault
                    }
                }
            }
        })
    }
}

#[derive(Debug, Clone, BamlHash)]
pub struct UnresolvedVertex<Meta> {
    // Either base_url or location
    base_url_or_location: Either<UnresolvedUrl, StringOr>,
    project_id: Option<StringOr>,
    auth_strategy: UnresolvedGcpAuthStrategy<Meta>,
    model: StringOr,
    #[baml_safe_hash]
    headers: IndexMap<String, StringOr>,
    #[baml_safe_hash]
    query_params: IndexMap<String, StringOr>,
    role_selection: UnresolvedRolesSelection,
    allowed_role_metadata: UnresolvedAllowedRoleMetadata,
    supported_request_modes: SupportedRequestModes,
    finish_reason_filter: UnresolvedFinishReasonFilter,
    #[baml_safe_hash]
    properties: IndexMap<String, (Meta, UnresolvedValue<Meta>)>,
    anthropic_version: Option<StringOr>,
    media_url_handler: UnresolvedMediaUrlHandler,
    http_config: super::helpers::HttpConfig,
}

pub enum BaseUrlOrLocation {
    BaseUrl(String),
    Location(String),
}

pub struct ResolvedVertex {
    pub base_url_or_location: BaseUrlOrLocation,
    pub project_id: Option<String>,
    pub auth_strategy: ResolvedGcpAuthStrategy,
    pub model: String,
    pub headers: IndexMap<String, String>,
    pub query_params: IndexMap<String, String>,
    /// This is usually not pub, but we need it so that we can pass it through to the Anthropic client.
    pub role_selection: RolesSelection,
    pub allowed_metadata: AllowedRoleMetadata,
    pub supported_request_modes: SupportedRequestModes,
    pub properties: IndexMap<String, serde_json::Value>,
    pub proxy_url: Option<String>,
    pub finish_reason_filter: FinishReasonFilter,
    pub anthropic_version: Option<String>,
    pub media_url_handler: MediaUrlHandler,
    pub http_config: super::helpers::HttpConfig,
}

impl ResolvedVertex {
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

impl<Meta: Clone> UnresolvedVertex<Meta> {
    pub fn required_env_vars(&self) -> HashSet<String> {
        let mut env_vars = HashSet::new();
        match self.base_url_or_location {
            either::Either::Left(ref base_url) => env_vars.extend(base_url.required_env_vars()),
            either::Either::Right(ref location) => env_vars.extend(location.required_env_vars()),
        }
        if let Some(ref project_id) = self.project_id {
            env_vars.extend(project_id.required_env_vars());
        }
        env_vars.extend(self.auth_strategy.required_env_vars());
        env_vars.extend(self.model.required_env_vars());
        env_vars.extend(self.headers.values().flat_map(StringOr::required_env_vars));
        env_vars.extend(
            self.query_params
                .values()
                .flat_map(StringOr::required_env_vars),
        );
        env_vars.extend(self.role_selection.required_env_vars());
        env_vars.extend(self.allowed_role_metadata.required_env_vars());
        env_vars.extend(self.supported_request_modes.required_env_vars());
        env_vars.extend(
            self.properties
                .values()
                .flat_map(|(_, v)| v.required_env_vars()),
        );

        env_vars
    }

    pub fn without_meta(&self) -> UnresolvedVertex<()> {
        UnresolvedVertex {
            base_url_or_location: self.base_url_or_location.clone(),
            project_id: self.project_id.clone(),
            auth_strategy: self.auth_strategy.without_meta(),
            model: self.model.clone(),
            headers: self.headers.clone(),
            query_params: self.query_params.clone(),
            role_selection: self.role_selection.clone(),
            allowed_role_metadata: self.allowed_role_metadata.clone(),
            supported_request_modes: self.supported_request_modes.clone(),
            properties: self
                .properties
                .iter()
                .map(|(k, (_, v))| (k.clone(), ((), v.without_meta())))
                .collect(),
            finish_reason_filter: self.finish_reason_filter.clone(),
            anthropic_version: self.anthropic_version.clone(),
            media_url_handler: self.media_url_handler.clone(),
            http_config: self.http_config.clone(),
        }
    }

    pub fn resolve(&self, ctx: &impl GetEnvVar) -> Result<ResolvedVertex> {
        // Validate auth options - only one should be provided
        let base_url_or_location = match &self.base_url_or_location {
            either::Either::Left(base_url) => BaseUrlOrLocation::BaseUrl(base_url.resolve(ctx)?),
            either::Either::Right(location) => BaseUrlOrLocation::Location(location.resolve(ctx)?),
        };

        let model = self.model.resolve(ctx)?;

        let role_selection = self.role_selection.resolve(ctx)?;

        let headers = self
            .headers
            .iter()
            .map(|(k, v)| Ok((k.clone(), v.resolve(ctx)?)))
            .collect::<Result<IndexMap<_, _>>>()?;

        let query_params = self
            .query_params
            .iter()
            .map(|(k, v)| Ok((k.clone(), v.resolve(ctx)?)))
            .collect::<Result<IndexMap<_, _>>>()?;

        // HACK: for some reason .resolve returns the env var name with $ if it's not found. So we need to check for that.
        let project_id = match self.project_id {
            Some(ref project_id) => {
                let resolved = project_id.resolve(ctx)?;
                if resolved.starts_with("$") || resolved.is_empty() {
                    None
                } else {
                    Some(resolved)
                }
            }
            None => None,
        };

        Ok(ResolvedVertex {
            base_url_or_location,
            project_id,
            auth_strategy: self.auth_strategy.resolve(ctx)?,
            model,
            headers,
            query_params,
            role_selection,
            allowed_metadata: self.allowed_role_metadata.resolve(ctx)?,
            supported_request_modes: self.supported_request_modes.clone(),
            properties: self
                .properties
                .iter()
                .map(|(k, (_, v))| Ok((k.clone(), v.resolve_serde::<serde_json::Value>(ctx)?)))
                .collect::<Result<IndexMap<_, _>>>()?,
            proxy_url: super::helpers::get_proxy_url(ctx),
            finish_reason_filter: self.finish_reason_filter.resolve(ctx)?,
            anthropic_version: match self.anthropic_version {
                Some(ref anthropic_version) => Some(anthropic_version.resolve(ctx)?),
                None => None,
            },
            media_url_handler: self.media_url_handler.resolve(ctx)?,
            http_config: self.http_config.clone(),
        })
    }

    pub fn create_from(mut properties: PropertyHandler<Meta>) -> Result<Self, Vec<Error<Meta>>> {
        let auth_strategy: UnresolvedGcpAuthStrategy<Meta> = {
            let credentials_field = properties.ensure_any("credentials");
            let credentials_content_field = properties.ensure_string("credentials_content", false);

            match (credentials_field, credentials_content_field) {
                (Some((credentials_span, credentials_value)), credentials_content_field) => {
                    if let Some((credentials_content_span, _credentials_content_value, _)) =
                        credentials_content_field
                    {
                        properties.push_error("Both 'credentials' and 'credentials_content' provided. Please only set one or the other.", credentials_span);
                        properties.push_error("Both 'credentials' and 'credentials_content' provided. Please only set one or the other.", credentials_content_span);
                    }

                    match credentials_value {
                        UnresolvedValue::String(s, ..) => {
                            UnresolvedGcpAuthStrategy::CredentialsString(s)
                        }
                        UnresolvedValue::Map(m, ..) => {
                            UnresolvedGcpAuthStrategy::CredentialsJsonObject(m)
                        }
                        other => {
                            properties.push_error(
                                format!(
                                    "credentials must be a string or an object. Got: {}",
                                    other.r#type()
                                ),
                                other.meta().clone(),
                            );
                            UnresolvedGcpAuthStrategy::SystemDefault
                        }
                    }
                }
                (None, Some((_, credentials_content, _))) => {
                    UnresolvedGcpAuthStrategy::CredentialsContentString(credentials_content)
                }
                (None, None) => UnresolvedGcpAuthStrategy::SystemDefault,
            }
        };
        let model = properties.ensure_string("model", true).map(|(_, v, _)| v);

        let base_url_or_location = {
            let base_url = properties.ensure_base_url(false);
            let location = properties
                .ensure_string("location", false)
                .map(|(key_span, v, _)| (key_span, v.clone()));

            match (base_url, location) {
                (Some(url), None) => Some(either::Either::Left(url.1)),
                (None, Some(name)) => Some(either::Either::Right(name.1)),
                (Some((key_1_span, ..)), Some((key_2_span, _))) => {
                    for key in [key_1_span, key_2_span] {
                        properties.push_error(
                            "vertex-ai clients may specify either location or base_url, but not both (try removing 'base_url' from options).",
                            key,
                        );
                    }
                    None
                }
                (None, None) => {
                    properties.push_option_error("vertex-ai clients must specify a GCP region in options.location (e.g. us-central1)");
                    None
                }
            }
        };

        let project_id = properties
            .ensure_string("project_id", false)
            .map(|(_, v, _)| v)
            .or_else(|| Some(StringOr::EnvVar("GOOGLE_CLOUD_PROJECT".to_string())));

        let role_selection = properties.ensure_roles_selection();
        let allowed_metadata = properties.ensure_allowed_metadata();
        let supported_request_modes = properties.ensure_supported_request_modes();
        let headers = properties.ensure_headers().unwrap_or_default();
        let query_params = properties.ensure_query_params().unwrap_or_default();
        let finish_reason_filter = properties.ensure_finish_reason_filter();

        let anthropic_version = properties
            .ensure_string("anthropic_version", false)
            .map(|(_, v, _)| v);

        let media_url_handler = properties.ensure_media_url_handler();
        let http_config = properties.ensure_http_config("vertex");

        let (properties, errors) = properties.finalize();
        if !errors.is_empty() {
            return Err(errors);
        }

        let model = model.expect("model is required");
        let base_url_or_location =
            base_url_or_location.expect("location (or base_url) is required");

        Ok(Self {
            base_url_or_location,
            project_id,
            auth_strategy,
            model,
            headers,
            query_params,
            role_selection,
            allowed_role_metadata: allowed_metadata,
            supported_request_modes,
            properties,
            finish_reason_filter,
            anthropic_version,
            media_url_handler,
            http_config,
        })
    }
}
