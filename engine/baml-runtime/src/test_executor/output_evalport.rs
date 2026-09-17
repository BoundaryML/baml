use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result};
use baml_types::{BamlValue, Constraint, ConstraintLevel};
use chrono::{DateTime, SecondsFormat, Utc};
use jsonish::SerializeMode;
use serde_json::{json, Value};

use super::{
    file_reader_pinned, RenderTestExecutionStatus, TestExecutionStatus, TestExecutionStatusMap,
};
use crate::{
    test_constraints::TestConstraintsResult, BamlRuntime, TestFailReason, TestResponse, TestStatus,
};

const EVALPORT_SCHEMA_VERSION: &str = "1.0.0";
const SUITE_ID: &str = "baml-test-suite";
const OVERALL_GRADER_ID: &str = "baml-test-result";

#[derive(Clone)]
struct TestDefinition {
    function_name: String,
    test_name: String,
    location: String,
    args: Value,
    constraints: Vec<Constraint>,
}

pub(super) struct EvalPortRenderer {
    target_dir: PathBuf,
    definitions: BTreeMap<(String, String), TestDefinition>,
    started_at: DateTime<Utc>,
    run_id: String,
}

impl EvalPortRenderer {
    pub(super) fn new(
        target_dir: &Path,
        runtime: &BamlRuntime,
        selected_tests: &BTreeMap<(String, String), String>,
        env_vars: &HashMap<String, String>,
    ) -> Result<Self> {
        let ctx_manager = runtime.create_ctx_manager(
            BamlValue::String("cli".to_string()),
            Some(Box::new(file_reader_pinned)),
        );
        let ctx = ctx_manager.create_ctx(None, None, env_vars.clone(), Vec::new())?;
        let definitions = selected_tests
            .iter()
            .map(|((function_name, test_name), location)| {
                let (args, constraints) = runtime
                    .get_test_params_and_constraints(function_name, test_name, &ctx, true)
                    .with_context(|| {
                        format!(
                            "Failed to export BAML test {function_name}::{test_name} to EvalPort"
                        )
                    })?;
                let args = serde_json::to_value(args).with_context(|| {
                    format!(
                        "Failed to serialize arguments for BAML test {function_name}::{test_name}"
                    )
                })?;
                Ok((
                    (function_name.clone(), test_name.clone()),
                    TestDefinition {
                        function_name: function_name.clone(),
                        test_name: test_name.clone(),
                        location: location.clone(),
                        args,
                        constraints,
                    },
                ))
            })
            .collect::<Result<_>>()?;

        Ok(Self {
            target_dir: target_dir.to_path_buf(),
            definitions,
            started_at: Utc::now(),
            run_id: format!("baml-{}", uuid::Uuid::new_v4()),
        })
    }

    pub(super) fn write_suite(&self) -> Result<()> {
        fs::create_dir_all(&self.target_dir).with_context(|| {
            format!(
                "Failed to create EvalPort output directory {}",
                self.target_dir.display()
            )
        })?;

        let mut graders = vec![json!({
            "id": OVERALL_GRADER_ID,
            "type": "custom",
            "params": { "handler": "baml.test" },
            "description": "Overall result reported by the BAML test runner"
        })];
        let mut test_cases = Vec::with_capacity(self.definitions.len());

        for definition in self.definitions.values() {
            let test_case_id = test_case_id(definition);
            let mut grader_ids = vec![OVERALL_GRADER_ID.to_string()];
            for (index, constraint) in definition.constraints.iter().enumerate() {
                let grader_id = constraint_grader_id(&test_case_id, index);
                grader_ids.push(grader_id.clone());
                graders.push(json!({
                    "id": grader_id,
                    "type": "custom",
                    "params": {
                        "handler": "baml.constraint",
                        "level": constraint_level(constraint),
                        "expression": constraint.expression.0,
                        "label": constraint.label,
                    },
                    "description": constraint_description(constraint, index),
                }));
            }

            let input = serde_json::to_string(&json!({
                "function": definition.function_name,
                "args": definition.args,
            }))?;
            test_cases.push(json!({
                "id": test_case_id,
                "input": input,
                "graders": grader_ids,
                "metadata": {
                    "baml.function": definition.function_name,
                    "baml.test": definition.test_name,
                    "baml.args": definition.args,
                    "baml.source": definition.location,
                },
                "tags": ["baml"],
            }));
        }

        let suite = json!({
            "$schema": "https://evalport.org/schema/suite.json",
            "version": EVALPORT_SCHEMA_VERSION,
            "id": SUITE_ID,
            "name": "BAML test suite",
            "description": "Selected tests exported by baml test",
            "graders": graders,
            "test_cases": test_cases,
            "metadata": {
                "baml.runtime_version": env!("CARGO_PKG_VERSION"),
                "baml.interop": "evalport",
            },
        });

        self.write_json("suite.json", &suite)
    }

    fn write_results(&self, test_status_map: &TestExecutionStatusMap) -> Result<()> {
        let completed_at = Utc::now();
        let mut passed_count = 0_u64;
        let mut failed_count = 0_u64;
        let mut skipped_count = 0_u64;
        let mut duration_ms = 0_u64;
        let mut results = Vec::with_capacity(test_status_map.len());

        for (key, status) in test_status_map {
            let definition = self.definitions.get(key).with_context(|| {
                format!("Missing EvalPort test definition for {}::{}", key.0, key.1)
            })?;
            let (passed, skipped) = status_outcome(status);
            if skipped {
                skipped_count += 1;
            } else if passed {
                passed_count += 1;
            } else {
                failed_count += 1;
            }
            if let TestExecutionStatus::Finished(_, duration) = status {
                duration_ms = duration_ms.saturating_add(duration_to_millis(*duration));
            }
            results.push(self.render_result(definition, status, completed_at)?);
        }

        let total = results.len() as u64;
        let pass_rate = if total == 0 {
            0.0
        } else {
            passed_count as f64 / total as f64
        };
        let result_set = json!({
            "$schema": "https://evalport.org/schema/resultset.json",
            "version": EVALPORT_SCHEMA_VERSION,
            "suite_id": SUITE_ID,
            "run_id": self.run_id,
            "started_at": timestamp(self.started_at),
            "completed_at": timestamp(completed_at),
            "runner": {
                "name": "baml-cli",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "results": results,
            "summary": {
                "total": total,
                "passed": passed_count,
                "failed": failed_count,
                "skipped": skipped_count,
                "pass_rate": pass_rate,
                "avg_score": pass_rate,
                "duration_ms": duration_ms,
            },
            "metadata": {
                "baml.interop": "evalport",
            },
        });

        self.write_json("results.json", &result_set)
    }

    fn render_result(
        &self,
        definition: &TestDefinition,
        status: &TestExecutionStatus,
        completed_at: DateTime<Utc>,
    ) -> Result<Value> {
        let (passed, skipped) = status_outcome(status);
        let mut grader_results = vec![overall_grader_result(status)];
        if let TestExecutionStatus::Finished(Ok(response), _) = status {
            grader_results.extend(constraint_grader_results(definition, response));
        } else {
            grader_results.extend(definition.constraints.iter().enumerate().map(|(index, _)| {
                json!({
                    "grader_id": constraint_grader_id(&test_case_id(definition), index),
                    "type": "custom",
                    "score": null,
                    "passed": false,
                    "reason": "Constraint was not evaluated",
                    "metadata": { "skip_reason": "test_not_completed" },
                })
            }));
        }

        let mut result = json!({
            "test_case_id": test_case_id(definition),
            "completed_at": timestamp(completed_at),
            "grader_results": grader_results,
            "passed": passed,
            "metadata": {
                "baml.function": definition.function_name,
                "baml.test": definition.test_name,
                "baml.skipped": skipped,
            },
        });
        if let Some(actual_output) = actual_output(status)? {
            result["actual_output"] = Value::String(actual_output);
        }
        if let TestExecutionStatus::Finished(_, duration) = status {
            result["duration_ms"] = json!(duration_to_millis(*duration));
        }
        if let TestExecutionStatus::Finished(Err(error), _) = status {
            result["error"] = json!({
                "type": "runner_error",
                "message": error.to_string(),
                "retryable": false,
            });
        }
        Ok(result)
    }

    fn write_json(&self, file_name: &str, value: &Value) -> Result<()> {
        let path = self.target_dir.join(file_name);
        let json = serde_json::to_vec_pretty(value)?;
        fs::write(&path, json)
            .with_context(|| format!("Failed to write EvalPort document {}", path.display()))
    }
}

impl RenderTestExecutionStatus for EvalPortRenderer {
    fn render_progress(&self, _test_status_map: &TestExecutionStatusMap) {}

    fn render_final(
        &self,
        test_status_map: &TestExecutionStatusMap,
        _selected_tests: &BTreeMap<(String, String), String>,
    ) -> Result<()> {
        self.write_results(test_status_map)
    }

    fn print_message(&self, _msg: &str) {}
}

fn test_case_id(definition: &TestDefinition) -> String {
    format!("{}::{}", definition.function_name, definition.test_name)
}

fn constraint_grader_id(test_case_id: &str, index: usize) -> String {
    format!("{test_case_id}::constraint::{}", index + 1)
}

fn constraint_level(constraint: &Constraint) -> &'static str {
    match constraint.level {
        ConstraintLevel::Assert => "assert",
        ConstraintLevel::Check => "check",
    }
}

fn constraint_description(constraint: &Constraint, index: usize) -> String {
    let label = constraint
        .label
        .as_deref()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("constraint_{}", index + 1));
    format!("BAML {} {label}", constraint_level(constraint))
}

fn timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn duration_to_millis(duration: Duration) -> u64 {
    duration.as_millis().min(u64::MAX as u128) as u64
}

fn status_outcome(status: &TestExecutionStatus) -> (bool, bool) {
    match status {
        TestExecutionStatus::Finished(Ok(response), _) => {
            (matches!(response.status(), TestStatus::Pass), false)
        }
        TestExecutionStatus::Finished(Err(_), _) => (false, false),
        TestExecutionStatus::Pending
        | TestExecutionStatus::Running
        | TestExecutionStatus::Excluded => (false, true),
    }
}

fn overall_grader_result(status: &TestExecutionStatus) -> Value {
    let (passed, skipped) = status_outcome(status);
    let score = if skipped {
        Value::Null
    } else {
        json!(u8::from(passed))
    };
    let reason = match status {
        TestExecutionStatus::Finished(Ok(response), _) => match response.status() {
            TestStatus::Pass => "BAML test passed".to_string(),
            TestStatus::Fail(reason) => reason.to_string(),
            TestStatus::NeedsHumanEval(reasons) => reasons.join(", "),
        },
        TestExecutionStatus::Finished(Err(error), _) => error.to_string(),
        TestExecutionStatus::Pending => "Test was cancelled while pending".to_string(),
        TestExecutionStatus::Running => "Test was cancelled while running".to_string(),
        TestExecutionStatus::Excluded => "Test was excluded".to_string(),
    };
    json!({
        "grader_id": OVERALL_GRADER_ID,
        "type": "custom",
        "score": score,
        "passed": passed,
        "reason": reason,
        "metadata": if skipped { json!({ "skip_reason": "not_completed" }) } else { json!({}) },
    })
}

fn constraint_grader_results(definition: &TestDefinition, response: &TestResponse) -> Vec<Value> {
    let constraints_were_evaluated = matches!(
        response.status(),
        TestStatus::Pass
            | TestStatus::Fail(TestFailReason::TestConstraintsFailure { .. })
            | TestStatus::NeedsHumanEval(_)
    );
    if !constraints_were_evaluated {
        return skipped_constraint_results(definition, "BAML test failed before constraints ran");
    }

    let TestConstraintsResult::Completed {
        checks,
        failed_assert,
    } = &response.constraints_result
    else {
        return skipped_constraint_results(definition, "Constraint evaluation failed internally");
    };

    let mut check_results = checks.iter().cloned().collect::<VecDeque<(String, bool)>>();
    let failed_assert_index = failed_assert.as_ref().and_then(|failed_label| {
        definition.constraints.iter().position(|constraint| {
            constraint.level == ConstraintLevel::Assert
                && constraint.label.as_deref().unwrap_or("") == failed_label
        })
    });

    definition
        .constraints
        .iter()
        .enumerate()
        .map(|(index, constraint)| {
            let grader_id = constraint_grader_id(&test_case_id(definition), index);
            match constraint.level {
                ConstraintLevel::Check => {
                    let expected_label = constraint.label.as_deref().unwrap_or("");
                    let result = check_results
                        .front()
                        .filter(|(label, _)| label == expected_label)
                        .cloned();
                    if let Some((_, passed)) = result {
                        check_results.pop_front();
                        json!({
                            "grader_id": grader_id,
                            "type": "custom",
                            "score": u8::from(passed),
                            "passed": passed,
                            "reason": format!("BAML check {}", if passed { "passed" } else { "failed" }),
                        })
                    } else {
                        skipped_constraint_result(grader_id, "Check was not reached")
                    }
                }
                ConstraintLevel::Assert => match failed_assert_index {
                    None => json!({
                        "grader_id": grader_id,
                        "type": "custom",
                        "score": 1,
                        "passed": true,
                        "reason": "BAML assertion passed",
                    }),
                    Some(failed_index) if index < failed_index => json!({
                        "grader_id": grader_id,
                        "type": "custom",
                        "score": 1,
                        "passed": true,
                        "reason": "BAML assertion passed",
                    }),
                    Some(failed_index) if index == failed_index => json!({
                        "grader_id": grader_id,
                        "type": "custom",
                        "score": 0,
                        "passed": false,
                        "reason": "BAML assertion failed",
                    }),
                    Some(_) => skipped_constraint_result(grader_id, "Assertion was not reached"),
                },
            }
        })
        .collect()
}

fn skipped_constraint_results(definition: &TestDefinition, reason: &str) -> Vec<Value> {
    definition
        .constraints
        .iter()
        .enumerate()
        .map(|(index, _)| {
            skipped_constraint_result(
                constraint_grader_id(&test_case_id(definition), index),
                reason,
            )
        })
        .collect()
}

fn skipped_constraint_result(grader_id: String, reason: &str) -> Value {
    json!({
        "grader_id": grader_id,
        "type": "custom",
        "score": null,
        "passed": false,
        "reason": reason,
        "metadata": { "skip_reason": "not_evaluated" },
    })
}

fn actual_output(status: &TestExecutionStatus) -> Result<Option<String>> {
    let TestExecutionStatus::Finished(Ok(response), _) = status else {
        return Ok(None);
    };
    if let Some(function_response) = &response.function_response {
        return Ok(function_response.content().ok().map(ToOwned::to_owned));
    }
    let Some(Ok(value)) = &response.expr_function_response else {
        return Ok(None);
    };
    let value = serde_json::to_value(value.serialize_partial())?;
    Ok(Some(match value {
        Value::String(value) => value,
        value => serde_json::to_string(&value)?,
    }))
}
