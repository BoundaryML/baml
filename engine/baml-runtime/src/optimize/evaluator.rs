//! Evaluator - Run tests and collect scores for candidates
//!
//! This module handles running BAML tests against a candidate's prompt/schema
//! configuration and collecting the resulting metrics.

use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result};
use baml_types::BamlValue;

use super::candidate::{CandidateScores, ReflectiveExample};
use crate::{BamlRuntime, TestFailReason, TestStatus, TripWire};

/// Evaluates candidates by running tests
pub struct Evaluator {
    env_vars: HashMap<String, String>,
    parallel: usize,
}

/// Classification of test failure with structured information where available.
/// This is an owned version that can be stored independently of TestFailReason.
#[derive(Debug, Clone)]
pub enum FailureKind {
    Unspecified,
    /// LLM call failure with structured error code information
    LLMFailure(LLMFailureInfo),
    ParseFailure,
    FinishReasonFailed,
    ConstraintsFailure {
        checks: Vec<(String, bool)>,
        failed_assert: Option<String>,
    },
}

/// Structured information about an LLM failure
#[derive(Debug, Clone)]
pub struct LLMFailureInfo {
    pub error_code: crate::internal::llm_client::ErrorCode,
    pub message: String,
    pub client: String,
}

impl<'a> From<&TestFailReason<'a>> for FailureKind {
    fn from(reason: &TestFailReason<'a>) -> Self {
        match reason {
            TestFailReason::TestUnspecified(_) => FailureKind::Unspecified,
            TestFailReason::TestLLMFailure(llm_resp) => {
                FailureKind::LLMFailure(LLMFailureInfo::from_llm_response(llm_resp))
            }
            TestFailReason::TestParseFailure(_) => FailureKind::ParseFailure,
            TestFailReason::TestFinishReasonFailed(_) => FailureKind::FinishReasonFailed,
            TestFailReason::TestConstraintsFailure {
                checks,
                failed_assert,
            } => FailureKind::ConstraintsFailure {
                checks: checks.clone(),
                failed_assert: failed_assert.clone(),
            },
        }
    }
}

impl LLMFailureInfo {
    /// Extract structured error information from LLMResponse
    fn from_llm_response(llm_resp: &crate::internal::llm_client::LLMResponse) -> Self {
        match llm_resp {
            crate::internal::llm_client::LLMResponse::LLMFailure(err) => LLMFailureInfo {
                error_code: err.code.clone(),
                message: err.message.clone(),
                client: err.client.clone(),
            },
            crate::internal::llm_client::LLMResponse::UserFailure(msg) => LLMFailureInfo {
                error_code: crate::internal::llm_client::ErrorCode::Other(0),
                message: msg.clone(),
                client: "unknown".to_string(),
            },
            crate::internal::llm_client::LLMResponse::InternalFailure(msg) => LLMFailureInfo {
                error_code: crate::internal::llm_client::ErrorCode::Other(1),
                message: msg.clone(),
                client: "unknown".to_string(),
            },
            crate::internal::llm_client::LLMResponse::Cancelled(msg) => LLMFailureInfo {
                error_code: crate::internal::llm_client::ErrorCode::Other(2),
                message: msg.clone(),
                client: "unknown".to_string(),
            },
            crate::internal::llm_client::LLMResponse::Success(_) => {
                // This shouldn't happen in a failure reason, but handle it gracefully
                LLMFailureInfo {
                    error_code: crate::internal::llm_client::ErrorCode::Other(3),
                    message: "Unexpected success response in failure reason".to_string(),
                    client: "unknown".to_string(),
                }
            }
        }
    }
}

/// Result of evaluating a single test
#[derive(Debug)]
pub struct TestEvalResult {
    pub function_name: String,
    pub test_name: String,
    pub passed: bool,
    pub prompt_tokens: Option<f64>,
    pub completion_tokens: Option<f64>,
    pub latency_ms: f64,
    pub check_results: HashMap<String, bool>,
    pub error: Option<String>,
    pub output: Option<String>,
    pub inputs: HashMap<String, String>,
    /// Structured failure information for more reliable error categorization.
    /// Only LLM failures have detailed structured info; other failure types are
    /// categorized based on the TestFailReason variant.
    pub failure_kind: Option<FailureKind>,
}

impl Evaluator {
    /// Create a new evaluator
    pub fn new(env_vars: HashMap<String, String>, parallel: usize) -> Self {
        Self { env_vars, parallel }
    }

    /// Evaluate by running all tests for a function
    pub async fn evaluate(
        &self,
        runtime: Arc<BamlRuntime>,
        function_name: &str,
        test_filter: &crate::test_executor::TestFilter,
    ) -> Result<(CandidateScores, Vec<TestEvalResult>)> {
        use crate::InternalRuntimeInterface;

        // Get tests for this function
        let ir = runtime.ir();
        let function = ir
            .walk_functions()
            .find(|f| f.name() == function_name)
            .with_context(|| format!("Function '{}' not found", function_name))?;

        let tests: Vec<_> = function
            .elem()
            .tests
            .iter()
            .filter(|t| test_filter.includes(function_name, &t.elem.name))
            .collect();

        if tests.is_empty() {
            anyhow::bail!("No tests found for function '{}'", function_name);
        }

        // Run tests
        let mut results = Vec::new();
        let semaphore = Arc::new(tokio::sync::Semaphore::new(self.parallel));

        let mut handles = Vec::new();

        for test_node in tests {
            let test_name = test_node.elem.name.clone();
            // Extract test inputs from the test definition
            let test_inputs: HashMap<String, String> = test_node
                .elem
                .args
                .iter()
                .map(|(k, v)| (k.clone(), format!("{:?}", v)))
                .collect();

            let runtime = runtime.clone();
            let env_vars = self.env_vars.clone();
            let function_name = function_name.to_string();
            let semaphore = semaphore.clone();

            let handle = tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();

                let ctx_manager =
                    runtime.create_ctx_manager(BamlValue::String("optimize".to_string()), None);

                let start = std::time::Instant::now();

                let (result, _call_id) = runtime
                    .run_test(
                        &function_name,
                        &test_name,
                        &ctx_manager,
                        None::<fn(crate::FunctionResult)>,
                        None,
                        env_vars,
                        None,
                        TripWire::new(None),
                        None::<fn()>,
                        None,
                    )
                    .await;

                let latency_ms = start.elapsed().as_millis() as f64;

                match result {
                    Ok(response) => {
                        let status = response.status();
                        let passed = matches!(status, TestStatus::Pass);

                        // Extract the actual output from the response
                        let output = response_output(&response);

                        // Extract detailed error information
                        let (error, check_results, failure_kind) = error_details(&status);

                        // Extract token counts from LLM response if available
                        let (prompt_tokens, completion_tokens) = response
                            .function_response
                            .as_ref()
                            .and_then(|fr| {
                                let llm_resp = fr.llm_response();
                                match llm_resp {
                                    crate::internal::llm_client::LLMResponse::Success(complete) => {
                                        complete
                                            .metadata
                                            .prompt_tokens
                                            .zip(complete.metadata.output_tokens)
                                    }
                                    _ => None,
                                }
                            })
                            .map(|(p, c)| (Some(p as f64), Some(c as f64)))
                            .unwrap_or((None, None));

                        TestEvalResult {
                            function_name,
                            test_name,
                            passed,
                            prompt_tokens,
                            completion_tokens,
                            latency_ms,
                            check_results,
                            error,
                            output,
                            inputs: test_inputs,
                            failure_kind,
                        }
                    }
                    Err(e) => TestEvalResult {
                        function_name,
                        test_name,
                        passed: false,
                        prompt_tokens: None,
                        completion_tokens: None,
                        latency_ms,
                        check_results: HashMap::new(),
                        error: Some(e.to_string()),
                        output: None,
                        inputs: test_inputs,
                        failure_kind: Some(FailureKind::Unspecified),
                    },
                }
            });

            handles.push(handle);
        }

        // Collect results
        for handle in handles {
            let result = handle.await.context("Test task failed")?;
            results.push(result);
        }

        // Compute scores
        let scores = compute_scores(&results);

        Ok((scores, results))
    }

    /// Collect failure examples for reflection
    pub fn collect_failures(
        &self,
        runtime: &crate::BamlRuntime,
        function_name: &str,
        results: &[TestEvalResult],
        max_failures: usize,
    ) -> Vec<ReflectiveExample> {
        results
            .iter()
            .filter(|r| !r.passed)
            .take(max_failures)
            .map(|r| {
                // Extract the test source code
                let test_source = super::schema_extractor::extract_test_source(
                    runtime,
                    function_name,
                    &r.test_name,
                );

                ReflectiveExample {
                    inputs: r.inputs.clone(),
                    generated_outputs: r
                        .output
                        .as_ref()
                        .map(|o| {
                            let mut map = HashMap::new();
                            map.insert("output".to_string(), o.clone());
                            map
                        })
                        .unwrap_or_default(),
                    feedback: r.error.clone().unwrap_or_else(|| "Test failed".to_string()),
                    failure_location: failure_location(r),
                    test_source,
                    test_name: Some(r.test_name.clone()),
                    prompt_tokens: r.prompt_tokens,
                    completion_tokens: r.completion_tokens,
                    latency_ms: Some(r.latency_ms),
                }
            })
            .collect()
    }

    /// Collect successful examples for reference
    pub fn collect_successes(
        &self,
        results: &[TestEvalResult],
        max_successes: usize,
    ) -> Vec<ReflectiveExample> {
        results
            .iter()
            .filter(|r| r.passed)
            .take(max_successes)
            .map(|r| ReflectiveExample {
                inputs: r.inputs.clone(),
                generated_outputs: r
                    .output
                    .as_ref()
                    .map(|o| {
                        let mut map = HashMap::new();
                        map.insert("output".to_string(), o.clone());
                        map
                    })
                    .unwrap_or_default(),
                feedback: "Test passed".to_string(),
                failure_location: None,
                test_source: None,
                test_name: Some(r.test_name.clone()),
                prompt_tokens: r.prompt_tokens,
                completion_tokens: r.completion_tokens,
                latency_ms: Some(r.latency_ms),
            })
            .collect()
    }
}

/// Compute aggregate scores from test results
fn compute_scores(results: &[TestEvalResult]) -> CandidateScores {
    let tests_passed = results.iter().filter(|r| r.passed).count();
    let tests_total = results.len();

    let prompt_tokens: Vec<f64> = results.iter().filter_map(|r| r.prompt_tokens).collect();

    let completion_tokens: Vec<f64> = results.iter().filter_map(|r| r.completion_tokens).collect();

    let latencies: Vec<f64> = results.iter().map(|r| r.latency_ms).collect();

    // Aggregate check results
    let mut check_aggregates: HashMap<String, Vec<bool>> = HashMap::new();
    for result in results {
        for (name, passed) in &result.check_results {
            check_aggregates
                .entry(name.clone())
                .or_default()
                .push(*passed);
        }
    }

    CandidateScores::from_test_results(
        tests_passed,
        tests_total,
        prompt_tokens,
        completion_tokens,
        latencies,
        check_aggregates,
    )
}

/// Determine the likely failure location from a test result using structured error information
fn failure_location(result: &TestEvalResult) -> Option<String> {
    use crate::internal::llm_client::ErrorCode;

    // Use structured failure kind if available
    if let Some(ref kind) = result.failure_kind {
        return Some(match kind {
            FailureKind::ParseFailure => "parsing".to_string(),
            FailureKind::ConstraintsFailure { .. } => "assertion".to_string(),
            FailureKind::FinishReasonFailed => "finish_reason".to_string(),
            FailureKind::LLMFailure(info) => {
                // Use the structured ErrorCode to determine failure location
                match &info.error_code {
                    ErrorCode::Timeout => "infrastructure".to_string(),
                    ErrorCode::RateLimited => "infrastructure".to_string(),
                    ErrorCode::ServiceUnavailable => "infrastructure".to_string(),
                    ErrorCode::ServerError => "infrastructure".to_string(),
                    ErrorCode::InvalidAuthentication => "configuration".to_string(),
                    ErrorCode::NotSupported => "configuration".to_string(),
                    ErrorCode::UnsupportedResponse(_) => "llm_response".to_string(),
                    ErrorCode::Other(code) => {
                        // For Other error codes, use the code to classify
                        match code {
                            0 => "user_error".to_string(),     // UserFailure
                            1 => "internal_error".to_string(), // InternalFailure
                            2 => "cancelled".to_string(),      // Cancelled
                            _ => {
                                // Fallback: if no output was generated, likely a prompt issue
                                if result.output.is_none() {
                                    "prompt".to_string()
                                } else {
                                    "llm_call".to_string()
                                }
                            }
                        }
                    }
                }
            }
            FailureKind::Unspecified => "unknown".to_string(),
        });
    }

    // Fallback to string-based detection if no structured reason is available
    let error = result.error.as_ref()?;
    let error_lower = error.to_lowercase();

    if error_lower.contains("parse")
        || error_lower.contains("json")
        || error_lower.contains("deserialize")
    {
        Some("parsing".to_string())
    } else if error_lower.contains("assert") || error_lower.contains("constraint") {
        Some("assertion".to_string())
    } else if error_lower.contains("timeout") || error_lower.contains("rate limit") {
        Some("infrastructure".to_string())
    } else if result.output.is_none() {
        Some("prompt".to_string())
    } else {
        Some("unknown".to_string())
    }
}

/// Extract the actual output from a TestResponse
fn response_output(response: &crate::TestResponse) -> Option<String> {
    // Try to get output from expr function response first
    if let Some(Ok(val)) = &response.expr_function_response {
        return Some(serde_json::to_string_pretty(&val.serialize_partial()).unwrap_or_default());
    }

    // Try to get output from LLM function response
    if let Some(func_res) = &response.function_response {
        if let Some(Ok(value)) = func_res.result_with_constraints() {
            return Some(
                serde_json::to_string_pretty(&value.serialize_partial()).unwrap_or_default(),
            );
        }
        // Even if parsing failed, try to get the raw LLM output
        if let Ok(content) = func_res.llm_response().content() {
            return Some(content.to_string());
        }
    }

    None
}

/// Extract detailed error information from TestStatus
fn error_details(
    status: &TestStatus<'_>,
) -> (Option<String>, HashMap<String, bool>, Option<FailureKind>) {
    match status {
        TestStatus::Pass => (None, HashMap::new(), None),
        TestStatus::NeedsHumanEval(checks) => {
            let check_map: HashMap<String, bool> =
                checks.iter().map(|c| (c.clone(), false)).collect();
            (
                Some(format!(
                    "Checks need human evaluation: {}",
                    checks.join(", ")
                )),
                check_map,
                None,
            )
        }
        TestStatus::Fail(reason) => {
            let failure_kind = FailureKind::from(reason);
            let (error_msg, check_results) = match reason {
                TestFailReason::TestUnspecified(e) => (e.to_string(), HashMap::new()),
                TestFailReason::TestLLMFailure(llm_resp) => {
                    let msg = format!("LLM call failed: {}", llm_resp);
                    (msg, HashMap::new())
                }
                TestFailReason::TestParseFailure(e) => {
                    (format!("Failed to parse LLM output: {}", e), HashMap::new())
                }
                TestFailReason::TestFinishReasonFailed(e) => {
                    (format!("LLM finish reason error: {}", e), HashMap::new())
                }
                TestFailReason::TestConstraintsFailure {
                    checks,
                    failed_assert,
                } => {
                    let check_map: HashMap<String, bool> = checks
                        .iter()
                        .map(|(name, passed)| (name.clone(), *passed))
                        .collect();

                    let mut msg_parts = Vec::new();

                    // Report the specific assertion that failed
                    if let Some(assert_name) = failed_assert {
                        msg_parts.push(format!("Assertion '{}' failed", assert_name));
                    }

                    // Report any failed checks
                    let failed_checks: Vec<_> = checks
                        .iter()
                        .filter(|(_, passed)| !passed)
                        .map(|(name, _)| name.as_str())
                        .collect();
                    if !failed_checks.is_empty() {
                        msg_parts.push(format!("Failed checks: {}", failed_checks.join(", ")));
                    }

                    let msg = if msg_parts.is_empty() {
                        "Constraint failure".to_string()
                    } else {
                        msg_parts.join("; ")
                    };

                    (msg, check_map)
                }
            };
            (Some(error_msg), check_results, Some(failure_kind))
        }
    }
}
