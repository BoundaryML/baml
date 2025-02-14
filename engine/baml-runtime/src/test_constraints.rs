use baml_types::{BamlValue, BamlValueWithMeta, Constraint, ConstraintLevel, ResponseCheck};
use internal_baml_core::ir::jinja_helpers::{evaluate_predicate, render_expression};
use jsonish::BamlValueWithFlags;

use anyhow::Result;
use indexmap::IndexMap;
use minijinja;
use std::{collections::HashMap, fmt};

use crate::internal::llm_client::LLMCompleteResponse;

/// Evaluate a list of constraints to be applied to a `BamlValueWithFlags`, in
/// the order that the constraints were specified by the user.
///
/// When a check in a test is evaluated, its results are added to the context
/// so that future constraints can refer to it.
pub fn evaluate_test_constraints(
    args: &IndexMap<String, BamlValue>,
    value: &BamlValueWithMeta<Vec<ResponseCheck>>,
    response: &LLMCompleteResponse,
    constraints: Vec<Constraint>,
) -> TestConstraintsResult {
    // Fold over all the constraints, updating both our success state, and
    // our jinja context full of Check results.
    // Finally, return the success state.
    constraints
        .into_iter()
        .fold(Accumulator::new(), |acc, constraint| {
            step_constraints(args, value, response, acc, constraint)
        })
        .result
}

/// The result of running a series of block-level constraints within a test.
#[derive(Clone, Debug, PartialEq)]
pub enum TestConstraintsResult {
    /// Constraint testing finished with the following check
    /// results, and optionally a failing assert.
    Completed {
        checks: Vec<(String, bool)>,
        failed_assert: Option<String>,
    },

    /// There was a problem evaluating a constraint.
    InternalError { details: String },
}

/// State update helper functions.
impl TestConstraintsResult {
    pub fn empty() -> Self {
        TestConstraintsResult::Completed {
            checks: Vec::new(),
            failed_assert: None,
        }
    }
    fn checks(self) -> Vec<(String, bool)> {
        match self {
            TestConstraintsResult::Completed { checks, .. } => checks,
            _ => Vec::new(),
        }
    }
    fn add_check_result(self, name: String, result: bool) -> Self {
        match self {
            TestConstraintsResult::Completed { mut checks, .. } => {
                checks.push((name, result));
                TestConstraintsResult::Completed {
                    checks,
                    failed_assert: None,
                }
            }
            _ => self,
        }
    }
    fn fail_assert(self, name: Option<String>) -> Self {
        match self {
            TestConstraintsResult::Completed { checks, .. } => TestConstraintsResult::Completed {
                checks,
                failed_assert: Some(name.unwrap_or("".to_string())),
            },
            _ => self,
        }
    }
}

/// The state that we track as we iterate over constraints in the test block.
struct Accumulator {
    pub result: TestConstraintsResult,
    pub check_results: Vec<(String, minijinja::Value)>,
}

impl Accumulator {
    pub fn new() -> Self {
        Accumulator {
            result: TestConstraintsResult::Completed {
                checks: Vec::new(),
                failed_assert: None,
            },
            check_results: Vec::new(),
        }
    }
}

/// The accumultator function, for running a single constraint
/// and updating the success state and the jinja context.
fn step_constraints(
    args: &IndexMap<String, BamlValue>,
    value: &BamlValueWithMeta<Vec<ResponseCheck>>,
    response: &LLMCompleteResponse,
    acc: Accumulator,
    constraint: Constraint,
) -> Accumulator {
    // Short-circuit if we have already had a hard failure. We can skip
    // the work in the rest of this function if we have already encountered
    // a hard failure.
    let already_failed = matches!(
        acc.result,
        TestConstraintsResult::Completed {
            failed_assert: Some(_),
            ..
        }
    ) || matches!(acc.result, TestConstraintsResult::InternalError { .. });
    if already_failed {
        return acc;
    }

    let mut check_results: Vec<(String, minijinja::Value)> = acc.check_results.clone();
    let check_results_for_jinja = check_results.iter().cloned().collect::<HashMap<_, _>>();
    let underscore = minijinja::Value::from_serialize(
        vec![
            ("result", minijinja::Value::from_serialize(value)),
            (
                "latency_ms",
                minijinja::Value::from_serialize(response.latency.as_millis()),
            ),
            (
                "checks",
                minijinja::Value::from_serialize(check_results_for_jinja),
            ),
        ]
        .into_iter()
        .collect::<HashMap<_, _>>(),
    );

    let ctx = vec![
        ("_".to_string(), underscore),
        ("this".to_string(), minijinja::Value::from_serialize(value)),
    ]
    .into_iter()
    .chain(
        args.iter()
            .map(|(name, value)| (name.to_string(), minijinja::Value::from_serialize(value))),
    )
    .collect();

    let constraint_result_str = render_expression(&constraint.expression, &ctx);
    let bool_result_or_internal_error: Result<bool, String> =
        match constraint_result_str.as_ref().map(|s| s.as_str()) {
            Ok("true") => Ok(true),
            Ok("false") => Ok(false),
            Ok("") => Ok(false),
            Ok(x) => Err(format!("Expected true or false, got {x}.")),
            Err(e) => Err(format!("Constraint error: {e:?}")),
        };

    // After running the constraint, we update the checks available in the
    // minijinja context.
    use ConstraintLevel::*;

    // The next value of the accumulator depends on several factors:
    //  - Whether we are processing a Check or an Assert.
    //  - Whether the constraint has a name or not.
    //  - The current accumulator state.
    //  In this match block, we use the result
    match (
        constraint.level,
        constraint.label,
        bool_result_or_internal_error,
    ) {
        // A check ran to completion and succeeded or failed
        // (i.e. returned a bool). This updates both the checks jinja context
        // and the status.
        (Check, Some(check_name), Ok(check_passed)) => {
            check_results.push((check_name.clone(), check_passed.into()));
            let mut new_checks = match acc.result {
                TestConstraintsResult::Completed { checks, .. } => checks,
                _ => Vec::new(),
            };
            new_checks.push((check_name, check_passed));
            let result = TestConstraintsResult::Completed {
                checks: new_checks,
                failed_assert: None,
            };
            Accumulator {
                result,
                check_results,
            }
        }

        // Internal error always produces a hard error.
        (_, _, Err(e)) => Accumulator {
            result: TestConstraintsResult::InternalError { details: e },
            check_results: acc.check_results,
        },

        // A check without a name has no effect, and should never be observed, because
        // the parser enforces that all checks are named.
        (Check, None, _) => {
            log::warn!(
                "Encountered a check without a name: {:?}",
                constraint.expression
            );
            acc
        }

        // A passing assert has no effect.
        (Assert, _, Ok(true)) => acc,

        // A failing assert is a hard error.
        (Assert, maybe_name, Ok(false)) => {
            let result = acc.result.fail_assert(maybe_name);
            Accumulator {
                result,
                check_results,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::llm_client::{LLMCompleteResponse, LLMCompleteResponseMetadata};
    use baml_types::{
        BamlValueWithMeta, Constraint, ConstraintLevel, JinjaExpression, ResponseCheck,
    };
    use internal_baml_jinja::RenderedPrompt;

    use std::collections::HashMap;

    /// Construct a value to use as a test fixture.
    /// It aims to combine a mix of:
    ///   - top-level vs. nested constraints
    ///   - asserts vs. checks
    ///   - successes vs. failures
    ///
    /// Roughly this schema:
    /// {
    ///   "name": {
    ///      value: "Greg",
    ///      meta: [
    ///        (@assert(good_name, {{ this|length > 0}}), true),
    ///        (@check(long_name, {{ this|length > 4}}), false),
    ///      ]}},
    ///   "kids": {
    ///     value: [
    ///       { name: {
    ///         value: "Tao",
    ///         meta: (same meta as top-level name)
    ///         },
    ///         age: 6
    ///       },
    ///       { name: {
    ///          value: "Ellie",
    ///          meta: (same meta as top-level name, but no failing check)
    ///          },
    ///          age: 3
    ///       }
    ///     ],
    ///     "meta": [
    ///       (@check(has_kids, {{ this|length > 0 }}), true)
    ///     ]
    ///   }
    /// }
    fn mk_value() -> BamlValueWithMeta<Vec<ResponseCheck>> {
        fn mk_name(name: &str) -> BamlValueWithMeta<Vec<ResponseCheck>> {
            let meta = vec![
                ResponseCheck {
                    name: "good_name".to_string(),
                    expression: "this|length > 0".to_string(),
                    status: "succeeded".to_string(),
                },
                ResponseCheck {
                    name: "long_name".to_string(),
                    expression: "this|length > 4".to_string(),
                    status: if name.len() > 4 {
                        "succeeded".to_string()
                    } else {
                        "failed".to_string()
                    },
                },
            ];
            BamlValueWithMeta::String(name.to_string(), meta)
        }

        fn mk_child(name: &str, age: i64) -> BamlValueWithMeta<Vec<ResponseCheck>> {
            BamlValueWithMeta::Class(
                "child".to_string(),
                vec![
                    ("name".to_string(), mk_name(name)),
                    ("age".to_string(), BamlValueWithMeta::Int(age, vec![])),
                ]
                .into_iter()
                .collect(),
                vec![],
            )
        }

        BamlValueWithMeta::Class(
            "parent".to_string(),
            vec![
                ("name".to_string(), mk_name("Greg")),
                (
                    "kids".to_string(),
                    BamlValueWithMeta::List(vec![mk_child("Tao", 6), mk_child("Ellie", 3)], vec![]),
                ),
            ]
            .into_iter()
            .collect(),
            vec![],
        )
    }

    fn mk_response() -> LLMCompleteResponse {
        LLMCompleteResponse {
            client: "test_client".to_string(),
            model: "test_model".to_string(),
            prompt: RenderedPrompt::Completion(String::new()),
            request_options: Default::default(),
            content: String::new(),
            start_time: web_time::SystemTime::UNIX_EPOCH,
            latency: web_time::Duration::from_millis(500),
            metadata: LLMCompleteResponseMetadata {
                baml_is_complete: true,
                finish_reason: None,
                prompt_tokens: None,
                output_tokens: None,
                total_tokens: None,
            },
        }
    }

    fn mk_check(label: &str, expr: &str) -> Constraint {
        Constraint {
            label: Some(label.to_string()),
            level: ConstraintLevel::Check,
            expression: JinjaExpression(expr.to_string()),
        }
    }

    fn mk_assert(label: &str, expr: &str) -> Constraint {
        Constraint {
            label: Some(label.to_string()),
            level: ConstraintLevel::Assert,
            expression: JinjaExpression(expr.to_string()),
        }
    }

    fn run_pipeline(constraints: &[Constraint]) -> TestConstraintsResult {
        let args = IndexMap::new();
        let value = mk_value();
        let constraints = constraints.into();
        let response = mk_response();
        evaluate_test_constraints(&args, &value, &response, constraints)
    }

    #[test]
    fn basic_test_constraints() {
        let res = run_pipeline(&[mk_assert("has_kids", "_.result.kids|length > 0")]);
        assert_eq!(
            res,
            TestConstraintsResult::Completed {
                checks: vec![],
                failed_assert: None,
            }
        );
    }

    #[test]
    fn test_dependencies() {
        let res = run_pipeline(&[
            mk_check("has_kids", "_.result.kids|length > 0"),
            mk_check("not_too_many", "this.kids.length < 100"),
            mk_assert("both_pass", "_.checks.has_kids and _.checks.not_too_many"),
        ]);
        assert_eq!(
            res,
            TestConstraintsResult::Completed {
                checks: vec![
                    ("has_kids".to_string(), true),
                    ("not_too_many".to_string(), true),
                ],
                failed_assert: None
            }
        );
    }

    #[test]
    fn test_dependencies_non_check() {
        let res = run_pipeline(&[
            mk_assert("has_kids", "_.result.kids|length > 0"),
            mk_check("not_too_many", "this.kids.length < 100"),
            mk_assert("both_pass", "_.checks.has_kids and _.checks.not_too_many"),
        ]);
        // This constraint set should fail because `has_kids` is an assert, not
        // a check, therefore it doesn't get a field in `checks`.
        assert_eq!(
            res,
            TestConstraintsResult::Completed {
                checks: vec![("not_too_many".to_string(), true),],
                failed_assert: Some("both_pass".to_string())
            }
        );
    }

    #[test]
    fn test_fast_is_sufficient() {
        let res = run_pipeline(&[
            mk_check("has_kids", "_.result.kids|length > 0"),
            mk_check("not_too_many", "this.kids.length < 100"),
            mk_check("both_pass", "_.checks.has_kids and _.checks.not_too_many"),
            mk_assert("either_or", "_.checks.both_pass or _.latency_ms < 1000"),
        ]);
        assert_eq!(
            res,
            TestConstraintsResult::Completed {
                checks: vec![
                    ("has_kids".to_string(), true),
                    ("not_too_many".to_string(), true),
                    ("both_pass".to_string(), true),
                ],
                failed_assert: None
            }
        );
    }

    #[test]
    fn test_failing_checks() {
        let res = run_pipeline(&[
            mk_check("has_kids", "_.result.kids|length > 0"),
            mk_check("not_too_many", "this.kids.length < 100"),
            mk_assert("both_pass", "_.checks.has_kids and _.checks.not_too_many"),
            mk_check("no_kids", "this.kids|length == 0"),
            mk_check("way_too_many", "this.kids|length > 1000"),
        ]);
        assert_eq!(
            res,
            TestConstraintsResult::Completed {
                checks: vec![
                    ("has_kids".to_string(), true),
                    ("not_too_many".to_string(), true),
                    ("no_kids".to_string(), false),
                    ("way_too_many".to_string(), false)
                ],
                failed_assert: None
            }
        );
    }

    #[test]
    fn test_internal_error() {
        let res = run_pipeline(&[mk_check("faulty", "__.result.kids|length > 0")]);
        // This test fails because there is a typo: `__` (double underscore).
        assert!(matches!(res, TestConstraintsResult::InternalError { .. }));
    }
}
