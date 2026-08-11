//! The mandatory terminal query outcome (D13).
//!
//! Every SQL stream ends with exactly one out-of-band terminal outcome —
//! success, evidence-incomplete success, planning/execution failure,
//! budget exhaustion, or cancellation. A stream that ends without its
//! outcome is never a successful complete result. The outcome is not a
//! SQL data row and not a second query language.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::error::QueryError;
use crate::scope::Snapshot;

/// How the query's result stands (frozen catalog-v1 spellings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultState {
    /// Every requested row was produced and every required value
    /// evaluation was available.
    Complete,
    /// Rows were produced, but at least one required value evaluation was
    /// unavailable (D12) — the rows are honest, the answer is partial.
    Incomplete,
    /// The query failed at planning or execution; any rows already
    /// streamed are explicitly incomplete.
    Failed,
    /// A query-global budget ended the stream early.
    BudgetExhausted,
    /// The caller cancelled the query.
    Cancelled,
}

/// Typed reasons a value evaluation could not be decided (D12). These are
/// the row-level unavailability states projected into outcome accounting;
/// a captured BAML null and an absent path are ordinary data, not listed
/// here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnavailableReason {
    Pending,
    NotCaptured,
    Omitted,
    Redacted,
    Lost,
    Truncated,
    Corrupt,
    Unsupported,
    /// The value store/dependency could not serve the read.
    StoreUnavailable,
    /// A query budget stopped this specific evaluation.
    QueryBudgetExhausted,
}

impl UnavailableReason {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            UnavailableReason::Pending => "pending",
            UnavailableReason::NotCaptured => "not_captured",
            UnavailableReason::Omitted => "omitted",
            UnavailableReason::Redacted => "redacted",
            UnavailableReason::Lost => "lost",
            UnavailableReason::Truncated => "truncated",
            UnavailableReason::Corrupt => "corrupt",
            UnavailableReason::Unsupported => "unsupported",
            UnavailableReason::StoreUnavailable => "store_unavailable",
            UnavailableReason::QueryBudgetExhausted => "query_budget_exhausted",
        }
    }
}

/// Value-evaluation accounting: every attempted hydration/evaluation is
/// reconciled here, so "no match" is never silently "could not evaluate".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueEvaluations {
    pub attempted: u64,
    pub available: u64,
    pub unavailable: u64,
    /// Unavailable evaluations grouped by typed reason (wire spellings).
    pub by_reason: BTreeMap<String, u64>,
}

/// The terminal outcome record (wire shape frozen with catalog v1; see
/// IN-Q1 notes). Human surfaces render it to stderr; structured streams
/// carry it as the final control frame.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryOutcome {
    /// True only when the stream reached its natural end (complete or
    /// incomplete); false for failure/budget/cancellation.
    pub query_completed: bool,
    pub result_state: ResultState,
    pub snapshot: Snapshot,
    pub value_evaluations: ValueEvaluations,
    /// Rows delivered to the caller before the stream ended.
    pub rows_streamed: u64,
    /// Present exactly when `result_state` is `failed`, `budget_exhausted`
    /// with an error, or `cancelled`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<OutcomeError>,
}

/// The error half of a terminal outcome (stable code + message).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutcomeError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remedy: Option<String>,
}

impl From<&QueryError> for OutcomeError {
    fn from(err: &QueryError) -> OutcomeError {
        OutcomeError {
            code: err.code.as_str().to_string(),
            message: err.message.clone(),
            retryable: err.code.retryable(),
            remedy: err.remedy.clone(),
        }
    }
}

impl QueryOutcome {
    /// A natural end of stream: complete when nothing was unavailable,
    /// incomplete otherwise (D12).
    #[must_use]
    pub fn completed(snapshot: Snapshot, values: ValueEvaluations, rows_streamed: u64) -> Self {
        let result_state = if values.unavailable == 0 {
            ResultState::Complete
        } else {
            ResultState::Incomplete
        };
        QueryOutcome {
            query_completed: true,
            result_state,
            snapshot,
            value_evaluations: values,
            rows_streamed,
            error: None,
        }
    }

    /// A terminal failure/budget/cancellation outcome.
    #[must_use]
    pub fn ended(
        snapshot: Snapshot,
        values: ValueEvaluations,
        rows_streamed: u64,
        error: &QueryError,
    ) -> Self {
        use crate::error::QueryErrorCode;
        let result_state = match err_state(error.code) {
            Some(state) => state,
            None => ResultState::Failed,
        };
        fn err_state(code: QueryErrorCode) -> Option<ResultState> {
            match code {
                QueryErrorCode::BudgetExceeded => Some(ResultState::BudgetExhausted),
                QueryErrorCode::Cancelled => Some(ResultState::Cancelled),
                _ => None,
            }
        }
        QueryOutcome {
            query_completed: false,
            result_state,
            snapshot,
            value_evaluations: values,
            rows_streamed,
            error: Some(OutcomeError::from(error)),
        }
    }
}
