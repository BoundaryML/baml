//! Candidate data structures for prompt/schema optimization
//!
//! A candidate represents a specific version of a function's prompt and
//! associated schema annotations that can be evaluated against tests.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Represents a field in a class schema that can be optimized
/// TODO: Aliases must be singular.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SchemaFieldDefinition {
    pub field_name: String,
    pub field_type: String,
    pub description: Option<String>,
    pub alias: Option<String>,
}

/// Represents a class definition with its fields
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClassDefinition {
    pub class_name: String,
    pub description: Option<String>,
    pub fields: Vec<SchemaFieldDefinition>,
}

/// Represents an enum definition with value descriptions
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EnumDefinition {
    pub enum_name: String,
    pub values: Vec<String>,
    pub value_descriptions: HashMap<String, String>,
}

/// The complete optimizable context for a function
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OptimizableFunction {
    pub function_name: String,
    pub prompt_text: String,
    pub classes: Vec<ClassDefinition>,
    pub enums: Vec<EnumDefinition>,
    /// The full BAML source code of the function
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function_source: Option<String>,
}

/// An example from test execution showing inputs, outputs, and feedback
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReflectiveExample {
    pub inputs: HashMap<String, String>,
    pub generated_outputs: HashMap<String, String>,
    pub feedback: String,
    pub failure_location: Option<String>,
    /// The BAML source code of the test block (including assertions)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_source: Option<String>,
    /// The name of the test
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_name: Option<String>,
    /// Prompt tokens used for this example
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<f64>,
    /// Completion tokens used for this example
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<f64>,
    /// Latency in milliseconds for this example
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<f64>,
}

/// Optimization objectives passed to the reflection function
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OptimizationObjectives {
    /// List of objectives with their weights and current values
    pub objectives: Vec<ObjectiveStatus>,
}

/// Status of a single optimization objective
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ObjectiveStatus {
    /// Name of the objective (e.g., "accuracy", "tokens", "latency")
    pub name: String,
    /// Weight of this objective (0.0 to 1.0, higher = more important)
    pub weight: f64,
    /// Direction: "maximize" or "minimize"
    pub direction: String,
    /// Current value of this objective
    pub current_value: f64,
    /// Human-readable description of current status
    pub status: String,
}

/// Current metrics for the candidate being improved
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CurrentMetrics {
    /// Test pass rate (0.0 to 1.0)
    pub test_pass_rate: f64,
    /// Tests passed / total
    pub tests_passed: usize,
    pub tests_total: usize,
    /// Average prompt tokens
    pub avg_prompt_tokens: f64,
    /// Average completion tokens
    pub avg_completion_tokens: f64,
    /// Total average tokens (prompt + completion)
    pub avg_total_tokens: f64,
    /// Average latency in milliseconds
    pub avg_latency_ms: f64,
}

/// The result of reflection: improved prompt and schema
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImprovedFunction {
    pub prompt_text: String,
    pub classes: Vec<ClassDefinition>,
    pub enums: Vec<EnumDefinition>,
    pub rationale: String,
}

/// How a candidate was created
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CandidateMethod {
    /// Initial candidate from user's original code
    Initial,
    /// Created by reflecting on failures
    Reflection,
    /// Created by merging two successful candidates
    Merge,
}

/// Scores from evaluating a candidate
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CandidateScores {
    /// Fraction of tests that passed (0.0 to 1.0)
    pub test_pass_rate: f64,
    /// Number of tests that passed
    pub tests_passed: usize,
    /// Total number of tests run
    pub tests_total: usize,
    /// Average prompt tokens across all test runs
    pub avg_prompt_tokens: f64,
    /// Average completion tokens across all test runs
    pub avg_completion_tokens: f64,
    /// Average latency in milliseconds
    pub avg_latency_ms: f64,
    /// Scores for named @@check constraints (0.0 to 1.0)
    pub check_scores: HashMap<String, f64>,
}

impl CandidateScores {
    /// Create scores from test results
    pub fn from_test_results(
        tests_passed: usize,
        tests_total: usize,
        prompt_tokens: Vec<f64>,
        completion_tokens: Vec<f64>,
        latencies_ms: Vec<f64>,
        // TODO: This should be a map from String to usize
        // (the number of passing checks with each key name).
        check_results: HashMap<String, Vec<bool>>,
    ) -> Self {
        let avg = |v: &[f64]| {
            if v.is_empty() {
                0.0
            } else {
                v.iter().sum::<f64>() / v.len() as f64
            }
        };

        let check_scores = check_results
            .into_iter()
            .map(|(name, results)| {
                let passed = results.iter().filter(|&&b| b).count();
                let rate = if results.is_empty() {
                    0.0
                } else {
                    passed as f64 / results.len() as f64
                };
                (name, rate)
            })
            .collect();

        Self {
            test_pass_rate: if tests_total == 0 {
                0.0
            } else {
                tests_passed as f64 / tests_total as f64
            },
            tests_passed,
            tests_total,
            avg_prompt_tokens: avg(&prompt_tokens),
            avg_completion_tokens: avg(&completion_tokens),
            avg_latency_ms: avg(&latencies_ms),
            check_scores,
        }
    }
}

/// A candidate prompt/schema configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    /// Unique identifier for this candidate
    pub id: usize,
    /// Which optimization iteration created this candidate
    pub iteration: usize,
    /// IDs of parent candidates (empty for initial)
    pub parent_ids: Vec<usize>,
    /// How this candidate was created
    pub method: CandidateMethod,
    /// The optimizable function definition
    pub function: OptimizableFunction,
    /// Scores from evaluation (None if not yet evaluated)
    pub scores: Option<CandidateScores>,
    /// Rationale from the reflection/merge that created this candidate
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}

impl Candidate {
    /// Create the initial candidate from user's code
    pub fn initial(function: OptimizableFunction) -> Self {
        Self {
            id: 0,
            iteration: 0,
            parent_ids: vec![],
            method: CandidateMethod::Initial,
            function,
            scores: None,
            rationale: None,
        }
    }

    /// Create a new candidate from reflection
    pub fn from_reflection(
        id: usize,
        iteration: usize,
        parent_id: usize,
        base_function: &OptimizableFunction,
        improved: ImprovedFunction,
    ) -> Self {
        // Merge the improved function with the base
        let function = OptimizableFunction {
            function_name: base_function.function_name.clone(),
            prompt_text: improved.prompt_text.clone(),
            classes: merge_classes(&base_function.classes, &improved.classes),
            enums: merge_enums(&base_function.enums, &improved.enums),
            function_source: base_function.function_source.clone(),
        };

        Self {
            id,
            iteration,
            parent_ids: vec![parent_id],
            method: CandidateMethod::Reflection,
            function,
            scores: None,
            rationale: Some(improved.rationale),
        }
    }

    /// Create a new candidate from merging two parents
    pub fn from_merge(
        id: usize,
        iteration: usize,
        parent_a_id: usize,
        parent_b_id: usize,
        base_function: &OptimizableFunction,
        improved: ImprovedFunction,
    ) -> Self {
        let function = OptimizableFunction {
            function_name: base_function.function_name.clone(),
            prompt_text: improved.prompt_text.clone(),
            classes: merge_classes(&base_function.classes, &improved.classes),
            enums: merge_enums(&base_function.enums, &improved.enums),
            function_source: base_function.function_source.clone(),
        };

        Self {
            id,
            iteration,
            parent_ids: vec![parent_a_id, parent_b_id],
            method: CandidateMethod::Merge,
            function,
            scores: None,
            rationale: Some(improved.rationale),
        }
    }

    /// Check if this candidate has been evaluated
    pub fn is_evaluated(&self) -> bool {
        self.scores.is_some()
    }
}

/// Merge improved classes with base classes
/// Improved classes override base classes with the same name
fn merge_classes(base: &[ClassDefinition], improved: &[ClassDefinition]) -> Vec<ClassDefinition> {
    let mut result = base.to_vec();

    for imp_class in improved {
        if let Some(existing) = result
            .iter_mut()
            .find(|c| c.class_name == imp_class.class_name)
        {
            // Override with improved version
            *existing = imp_class.clone();
        } else {
            // This shouldn't happen in practice - GEPA shouldn't add new classes
            result.push(imp_class.clone());
        }
    }

    result
}

/// Merge improved enums with base enums
fn merge_enums(base: &[EnumDefinition], improved: &[EnumDefinition]) -> Vec<EnumDefinition> {
    let mut result = base.to_vec();

    for imp_enum in improved {
        if let Some(existing) = result
            .iter_mut()
            .find(|e| e.enum_name == imp_enum.enum_name)
        {
            *existing = imp_enum.clone();
        } else {
            result.push(imp_enum.clone());
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_candidate_scores_from_results() {
        let scores = CandidateScores::from_test_results(
            3,
            5,
            vec![100.0, 120.0, 110.0, 105.0, 115.0],
            vec![50.0, 60.0, 55.0, 52.0, 58.0],
            vec![1000.0, 1200.0, 1100.0, 1050.0, 1150.0],
            HashMap::from([
                ("check1".to_string(), vec![true, true, false, true, false]),
                ("check2".to_string(), vec![true, true, true, true, true]),
            ]),
        );

        assert_eq!(scores.test_pass_rate, 0.6);
        assert_eq!(scores.tests_passed, 3);
        assert_eq!(scores.tests_total, 5);
        assert_eq!(scores.avg_prompt_tokens, 110.0);
        assert_eq!(scores.avg_completion_tokens, 55.0);
        assert_eq!(scores.avg_latency_ms, 1100.0);
        assert_eq!(scores.check_scores.get("check1"), Some(&0.6));
        assert_eq!(scores.check_scores.get("check2"), Some(&1.0));
    }
}
