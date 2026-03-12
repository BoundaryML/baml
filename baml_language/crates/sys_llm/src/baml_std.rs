// TODO: direct copy from baml_builtins2/baml_std/baml/llm_types.baml
// later we can replace it accordingly
pub struct PrimitiveClient {
    pub name: String,
    pub provider: String,
    pub default_role: String,
    pub allowed_roles: Vec<String>,
    pub options: indexmap::IndexMap<String, bex_heap::BexExternalValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: indexmap::IndexMap<String, String>,
    pub body: String,
}
