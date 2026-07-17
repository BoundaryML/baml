use baml_base::Name;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicy {
    pub name: Name,
    /// Raw string value of `max_retries` (parsed at emit time).
    pub max_retries: Option<String>,
    /// Raw string value of `initial_delay_ms`.
    pub initial_delay_ms: Option<String>,
    /// Raw string value of multiplier.
    pub multiplier: Option<String>,
    /// Raw string value of `max_delay_ms`.
    pub max_delay_ms: Option<String>,
}
