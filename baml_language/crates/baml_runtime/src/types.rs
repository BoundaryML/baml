//! Common types used throughout the runtime.

use std::time::Duration;

// Re-export core types from baml_types
pub use baml_types::{BamlMap, BamlValue, BamlValueWithMeta, BamlMedia, BamlMediaType, TypeIR};

/// Result of a function execution.
#[derive(Debug, Clone)]
pub struct FunctionResult {
    /// The parsed output value.
    pub value: BamlValue,
    /// All orchestration attempts made.
    pub attempts: Vec<OrchestrationAttemptSummary>,
    /// Total execution duration.
    pub duration: Duration,
}

/// Summary of an orchestration attempt (for result reporting).
#[derive(Debug, Clone)]
pub struct OrchestrationAttemptSummary {
    /// Which client was used.
    pub client_name: String,
    /// Whether this attempt succeeded.
    pub success: bool,
    /// Error message if failed.
    pub error: Option<String>,
    /// Duration of this attempt.
    pub duration: Duration,
}

/// Partial result during streaming.
#[derive(Debug, Clone)]
pub struct PartialResult {
    /// The partially parsed value (may be incomplete).
    pub value: BamlValue,
    /// Raw accumulated content from the stream.
    pub raw_content: String,
}

/// Result of running a test.
#[derive(Debug, Clone)]
pub struct TestResult {
    /// The function execution result.
    pub function_result: FunctionResult,
    /// Results of evaluating @assert/@check constraints.
    pub constraint_results: Vec<ConstraintResult>,
}

/// Result of evaluating a single constraint.
#[derive(Debug, Clone)]
pub struct ConstraintResult {
    /// Name of the constraint.
    pub name: String,
    /// Whether the constraint passed.
    pub passed: bool,
    /// Optional message explaining the result.
    pub message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_baml_value_string() {
        let v = BamlValue::from("hello");
        assert_eq!(v.as_str(), Some("hello"));
    }

    #[test]
    fn test_baml_value_int() {
        let v = BamlValue::from(42i64);
        assert_eq!(v.as_int(), Some(42));
    }

    #[test]
    fn test_baml_value_map() {
        let mut map = BamlMap::new();
        map.insert("key".to_string(), BamlValue::from("value"));
        let v = BamlValue::Map(map);

        if let BamlValue::Map(m) = v {
            assert_eq!(m.get("key").and_then(|v| v.as_str()), Some("value"));
        } else {
            panic!("Expected Map");
        }
    }
}
