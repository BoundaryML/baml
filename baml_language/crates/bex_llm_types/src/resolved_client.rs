//! Fully resolved LLM client configuration.
//!
//! This type represents an LLM client after all environment variables have been
//! resolved. It contains everything needed for both prompt rendering and request building.

use std::collections::HashMap;

use indexmap::IndexMap;

/// A fully resolved LLM client configuration.
///
/// This is constructed once at client creation time (after env var resolution)
/// and reused for every `render_prompt()` and `build_request()` call.
#[derive(Debug, Clone)]
pub struct ResolvedClient {
    /// Client name (e.g., "my-gpt4-client").
    pub name: String,

    /// Provider identifier (e.g., "openai", "anthropic", "openai-generic").
    pub provider: String,

    /// Role configuration for prompt rendering.
    pub roles: RoleConfig,

    /// Model capabilities and features.
    pub features: ModelFeatures,

    /// Client options/properties (model, temperature, etc.) - fully resolved.
    /// These are available in jinja templates via `_.client.options.*`.
    pub options: IndexMap<String, serde_json::Value>,

    /// Request configuration (streaming, timeouts, finish reasons).
    pub request_config: RequestConfig,
}

/// Role configuration for message handling.
#[derive(Debug, Clone, Default)]
pub struct RoleConfig {
    /// The default role to use when no role is specified.
    pub default_role: String,

    /// The roles that this client supports (e.g., "system", "user", "assistant").
    /// Empty means all roles are allowed.
    pub allowed_roles: Vec<String>,

    /// Role remapping (e.g., "system" -> "user" for clients that don't support system).
    /// Applied after validation.
    pub remap_roles: HashMap<String, String>,

    /// Which metadata fields are allowed on messages.
    pub allowed_metadata: AllowedMetadata,
}

/// Which metadata fields are allowed on chat messages.
#[derive(Debug, Clone, Default)]
pub enum AllowedMetadata {
    /// Allow all metadata fields.
    #[default]
    All,
    /// No metadata fields allowed.
    None,
    /// Only these specific fields are allowed.
    Only(Vec<String>),
}

impl AllowedMetadata {
    /// Check if a metadata key is allowed.
    pub fn is_allowed(&self, key: &str) -> bool {
        match self {
            AllowedMetadata::All => true,
            AllowedMetadata::None => false,
            AllowedMetadata::Only(keys) => keys.iter().any(|k| k == key),
        }
    }
}

/// Model capabilities and features that affect prompt transformation.
#[derive(Debug, Clone)]
pub struct ModelFeatures {
    /// Supports completion API (non-chat).
    pub completion: bool,

    /// Supports chat API.
    pub chat: bool,

    /// If true, consolidate multiple system messages into one.
    /// Some models (e.g., o1) require at most one system message.
    pub max_one_system_prompt: bool,

    /// How to handle media URLs for different media types.
    pub media_resolution: MediaResolutionConfig,
}

impl Default for ModelFeatures {
    fn default() -> Self {
        Self {
            completion: false,
            chat: true,
            max_one_system_prompt: false,
            media_resolution: MediaResolutionConfig::default(),
        }
    }
}

/// Configuration for how to resolve media URLs by type.
#[derive(Debug, Clone, Default)]
pub struct MediaResolutionConfig {
    pub images: MediaResolution,
    pub audio: MediaResolution,
    pub video: MediaResolution,
    pub pdf: MediaResolution,
}

/// How to handle media URLs when building requests.
#[derive(Debug, Clone, Copy, Default)]
pub enum MediaResolution {
    /// Always convert to base64.
    SendBase64,
    /// Pass URLs unchanged.
    #[default]
    SendUrl,
    /// Pass URLs but ensure MIME type is present.
    SendUrlWithMimeType,
    /// Keep gs:// URLs, convert others to base64 (for Google/Vertex).
    SendBase64UnlessGoogleUrl,
}

/// Request-level configuration.
#[derive(Debug, Clone, Default)]
pub struct RequestConfig {
    /// Whether streaming is supported.
    pub supports_streaming: bool,

    /// Filter for acceptable finish reasons.
    pub finish_reason_filter: FinishReasonFilter,

    /// HTTP timeout configuration.
    pub timeouts: TimeoutConfig,
}

/// Filter for which finish reasons are acceptable.
#[derive(Debug, Clone, Default)]
pub enum FinishReasonFilter {
    /// Accept all finish reasons.
    #[default]
    All,
    /// Only accept these specific reasons.
    AllowList(Vec<String>),
    /// Reject these specific reasons.
    DenyList(Vec<String>),
}

impl FinishReasonFilter {
    /// Check if a finish reason is allowed.
    pub fn is_allowed(&self, reason: &str) -> bool {
        match self {
            FinishReasonFilter::All => true,
            FinishReasonFilter::AllowList(list) => list.iter().any(|r| r == reason),
            FinishReasonFilter::DenyList(list) => !list.iter().any(|r| r == reason),
        }
    }
}

/// HTTP timeout configuration.
#[derive(Debug, Clone, Default)]
pub struct TimeoutConfig {
    /// Connection timeout in milliseconds.
    pub connect_timeout_ms: Option<u64>,
    /// Request timeout in milliseconds.
    pub request_timeout_ms: Option<u64>,
    /// Time to first token timeout in milliseconds (for streaming).
    pub time_to_first_token_timeout_ms: Option<u64>,
    /// Idle connection timeout in milliseconds.
    pub idle_timeout_ms: Option<u64>,
    /// Total timeout in milliseconds.
    pub total_timeout_ms: Option<u64>,
}

impl ResolvedClient {
    /// Get the default role for this client.
    pub fn default_role(&self) -> &str {
        &self.roles.default_role
    }

    /// Get allowed roles, or a default set if empty.
    pub fn allowed_roles_or(&self, default: &[&str]) -> Vec<String> {
        if self.roles.allowed_roles.is_empty() {
            default.iter().map(|s| (*s).to_string()).collect()
        } else {
            self.roles.allowed_roles.clone()
        }
    }

    /// Remap a role according to the client's remap configuration.
    pub fn remap_role<'a>(&'a self, role: &'a str) -> &'a str {
        self.roles
            .remap_roles
            .get(role)
            .map(String::as_str)
            .unwrap_or(role)
    }

    /// Check if a metadata key is allowed.
    pub fn is_metadata_allowed(&self, key: &str) -> bool {
        self.roles.allowed_metadata.is_allowed(key)
    }

    /// Check if streaming is supported.
    pub fn supports_streaming(&self) -> bool {
        self.request_config.supports_streaming
    }
}
