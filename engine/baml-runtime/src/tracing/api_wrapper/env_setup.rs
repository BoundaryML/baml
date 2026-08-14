use anyhow::Result;

#[derive(Debug)]
pub struct Config {
    pub base_url: String,
    pub secret: Option<String>,
    pub project_id: Option<String>,
    pub sessions_id: String,
    pub stage: String,
    pub host_name: String,
    pub log_redaction_enabled: bool,
    pub log_redaction_placeholder: String,
    pub max_log_chunk_chars: usize,
}

fn default_base_url() -> String {
    "https://app.boundaryml.com/api".to_string()
}

fn default_redaction_placeholder() -> String {
    "<BAML_LOG_REDACTED>".to_string()
}

fn default_sessions_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn default_stage() -> String {
    "development".to_string()
}

fn default_host_name() -> String {
    #[cfg(target_arch = "wasm32")]
    return "<browser>".to_string();

    #[cfg(not(target_arch = "wasm32"))]
    hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or("unknown".to_string())
}

fn default_max_log_chunk_chars() -> usize {
    64_000
}

impl Config {
    pub fn from_env_vars<T: AsRef<str>>(env_vars: impl Iterator<Item = (T, T)>) -> Result<Self> {
        // Mirror `envy::prefixed("BOUNDARY_")`: keep only BOUNDARY_-prefixed keys,
        // strip the prefix, and lowercase the remainder to match field names.
        let map: std::collections::HashMap<String, String> = env_vars
            .filter_map(|(k, v)| {
                k.as_ref()
                    .strip_prefix("BOUNDARY_")
                    .map(|rest| (rest.to_ascii_lowercase(), v.as_ref().to_string()))
            })
            .collect();

        let get = |k: &str| map.get(k).cloned();

        fn parse_field<T>(key: &str, raw: &str) -> Result<T>
        where
            T: std::str::FromStr,
            T::Err: std::fmt::Display,
        {
            raw.parse::<T>().map_err(|e| {
                anyhow::anyhow!(
                    "Failed to parse config from environment variables: BOUNDARY_{}: {e}",
                    key.to_ascii_uppercase()
                )
            })
        }

        let config = Config {
            base_url: get("base_url").unwrap_or_else(default_base_url),
            secret: get("secret"),
            project_id: get("project_id"),
            sessions_id: get("sessions_id").unwrap_or_else(default_sessions_id),
            stage: get("stage").unwrap_or_else(default_stage),
            host_name: get("host_name").unwrap_or_else(default_host_name),
            log_redaction_enabled: match get("log_redaction_enabled") {
                Some(s) => parse_field("log_redaction_enabled", &s)?,
                None => false,
            },
            log_redaction_placeholder: get("log_redaction_placeholder")
                .unwrap_or_else(default_redaction_placeholder),
            max_log_chunk_chars: match get("max_log_chunk_chars") {
                Some(s) => parse_field("max_log_chunk_chars", &s)?,
                None => default_max_log_chunk_chars(),
            },
        };

        Ok(config.normalize())
    }

    pub fn normalize(mut self) -> Self {
        if self.base_url.is_empty() {
            self.base_url = default_base_url();
        }
        if self.sessions_id.is_empty() {
            self.sessions_id = default_sessions_id();
        }
        if self.stage.is_empty() {
            self.stage = default_stage();
        }
        if self.host_name.is_empty() {
            self.host_name = default_host_name();
        }
        if self.log_redaction_placeholder.is_empty() {
            self.log_redaction_placeholder = default_redaction_placeholder();
        }
        // max_log_chunk_chars is usize, so no need to check for empty string
        self
    }
}
