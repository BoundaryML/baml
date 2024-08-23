mod azure;
mod ollama;
mod openai;

use std::collections::HashMap;

pub use azure::resolve_properties as resolve_azure_properties;
pub use ollama::resolve_properties as resolve_ollama_properties;
pub use openai::resolve_properties as resolve_openai_properties;

use crate::internal::llm_client::AllowedMetadata;

pub struct PostRequestProperities {
    pub default_role: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub headers: HashMap<String, String>,
    pub query_params: HashMap<String, String>,
    pub proxy_url: Option<String>,
    // These are passed directly to the OpenAI API.
    pub properties: HashMap<String, serde_json::Value>,
    pub allowed_metadata: AllowedMetadata,
}
