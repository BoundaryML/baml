use anyhow::{Context, Result};
use std::collections::HashMap;

use super::AllowedMetadata;

pub enum FinishReasonOptions {
    AllowList(Vec<String>),
    DenyList(Vec<String>),
}

impl FinishReasonOptions {
    pub fn is_allowed(&self, finish_reason: &str) -> bool {
        if finish_reason.is_empty() {
            return true;
        }
        match self {
            FinishReasonOptions::AllowList(allowlist) => allowlist
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(finish_reason)),
            FinishReasonOptions::DenyList(deny_list) => !deny_list
                .iter()
                .any(|denied| denied.eq_ignore_ascii_case(finish_reason)),
        }
    }
}

pub(super) struct PropertiesHandler {
    properties: HashMap<String, serde_json::Value>,
}

impl PropertiesHandler {
    pub fn new(properties: HashMap<String, serde_json::Value>) -> Self {
        Self { properties }
    }

    pub fn finalize(self) -> HashMap<String, serde_json::Value> {
        self.properties
    }

    fn get(&mut self, key: &str) -> Result<Option<serde_json::Value>> {
        Ok(self.properties.remove(key))
    }

    pub fn remove(&mut self, key: &str) -> Result<Option<serde_json::Value>> {
        // Ban certain keys
        match key {
            "finish_reason_allowlist"
            | "finish_reason_denylist"
            | "allowed_role_metadata"
            | "base_url"
            | "api_key"
            | "headers"
            | "default_role" => {
                anyhow::bail!("{} is a reserved key in options", key)
            }
            _ => Ok(self.properties.remove(key)),
        }
    }

    pub fn remove_str(&mut self, key: &str) -> Result<Option<String>> {
        // Ban certain keys
        match self.remove(key)? {
            Some(value) => match value.as_str() {
                Some(s) => Ok(Some(s.to_string())),
                None => anyhow::bail!("{} must be a string", key),
            },
            None => Ok(None),
        }
    }

    pub fn pull_finish_reason_options(&mut self) -> Result<Option<FinishReasonOptions>> {
        let allowlist = match self.get("finish_reason_allowlist")? {
            Some(value) => match value.as_array() {
                Some(array) => array
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<String>>(),
                None => anyhow::bail!("finish_reason_allowlist must be an array of strings"),
            },
            None => vec![],
        };

        let denylist = match self.get("finish_reason_denylist")? {
            Some(value) => match value.as_array() {
                Some(array) => array
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<String>>(),
                None => anyhow::bail!("finish_reason_denylist must be an array of strings"),
            },
            None => vec![],
        };

        Ok(match (allowlist.is_empty(), denylist.is_empty()) {
            (false, true) => Some(FinishReasonOptions::AllowList(allowlist)),
            (true, false) => Some(FinishReasonOptions::DenyList(denylist)),
            (false, false) => anyhow::bail!(
                "Only one of finish_reason_allowlist or finish_reason_denylist can be specified"
            ),
            (true, true) => None,
        })
    }

    pub fn pull_headers(&mut self) -> Result<HashMap<String, String>> {
        let headers = self.get("headers")?.map(|v| {
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

        Ok(headers)
    }

    pub fn pull_allowed_role_metadata(&mut self) -> Result<AllowedMetadata> {
        let allowed_metadata = match self.get("allowed_role_metadata")? {
            Some(allowed_metadata) => serde_json::from_value(allowed_metadata).context(
                "allowed_role_metadata must be an array of keys. For example: ['key1', 'key2']",
            )?,
            None => AllowedMetadata::None,
        };

        Ok(allowed_metadata)
    }

    pub fn pull_base_url(&mut self) -> Result<Option<String>> {
        self.get("base_url")?.map_or(Ok(None), |v| {
            match v
                .as_str()
                .map(|s| Some(s))
                .ok_or_else(|| anyhow::anyhow!("base_url must be a string"))?
            {
                Some(s) if s.is_empty() => {
                    anyhow::bail!("base_url must be a non-empty string")
                }
                Some(s) => Ok(Some(s.to_string())),
                None => Ok(None),
            }
        })
    }

    pub fn pull_default_role(&mut self, default: &str) -> Result<String> {
        let default_role = self.get("default_role")?.map(|v| {
            v.as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| anyhow::anyhow!("default_role must be a string"))
        });
        match default_role {
            Some(Ok(role)) => Ok(role),
            Some(Err(e)) => Err(e),
            None => Ok(default.to_string()),
        }
    }

    pub fn pull_api_key(&mut self) -> Result<Option<String>> {
        let api_key = self.get("api_key")?.map(|v| {
            v.as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| anyhow::anyhow!("api_key must be a string"))
        });
        match api_key {
            Some(Ok(key)) => Ok(Some(key)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }
}

impl crate::client_registry::ClientProperty {
    pub(super) fn property_handler(&self) -> Result<PropertiesHandler> {
        Ok(PropertiesHandler::new(
            self.options
                .iter()
                .map(|(k, v)| Ok((k.clone(), serde_json::json!(v))))
                .collect::<Result<HashMap<_, _>>>()?,
        ))
    }
}
