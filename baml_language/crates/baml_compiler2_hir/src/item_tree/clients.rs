use baml_base::Name;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    pub name: Name,
    /// Provider name (e.g., "openai", "anthropic", "fallback", "round-robin").
    pub provider: Option<Name>,
    /// Sub-client names for fallback/round-robin clients.
    pub sub_client_names: Vec<Name>,
    /// Retry policy name, if configured.
    pub retry_policy_name: Option<Name>,
    /// Starting index for round-robin clients.
    pub round_robin_start: Option<usize>,
}
