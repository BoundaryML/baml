use std::{borrow::Cow, collections::HashSet};

use baml_derive::BamlHash;
use baml_types::{GetEnvVar, StringOr, UnresolvedValue};
use indexmap::IndexMap;

use crate::{
    SupportedRequestModes, UnresolvedAllowedRoleMetadata, UnresolvedFinishReasonFilter,
    UnresolvedMediaUrlHandler, UnresolvedResolveMediaUrls, UnresolvedResponseType,
    UnresolvedRolesSelection,
};

/// Configuration for HTTP timeouts
#[derive(Debug, Clone, Default, BamlHash)]
pub struct HttpConfig {
    pub connect_timeout_ms: Option<u64>,
    pub request_timeout_ms: Option<u64>,
    pub time_to_first_token_timeout_ms: Option<u64>,
    pub idle_timeout_ms: Option<u64>,
    pub total_timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Hash)]
pub struct UnresolvedUrl(StringOr);

impl UnresolvedUrl {
    pub fn resolve(&self, ctx: &impl GetEnvVar) -> anyhow::Result<String> {
        let mut url = self.0.resolve(ctx)?;
        // Strip trailing slash
        if url.ends_with('/') {
            url.pop();
        }
        Ok(url)
    }

    pub fn new_static(url: impl Into<String>) -> Self {
        Self(StringOr::Value(url.into()))
    }

    pub fn required_env_vars(&self) -> HashSet<String> {
        self.0.required_env_vars()
    }
}

pub struct Error<Meta> {
    pub message: String,
    pub span: Meta,
}

impl<Meta> Error<Meta> {
    pub fn new(message: impl Into<Cow<'static, str>>, span: Meta) -> Self {
        Self {
            message: message.into().to_string(),
            span,
        }
    }
}

pub struct PropertyHandler<Meta> {
    options: IndexMap<String, (Meta, UnresolvedValue<Meta>)>,
    span: Meta,
    errors: Vec<Error<Meta>>,
}

impl<Meta: Clone> PropertyHandler<Meta> {
    pub fn new(options: IndexMap<String, (Meta, UnresolvedValue<Meta>)>, span: Meta) -> Self {
        Self {
            options,
            span,
            errors: Vec::new(),
        }
    }

    pub fn print_options(&self) {
        eprintln!(
            "options: {:#?}",
            self.options
                .iter()
                .map(|(k, (_, v))| (k, v.as_str()))
                .collect::<IndexMap<_, _>>()
        );
    }

    pub fn push_option_error(&mut self, message: impl Into<Cow<'static, str>>) {
        self.errors.push(Error::new(message, self.span.clone()));
    }

    pub fn push_error(&mut self, message: impl Into<Cow<'static, str>>, span: Meta) {
        self.errors.push(Error::new(message, span));
    }

    pub fn ensure_string(&mut self, key: &str, required: bool) -> Option<(Meta, StringOr, Meta)> {
        let result = match ensure_string(&mut self.options, key) {
            Ok(result) => {
                if required && result.is_none() {
                    self.push_option_error(format!("Missing required property: {key}"));
                }
                result
            }
            Err(e) => {
                self.errors.push(e);
                return None;
            }
        };

        result.map(|(key_span, value, meta)| (key_span.clone(), value, meta.clone()))
    }

    pub fn ensure_map(
        &mut self,
        key: &str,
        required: bool,
    ) -> Option<(Meta, IndexMap<String, (Meta, UnresolvedValue<Meta>)>, Meta)> {
        let result = match ensure_map(&mut self.options, key) {
            Ok(result) => {
                if required && result.is_none() {
                    self.push_option_error(format!("Missing required property: {key}"));
                }
                result
            }
            Err(e) => {
                self.errors.push(e);
                return None;
            }
        };

        result.map(|(key_span, value, meta)| (key_span.clone(), value, meta.clone()))
    }

    pub fn ensure_array(
        &mut self,
        key: &str,
        required: bool,
    ) -> Option<(Meta, Vec<UnresolvedValue<Meta>>, Meta)> {
        let result = match ensure_array(&mut self.options, key) {
            Ok(result) => {
                if required && result.is_none() {
                    self.push_option_error(format!("Missing required property: {key}"));
                }
                result
            }
            Err(e) => {
                self.errors.push(e);
                return None;
            }
        };

        result.map(|(key_span, value, meta)| (key_span.clone(), value, meta.clone()))
    }

    pub fn ensure_bool(&mut self, key: &str, required: bool) -> Option<(Meta, bool, Meta)> {
        let result = match ensure_bool(&mut self.options, key) {
            Ok(result) => {
                if required && result.is_none() {
                    self.push_option_error(format!("Missing required property: {key}"));
                }
                result
            }
            Err(e) => {
                self.errors.push(e);
                return None;
            }
        };

        result.map(|(key_span, value, meta)| (key_span.clone(), value, meta.clone()))
    }

    pub fn ensure_int(&mut self, key: &str, required: bool) -> Option<(Meta, i32, Meta)> {
        let result = match ensure_int(&mut self.options, key) {
            Ok(result) => {
                if required && result.is_none() {
                    self.push_option_error(format!("Missing required property: {key}"));
                }
                result
            }
            Err(e) => {
                self.errors.push(e);
                return None;
            }
        };

        result.map(|(key_span, value, meta)| (key_span.clone(), value, meta.clone()))
    }

    fn ensure_remap_role(&mut self, allowed_roles: &[StringOr]) -> Option<Vec<(String, StringOr)>> {
        self.ensure_map("remap_roles", false).map(|(_, value, _)| {
            value
                .into_iter()
                .filter_map(|(from, (key_span, remap_to))| match remap_to.as_str() {
                    Some(remap_string) => {
                        if allowed_roles.iter().any(|v| v.maybe_eq(&StringOr::Value(from.clone()))) {
                            Some((from, remap_string.clone()))
                        } else {
                            self.push_error(
                                format!(
                                    "remap_roles values must be one of: {allowed_roles_str}. Got: {from}. To support different remap roles, add allowed_roles [\"user\", \"assistant\", \"system\", ...]",
                                    allowed_roles_str = allowed_roles
                                        .iter()
                                        .map(|v| format!("{v:?}"))
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                ),
                                key_span,
                            );
                            None
                        }
                    }
                    None => {
                        self.push_error(
                            format!(
                                "remap_role must be a map of strings to strings. Got: {}",
                                remap_to.r#type()
                            ),
                            remap_to.meta().clone(),
                        );
                        None
                    }
                })
                .collect::<Vec<_>>()
        })
    }

    fn ensure_allowed_roles(&mut self) -> Option<Vec<StringOr>> {
        self.ensure_array("allowed_roles", false)
            .map(|(_, value, value_span)| {
                if value.is_empty() {
                    self.push_error("allowed_roles must not be empty", value_span);
                }

                value
                    .into_iter()
                    .filter_map(|v| match v.as_str() {
                        Some(s) => Some(s.clone()),
                        None => {
                            self.push_error(
                                format!(
                                    "values in allowed_roles must be strings. Got: {}",
                                    v.r#type()
                                ),
                                v.meta().clone(),
                            );
                            None
                        }
                    })
                    .collect()
            })
    }

    pub(crate) fn ensure_roles_selection(&mut self) -> UnresolvedRolesSelection {
        let allowed_roles = self.ensure_allowed_roles();

        let default_allowed_roles = vec![
            StringOr::Value("user".to_string()),
            StringOr::Value("assistant".to_string()),
            StringOr::Value("system".to_string()),
        ];
        let default_role =
            self.ensure_default_role(allowed_roles.as_ref().unwrap_or(&default_allowed_roles));
        let remap_role =
            self.ensure_remap_role(allowed_roles.as_ref().unwrap_or(&default_allowed_roles));
        UnresolvedRolesSelection::new(allowed_roles, default_role, remap_role)
    }

    fn ensure_default_role(&mut self, allowed_roles: &[StringOr]) -> Option<StringOr> {
        self.ensure_string("default_role", false)
            .and_then(|(_, value, span)| {
                if allowed_roles.iter().any(|v| value.maybe_eq(v)) {
                    Some(value)
                } else {
                    let allowed_roles_str = allowed_roles
                        .iter()
                        .map(|v| format!("{v:?}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    self.push_error(
                        format!(
                            "default_role must be one of {allowed_roles_str}. Got: {value}. To support different default roles, add allowed_roles [\"user\", \"assistant\", \"system\", ...]"
                        ),
                        span,
                    );
                    None
                }
            })
    }

    pub fn ensure_api_key(&mut self) -> Option<StringOr> {
        self.ensure_string("api_key", false)
            .map(|(_, value, _)| value)
    }

    pub fn ensure_base_url_with_default(&mut self, default: UnresolvedUrl) -> UnresolvedUrl {
        self.ensure_string("base_url", false)
            .map(|(_, value, _)| UnresolvedUrl(value))
            .unwrap_or(default)
    }

    pub fn ensure_base_url(&mut self, required: bool) -> Option<(Meta, UnresolvedUrl, Meta)> {
        self.ensure_string("base_url", required)
            .map(|(key_span, value, meta)| (key_span, UnresolvedUrl(value), meta))
    }

    pub fn ensure_supported_request_modes(&mut self) -> SupportedRequestModes {
        let result = self.ensure_bool("supports_streaming", false);
        match result {
            Some((_, value, _)) => SupportedRequestModes {
                stream: Some(value),
            },
            None => SupportedRequestModes { stream: None },
        }
    }

    pub fn ensure_finish_reason_filter(&mut self) -> UnresolvedFinishReasonFilter {
        let allow_list = self.ensure_array("finish_reason_allow_list", false);
        let deny_list = self.ensure_array("finish_reason_deny_list", false);

        match (allow_list, deny_list) {
            (Some(allow), Some(deny)) => {
                self.push_error(
                    "finish_reason_allow_list and finish_reason_deny_list cannot be used together",
                    allow.0,
                );
                self.push_error(
                    "finish_reason_allow_list and finish_reason_deny_list cannot be used together",
                    deny.0,
                );
                UnresolvedFinishReasonFilter::All
            }
            (Some((_, allow, _)), None) => UnresolvedFinishReasonFilter::AllowList(
                allow
                    .into_iter()
                    .filter_map(|v| match v.as_str() {
                        Some(s) => Some(s.clone()),
                        None => {
                            self.push_error(
                                "values in finish_reason_allow_list must be strings.",
                                v.meta().clone(),
                            );
                            None
                        }
                    })
                    .collect(),
            ),
            (None, Some((_, deny, _))) => UnresolvedFinishReasonFilter::DenyList(
                deny.into_iter()
                    .filter_map(|v| match v.into_str() {
                        Ok((s, _)) => Some(s.clone()),
                        Err(other) => {
                            self.push_error(
                                "values in finish_reason_deny_list must be strings.",
                                other.meta().clone(),
                            );
                            None
                        }
                    })
                    .collect(),
            ),
            (None, None) => UnresolvedFinishReasonFilter::All,
        }
    }

    pub fn ensure_any(&mut self, key: &str) -> Option<(Meta, UnresolvedValue<Meta>)> {
        self.options.shift_remove(key)
    }

    pub fn ensure_allowed_metadata(&mut self) -> UnresolvedAllowedRoleMetadata {
        if let Some((_, value)) = self.options.shift_remove("allowed_role_metadata") {
            if let Some(allowed_metadata) = value.as_array() {
                let allowed_metadata = allowed_metadata
                    .iter()
                    .filter_map(|v| match v.as_str() {
                        Some(s) => Some(s.clone()),
                        None => {
                            self.push_error(
                                "values in allowed_role_metadata must be strings.",
                                v.meta().clone(),
                            );
                            None
                        }
                    })
                    .collect();
                return UnresolvedAllowedRoleMetadata::Only(allowed_metadata);
            } else if let Some(allowed_metadata) = value.as_str() {
                return UnresolvedAllowedRoleMetadata::Value(allowed_metadata.clone());
            } else {
                self.push_error(
                    "allowed_role_metadata must be an array of keys or \"all\" or \"none\". For example: ['key1', 'key2']",
                    value.meta().clone(),
                );
            }
        }
        UnresolvedAllowedRoleMetadata::None
    }

    pub fn ensure_client_response_type(&mut self) -> Option<UnresolvedResponseType> {
        self.ensure_string("client_response_type", false)
            .and_then(|(key_span, value, _)| {
                if let StringOr::Value(value) = value {
                    Some(match value.as_str() {
                        "openai" => UnresolvedResponseType::OpenAI,
                        "openai-responses" => UnresolvedResponseType::OpenAIResponses,
                        "anthropic" => UnresolvedResponseType::Anthropic,
                        "google" => UnresolvedResponseType::Google,
                        "vertex" => UnresolvedResponseType::Vertex,
                        other => {
                            self.push_error(
                                format!(
                                    "client_response_type must be one of \"openai\", \"openai-responses\", \"anthropic\", \"google\", or \"vertex\". Got: {other}"
                                ),
                                key_span,
                            );
                            return None;
                        }
                    })
                } else {
                    self.push_error(
                        "client_response_type must be one of \"openai\", \"openai-responses\", \"anthropic\", \"google\", or \"vertex\" and not an environment variable",
                        key_span,
                    );
                    None
                }
            })
    }

    pub fn ensure_media_url_handler(&mut self) -> UnresolvedMediaUrlHandler {
        let mut result = UnresolvedMediaUrlHandler::default();

        if let Some((_span, map, _)) = self.ensure_map("media_url_handler", false) {
            for (key, (key_span, value)) in map {
                let resolve_mode = self.parse_resolve_media_urls(&value, &key_span);

                match key.as_str() {
                    "image" => result.images = resolve_mode,
                    "audio" => result.audio = resolve_mode,
                    "pdf" => result.pdf = resolve_mode,
                    "video" => result.video = resolve_mode,
                    other => {
                        self.push_error(
                            format!("Unknown media type in media_url_handler: {other}. Expected one of: image, audio, pdf, video"),
                            key_span
                        );
                    }
                }
            }
        }

        result
    }

    fn parse_resolve_media_urls(
        &mut self,
        value: &UnresolvedValue<Meta>,
        span: &Meta,
    ) -> Option<UnresolvedResolveMediaUrls> {
        match value.as_str() {
            Some(StringOr::Value(s)) => match s.as_str() {
                "send_base64" => Some(UnresolvedResolveMediaUrls::SendBase64),
                "send_url" => Some(UnresolvedResolveMediaUrls::SendUrl),
                "send_url_add_mime_type" => Some(UnresolvedResolveMediaUrls::SendUrlAddMimeType),
                "send_base64_unless_google_url" => {
                    Some(UnresolvedResolveMediaUrls::SendBase64UnlessGoogleUrl)
                }
                other => {
                    self.push_error(
                        format!(
                            "Invalid media URL handling mode: {other}. Expected one of: send_base64, send_url, send_url_add_mime_type, send_base64_unless_google_url"
                        ),
                        span.clone()
                    );
                    None
                }
            },
            Some(StringOr::EnvVar(_)) => {
                self.push_error(
                    "media_url_handler values cannot be environment variables",
                    span.clone(),
                );
                None
            }
            _ => {
                self.push_error("media_url_handler values must be strings", span.clone());
                None
            }
        }
    }

    pub fn ensure_query_params(&mut self) -> Option<IndexMap<String, StringOr>> {
        self.ensure_map("query_params", false).map(|(_, value, _)| {
            value
                .into_iter()
                .filter_map(|(k, (_, v))| match v.as_str() {
                    Some(s) => Some((k, s.clone())),
                    None => {
                        self.push_error(
                            format!(
                                "Query param key {} must have a string value. Got: {}",
                                k,
                                v.r#type()
                            ),
                            v.meta().clone(),
                        );
                        None
                    }
                })
                .collect()
        })
    }

    pub fn ensure_http_config(&mut self, provider_type: &str) -> HttpConfig {
        if let Some((_, http_value)) = self.ensure_any("http") {
            match http_value {
                UnresolvedValue::Map(config_map, value_span) => {
                    let mut http_config = HttpConfig::default();
                    let mut unrecognized_fields = Vec::new();

                    // Define allowed fields based on provider type
                    let is_composite =
                        provider_type == "fallback" || provider_type == "round-robin";
                    let allowed_fields: HashSet<&str> = if is_composite {
                        // Composite clients only support total_timeout_ms
                        vec!["total_timeout_ms"].into_iter().collect()
                    } else {
                        // Regular clients support all timeout types except total_timeout_ms
                        vec![
                            "connect_timeout_ms",
                            "request_timeout_ms",
                            "time_to_first_token_timeout_ms",
                            "idle_timeout_ms",
                        ]
                        .into_iter()
                        .collect()
                    };

                    for (key, (_, value)) in config_map {
                        match key.as_str() {
                            "connect_timeout_ms" if !is_composite => {
                                let value_meta = value.meta().clone();
                                match value.into_numeric() {
                                    Ok((val_str, _)) => {
                                        let val = val_str.parse::<i64>().unwrap_or(-1);
                                        if let Err(e) =
                                            validate_timeout_value(val, "connect_timeout_ms")
                                        {
                                            self.push_error(e, value_meta);
                                        } else {
                                            http_config.connect_timeout_ms = Some(val as u64);
                                        }
                                    }
                                    Err(other) => {
                                        self.push_error(
                                            format!(
                                                "connect_timeout_ms must be an integer. Got: {}",
                                                other.r#type()
                                            ),
                                            other.meta().clone(),
                                        );
                                    }
                                }
                            }
                            "request_timeout_ms" if !is_composite => {
                                let value_meta = value.meta().clone();
                                match value.into_numeric() {
                                    Ok((val_str, _)) => {
                                        let val = val_str.parse::<i64>().unwrap_or(-1);
                                        if let Err(e) =
                                            validate_timeout_value(val, "request_timeout_ms")
                                        {
                                            self.push_error(e, value_meta);
                                        } else {
                                            http_config.request_timeout_ms = Some(val as u64);
                                        }
                                    }
                                    Err(other) => {
                                        self.push_error(
                                            format!(
                                                "request_timeout_ms must be an integer. Got: {}",
                                                other.r#type()
                                            ),
                                            other.meta().clone(),
                                        );
                                    }
                                }
                            }
                            "time_to_first_token_timeout_ms" if !is_composite => {
                                let value_meta = value.meta().clone();
                                match value.into_numeric() {
                                    Ok((val_str, _)) => {
                                        let val = val_str.parse::<i64>().unwrap_or(-1);
                                        if let Err(e) = validate_timeout_value(
                                            val,
                                            "time_to_first_token_timeout_ms",
                                        ) {
                                            self.push_error(e, value_meta);
                                        } else {
                                            http_config.time_to_first_token_timeout_ms =
                                                Some(val as u64);
                                        }
                                    }
                                    Err(other) => {
                                        self.push_error(
                                            format!("time_to_first_token_timeout_ms must be an integer. Got: {}", other.r#type()),
                                            other.meta().clone(),
                                        );
                                    }
                                }
                            }
                            "idle_timeout_ms" if !is_composite => {
                                let value_meta = value.meta().clone();
                                match value.into_numeric() {
                                    Ok((val_str, _)) => {
                                        let val = val_str.parse::<i64>().unwrap_or(-1);
                                        if let Err(e) =
                                            validate_timeout_value(val, "idle_timeout_ms")
                                        {
                                            self.push_error(e, value_meta);
                                        } else {
                                            http_config.idle_timeout_ms = Some(val as u64);
                                        }
                                    }
                                    Err(other) => {
                                        self.push_error(
                                            format!(
                                                "idle_timeout_ms must be an integer. Got: {}",
                                                other.r#type()
                                            ),
                                            other.meta().clone(),
                                        );
                                    }
                                }
                            }
                            "total_timeout_ms" if is_composite => {
                                let value_meta = value.meta().clone();
                                match value.into_numeric() {
                                    Ok((val_str, _)) => {
                                        let val = val_str.parse::<i64>().unwrap_or(-1);
                                        if let Err(e) =
                                            validate_timeout_value(val, "total_timeout_ms")
                                        {
                                            self.push_error(e, value_meta);
                                        } else {
                                            http_config.total_timeout_ms = Some(val as u64);
                                        }
                                    }
                                    Err(other) => {
                                        self.push_error(
                                            format!(
                                                "total_timeout_ms must be an integer. Got: {}",
                                                other.r#type()
                                            ),
                                            other.meta().clone(),
                                        );
                                    }
                                }
                            }
                            field => {
                                // Track unrecognized or invalid fields
                                if !allowed_fields.contains(field) {
                                    unrecognized_fields.push(field.to_string());
                                }
                            }
                        }
                    }

                    // Report all unrecognized fields with helpful error message
                    if !unrecognized_fields.is_empty() {
                        // Build error messages with suggestions
                        for unrecognized_field in &unrecognized_fields {
                            let error_msg = if is_composite {
                                // For composite clients
                                if unrecognized_field == "total_timeout_ms" {
                                    // This shouldn't happen as it's in the allowed list for composites
                                    continue;
                                } else if let Some(suggestion) =
                                    find_best_match(unrecognized_field, &["total_timeout_ms"])
                                {
                                    format!(
                                        "Unrecognized field '{unrecognized_field}' in http configuration block. Did you mean '{suggestion}'? \
                                        Composite clients (fallback/round-robin) only support: total_timeout_ms"
                                    )
                                } else {
                                    format!(
                                        "Unrecognized field '{unrecognized_field}' in http configuration block. \
                                        Composite clients (fallback/round-robin) only support: total_timeout_ms"
                                    )
                                }
                            } else {
                                // For regular clients
                                let all_timeout_fields = vec![
                                    "connect_timeout_ms",
                                    "request_timeout_ms",
                                    "time_to_first_token_timeout_ms",
                                    "idle_timeout_ms",
                                    "total_timeout_ms", // Include for suggestions
                                ];

                                if unrecognized_field == "total_timeout_ms" {
                                    // Special case for total_timeout_ms in regular clients
                                    "Unrecognized field 'total_timeout_ms' in http configuration block. \
                                        'total_timeout_ms' is only available for composite clients (fallback/round-robin). \
                                        For regular clients, use: connect_timeout_ms, request_timeout_ms, \
                                        time_to_first_token_timeout_ms, idle_timeout_ms".to_string()
                                } else if let Some(suggestion) =
                                    find_best_match(unrecognized_field, &all_timeout_fields)
                                {
                                    if suggestion == "total_timeout_ms" {
                                        format!(
                                            "Unrecognized field '{unrecognized_field}' in http configuration block. \
                                            Did you mean 'total_timeout_ms'? Note: 'total_timeout_ms' is only \
                                            available for composite clients (fallback/round-robin)"
                                        )
                                    } else {
                                        format!(
                                            "Unrecognized field '{unrecognized_field}' in http configuration block. Did you mean '{suggestion}'?"
                                        )
                                    }
                                } else {
                                    format!(
                                        "Unrecognized field '{unrecognized_field}' in http configuration block. \
                                        Supported timeout fields are: connect_timeout_ms, request_timeout_ms, \
                                        time_to_first_token_timeout_ms, idle_timeout_ms"
                                    )
                                }
                            };

                            self.push_error(error_msg, value_span.clone());
                        }
                    }

                    // Apply defaults for regular (non-composite) clients
                    // Note: 0 means infinite timeout, so we don't override explicit 0 values
                    if !is_composite {
                        if http_config.connect_timeout_ms.is_none() {
                            http_config.connect_timeout_ms = Some(10_000); // 10s default
                        }
                        if http_config.request_timeout_ms.is_none() {
                            http_config.request_timeout_ms = Some(60_000 * 5); // 5 minutes default
                        }
                        // Streaming timeouts have no defaults - they're opt-in
                    }
                    // Composite clients have no defaults for total_timeout_ms

                    http_config
                }
                _ => {
                    self.push_error(
                        "http must be a configuration block with timeout settings",
                        http_value.meta().clone(),
                    );
                    // Apply defaults anyway for regular clients
                    let mut http_config = HttpConfig::default();
                    if provider_type != "fallback" && provider_type != "round-robin" {
                        http_config.connect_timeout_ms = Some(10_000);
                        http_config.request_timeout_ms = Some(60_000 * 5); // 5 minutes
                    }
                    http_config
                }
            }
        } else {
            // No http block - apply defaults for regular clients
            let mut http_config = HttpConfig::default();
            if provider_type != "fallback" && provider_type != "round-robin" {
                http_config.connect_timeout_ms = Some(10_000);
                http_config.request_timeout_ms = Some(60_000 * 5); // 5 minutes
            }
            http_config
        }
    }

    pub fn ensure_headers(&mut self) -> Option<IndexMap<String, StringOr>> {
        self.ensure_map("headers", false).map(|(_, value, _)| {
            value
                .into_iter()
                .filter_map(|(k, (_, v))| match v.as_str() {
                    Some(s) => Some((k, s.clone())),
                    None => {
                        self.push_error(
                            format!(
                                "Header key {} must have a string value. Got: {}",
                                k,
                                v.r#type()
                            ),
                            v.meta().clone(),
                        );
                        None
                    }
                })
                .collect()
        })
    }

    pub fn ensure_strategy(
        &mut self,
    ) -> Option<Vec<(either::Either<StringOr, crate::ClientSpec>, Meta)>> {
        self.ensure_array("strategy", true)
            .map(|(_, value, value_span)| {
                if value.is_empty() {
                    self.push_error("strategy must not be empty", value_span);
                }
                value
                    .into_iter()
                    .filter_map(|v| match v.as_str() {
                        Some(s) => {
                            if let StringOr::Value(value) = s {
                                if let Ok(client_spec) =
                                    crate::ClientSpec::new_from_id(value.as_str()).map_err(|e| {
                                        self.push_error(
                                            format!("Invalid strategy: {e}"),
                                            v.meta().clone(),
                                        );
                                    })
                                {
                                    Some((either::Either::Right(client_spec), v.meta().clone()))
                                } else {
                                    Some((either::Either::Left(s.clone()), v.meta().clone()))
                                }
                            } else {
                                Some((either::Either::Left(s.clone()), v.meta().clone()))
                            }
                        }
                        None => {
                            self.push_error(
                                format!("values in strategy must be strings. Got: {}", v.r#type()),
                                v.meta().clone(),
                            );
                            None
                        }
                    })
                    .collect()
            })
    }

    pub fn finalize_empty(self) -> Vec<Error<Meta>> {
        let mut errors = self.errors;
        for (k, (key_span, _)) in self.options {
            errors.push(Error::new(format!("Unsupported property: {k}"), key_span));
        }
        errors
    }

    pub fn finalize(
        self,
    ) -> (
        IndexMap<String, (Meta, UnresolvedValue<Meta>)>,
        Vec<Error<Meta>>,
    ) {
        (self.options, self.errors)
    }
}

fn ensure_string<Meta: Clone>(
    options: &mut IndexMap<String, (Meta, UnresolvedValue<Meta>)>,
    key: &str,
) -> Result<Option<(Meta, StringOr, Meta)>, Error<Meta>> {
    if let Some((key_span, value)) = options.shift_remove(key) {
        match value.into_str() {
            Ok((s, meta)) => Ok(Some((key_span, s, meta))),
            Err(other) => Err(Error {
                message: format!("{} must be a string. Got: {}", key, other.r#type()),
                span: other.meta().clone(),
            }),
        }
    } else {
        Ok(None)
    }
}

fn ensure_array<Meta: Clone>(
    options: &mut IndexMap<String, (Meta, UnresolvedValue<Meta>)>,
    key: &str,
) -> Result<Option<(Meta, Vec<UnresolvedValue<Meta>>, Meta)>, Error<Meta>> {
    if let Some((key_span, value)) = options.shift_remove(key) {
        match value.into_array() {
            Ok((a, meta)) => Ok(Some((key_span, a, meta))),
            Err(other) => Err(Error {
                message: format!("{} must be an array. Got: {}", key, other.r#type()),
                span: other.meta().clone(),
            }),
        }
    } else {
        Ok(None)
    }
}

fn ensure_map<Meta: Clone>(
    options: &mut IndexMap<String, (Meta, UnresolvedValue<Meta>)>,
    key: &str,
) -> Result<Option<(Meta, IndexMap<String, (Meta, UnresolvedValue<Meta>)>, Meta)>, Error<Meta>> {
    if let Some((key_span, value)) = options.shift_remove(key) {
        match value.into_map() {
            Ok((m, meta)) => Ok(Some((key_span, m, meta))),
            Err(other) => Err(Error {
                message: format!("{} must be a map. Got: {}", key, other.r#type()),
                span: other.meta().clone(),
            }),
        }
    } else {
        Ok(None)
    }
}

fn ensure_bool<Meta: Clone>(
    options: &mut IndexMap<String, (Meta, UnresolvedValue<Meta>)>,
    key: &str,
) -> Result<Option<(Meta, bool, Meta)>, Error<Meta>> {
    if let Some((key_span, value)) = options.shift_remove(key) {
        match value.into_bool() {
            Ok((b, meta)) => Ok(Some((key_span, b, meta))),
            Err(other) => Err(Error {
                message: format!("{} must be a bool. Got: {}", key, other.r#type()),
                span: other.meta().clone(),
            }),
        }
    } else {
        Ok(None)
    }
}

fn ensure_int<Meta: Clone>(
    options: &mut IndexMap<String, (Meta, UnresolvedValue<Meta>)>,
    key: &str,
) -> Result<Option<(Meta, i32, Meta)>, Error<Meta>> {
    if let Some((key_span, value)) = options.shift_remove(key) {
        match value.into_numeric() {
            Ok((i, meta)) => {
                if let Ok(i) = i.parse::<i32>() {
                    Ok(Some((key_span, i, meta)))
                } else {
                    Err(Error {
                        message: format!("{key} must be an integer. Got: {i}"),
                        span: meta,
                    })
                }
            }
            Err(other) => Err(Error {
                message: format!("{} must be an integer. Got: {}", key, other.r#type()),
                span: other.meta().clone(),
            }),
        }
    } else {
        Ok(None)
    }
}

// Helper function to validate timeout values
fn validate_timeout_value(value: i64, field_name: &str) -> Result<(), String> {
    if value < 0 {
        return Err(format!("{field_name} must be non-negative, got: {value}ms"));
    }
    // 0 means infinite timeout (no timeout) - explicitly allowed
    // Any non-negative value is valid according to the updated spec
    Ok(())
}

// Helper function to find the best match for a typo using edit distance
fn find_best_match<'a>(typo: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let typo_lower = typo.to_lowercase();
    let mut best_match = None;
    let mut best_distance = usize::MAX;

    for candidate in candidates {
        let candidate_lower = candidate.to_lowercase();
        let distance = levenshtein_distance(&typo_lower, &candidate_lower);

        // Only suggest if the distance is reasonable (less than half the length)
        if distance < best_distance && distance <= typo_lower.len() / 2 + 1 {
            best_distance = distance;
            best_match = Some(*candidate);
        }
    }

    best_match
}

// Simple Levenshtein distance implementation
fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let len1 = s1.len();
    let len2 = s2.len();

    if len1 == 0 {
        return len2;
    }
    if len2 == 0 {
        return len1;
    }

    let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

    for (i, item) in matrix.iter_mut().enumerate().take(len1 + 1) {
        item[0] = i;
    }
    for (j, item) in matrix[0].iter_mut().enumerate().take(len2 + 1) {
        *item = j;
    }

    for (i, c1) in s1.chars().enumerate() {
        for (j, c2) in s2.chars().enumerate() {
            let cost = if c1 == c2 { 0 } else { 1 };
            matrix[i + 1][j + 1] = std::cmp::min(
                matrix[i][j + 1] + 1, // deletion
                std::cmp::min(
                    matrix[i + 1][j] + 1, // insertion
                    matrix[i][j] + cost,  // substitution
                ),
            );
        }
    }

    matrix[len1][len2]
}

pub(crate) fn get_proxy_url(ctx: &impl GetEnvVar) -> Option<String> {
    if cfg!(target_arch = "wasm32") {
        // We don't want to accidentally set this unless the user explicitly
        // specifies it, so we enforce allow_missing_env_var=false here
        StringOr::EnvVar("BOUNDARY_PROXY_URL".to_string())
            .resolve(&ctx.set_allow_missing_env_var(false))
            .ok()
    } else {
        None
    }
}
