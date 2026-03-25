// TODO: direct copy from baml_builtins2/baml_std/baml/llm_types.baml
// later we can replace it accordingly
pub struct PrimitiveClient {
    pub name: String,
    pub provider: String,
    pub(crate) options: PrimitiveClientOptions,
}

impl PrimitiveClient {
    pub fn new(name: String, provider: String, options: PrimitiveClientOptions) -> Self {
        Self {
            name,
            provider,
            options,
        }
    }

    pub fn default_role(&self) -> String {
        self.options
            .default_role
            .clone()
            .unwrap_or_else(|| "system".to_string())
    }

    pub fn allowed_roles(&self) -> Vec<String> {
        self.options
            .allowed_roles
            .clone()
            .unwrap_or_else(|| vec!["user".to_string()])
    }

    pub fn is_finish_reason_allowed(&self, finish_reason: Option<&str>) -> bool {
        let Some(finish_reason) = finish_reason else {
            return true;
        };
        let finish_reason = &finish_reason.to_ascii_lowercase();
        match (
            &self.options.allowed_roles_allow_list,
            &self.options.allowed_roles_deny_list,
        ) {
            (Some(allowed_roles_allow_list), None) => {
                allowed_roles_allow_list.contains(finish_reason)
            }
            (None, Some(allowed_roles_deny_list)) => {
                !allowed_roles_deny_list.contains(finish_reason)
            }
            (Some(allowed_roles_allow_list), Some(allowed_roles_deny_list)) => {
                allowed_roles_allow_list.contains(finish_reason)
                    && !allowed_roles_deny_list.contains(finish_reason)
            }
            (None, None) => true,
        }
    }
}

#[derive(Default)]
pub struct PrimitiveClientOptions {
    pub model: Option<String>,
    pub max_one_system_prompt: Option<bool>,
    pub allowed_role_metadata: Option<bex_heap::BexExternalValue>,
    pub allowed_roles_allow_list: Option<Vec<String>>,
    pub allowed_roles_deny_list: Option<Vec<String>>,
    pub base_url: Option<String>,
    pub default_role: Option<String>,
    pub allowed_roles: Option<Vec<String>>,
    pub remap_roles: Option<indexmap::IndexMap<String, String>>,
    pub api_key: Option<String>,
    pub headers: indexmap::IndexMap<String, String>,
    pub query_params: indexmap::IndexMap<String, String>,
    // openai specific options
    pub resource_name: Option<String>,
    pub api_version: Option<String>,
    // anthropic specific options
    pub anthropic_version: Option<String>,
    // request body
    pub request_body: indexmap::IndexMap<String, bex_heap::BexExternalValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: indexmap::IndexMap<String, String>,
    pub body: String,
}
