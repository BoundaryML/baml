use std::{borrow::Cow, collections::HashSet};

use baml_types::{GetEnvVar, StringOr, UnresolvedValue};
use indexmap::IndexMap;

use crate::{
    SupportedRequestModes, UnresolvedAllowedRoleMetadata, UnresolvedFinishReasonFilter,
    UnresolvedResponseType, UnresolvedRolesSelection,
};

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
        println!(
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
