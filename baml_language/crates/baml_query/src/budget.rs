//! Query-global budgets, counters, and cancellation
//! (TASK/baml-query-scope.md §5.7).
//!
//! Counters never reset per input batch: one [`BudgetTracker`] spans the
//! whole pipeline — resident scans, hydration, decode, residual work,
//! and output. Exact numeric defaults are X1 policy work; the limits
//! struct is policy-driven and `unlimited()` exists for hosts that defer
//! to their own policy layer.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use crate::{
    error::{QueryError, QueryErrorCode},
    outcome::{UnavailableReason, ValueEvaluations},
};

/// Policy-supplied limits. `None`/`u64::MAX` means unlimited.
#[derive(Debug, Clone)]
pub struct QueryBudgets {
    /// Wall-clock deadline for the whole query.
    pub max_wall: Option<Duration>,
    /// Result rows streamed to the caller.
    pub max_result_rows: u64,
    /// Candidate rows entering residual value evaluation.
    pub max_candidate_rows: u64,
    /// Distinct value handles hydrated.
    pub max_hydrations: u64,
    /// Canonical bytes decoded across all hydrations.
    pub max_decoded_bytes: u64,
    /// Decode depth per value (recursion bound for hostile DAGs).
    pub max_decode_depth: u32,
    /// Decoded bytes per single value read.
    pub max_value_bytes: u64,
    /// Folded per-execution state held by the provider's fold cache.
    pub max_fold_bytes: u64,
}

impl QueryBudgets {
    /// No limits (host policy layer owns them).
    #[must_use]
    pub fn unlimited() -> QueryBudgets {
        QueryBudgets {
            max_wall: None,
            max_result_rows: u64::MAX,
            max_candidate_rows: u64::MAX,
            max_hydrations: u64::MAX,
            max_decoded_bytes: u64::MAX,
            max_decode_depth: 128,
            max_value_bytes: 64 << 20,
            max_fold_bytes: 256 << 20,
        }
    }
}

/// Shared cancellation handle. Cloneable; `cancel()` is sticky.
#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    #[must_use]
    pub fn new() -> CancellationToken {
        CancellationToken::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

/// The query-global counter set. One per query, shared by every stage
/// (Arc); atomics because hydration UDFs run wherever `DataFusion` executes
/// them.
#[derive(Debug)]
pub struct BudgetTracker {
    budgets: QueryBudgets,
    started: Instant,
    cancel: CancellationToken,
    result_rows: AtomicU64,
    candidate_rows: AtomicU64,
    hydrations: AtomicU64,
    decoded_bytes: AtomicU64,
    evaluations_attempted: AtomicU64,
    evaluations_available: AtomicU64,
    // One counter per typed reason (index = UnavailableReason discriminant
    // order in REASONS).
    unavailable: [AtomicU64; REASONS.len()],
}

/// Order of the by-reason counters (stable, matches wire spellings).
const REASONS: [UnavailableReason; 10] = [
    UnavailableReason::Pending,
    UnavailableReason::NotCaptured,
    UnavailableReason::Omitted,
    UnavailableReason::Redacted,
    UnavailableReason::Lost,
    UnavailableReason::Truncated,
    UnavailableReason::Corrupt,
    UnavailableReason::Unsupported,
    UnavailableReason::StoreUnavailable,
    UnavailableReason::QueryBudgetExhausted,
];

impl BudgetTracker {
    #[must_use]
    pub fn new(budgets: QueryBudgets, cancel: CancellationToken) -> Arc<BudgetTracker> {
        Arc::new(BudgetTracker {
            budgets,
            started: Instant::now(),
            cancel,
            result_rows: AtomicU64::new(0),
            candidate_rows: AtomicU64::new(0),
            hydrations: AtomicU64::new(0),
            decoded_bytes: AtomicU64::new(0),
            evaluations_attempted: AtomicU64::new(0),
            evaluations_available: AtomicU64::new(0),
            unavailable: Default::default(),
        })
    }

    #[must_use]
    pub fn budgets(&self) -> &QueryBudgets {
        &self.budgets
    }

    /// Cancellation or wall-deadline check — call at every stage edge.
    pub fn checkpoint(&self) -> Result<(), QueryError> {
        if self.cancel.is_cancelled() {
            return Err(QueryError::new(
                QueryErrorCode::Cancelled,
                "query cancelled",
            ));
        }
        if let Some(max_wall) = self.budgets.max_wall
            && self.started.elapsed() > max_wall
        {
            return Err(QueryError::new(
                QueryErrorCode::BudgetExceeded,
                format!("wall-clock budget exceeded ({} ms)", max_wall.as_millis()),
            ));
        }
        Ok(())
    }

    /// Count rows delivered to the caller; errors when the result budget
    /// is exhausted.
    pub fn count_result_rows(&self, rows: u64) -> Result<(), QueryError> {
        let total = self.result_rows.fetch_add(rows, Ordering::Relaxed) + rows;
        if total > self.budgets.max_result_rows {
            return Err(QueryError::new(
                QueryErrorCode::BudgetExceeded,
                format!(
                    "result-row budget exceeded ({})",
                    self.budgets.max_result_rows
                ),
            ));
        }
        Ok(())
    }

    /// Count rows entering residual value evaluation.
    pub fn count_candidate_rows(&self, rows: u64) -> Result<(), QueryError> {
        let total = self.candidate_rows.fetch_add(rows, Ordering::Relaxed) + rows;
        if total > self.budgets.max_candidate_rows {
            return Err(QueryError::new(
                QueryErrorCode::BudgetExceeded,
                format!(
                    "candidate-row budget exceeded ({})",
                    self.budgets.max_candidate_rows
                ),
            ));
        }
        Ok(())
    }

    /// Count one distinct-handle hydration (cache misses only).
    pub fn count_hydration(&self) -> Result<(), QueryError> {
        let total = self.hydrations.fetch_add(1, Ordering::Relaxed) + 1;
        if total > self.budgets.max_hydrations {
            return Err(QueryError::new(
                QueryErrorCode::BudgetExceeded,
                format!(
                    "hydration budget exceeded ({})",
                    self.budgets.max_hydrations
                ),
            ));
        }
        Ok(())
    }

    /// Count decoded canonical bytes.
    pub fn count_decoded_bytes(&self, bytes: u64) -> Result<(), QueryError> {
        let total = self.decoded_bytes.fetch_add(bytes, Ordering::Relaxed) + bytes;
        if total > self.budgets.max_decoded_bytes {
            return Err(QueryError::new(
                QueryErrorCode::BudgetExceeded,
                format!(
                    "decoded-byte budget exceeded ({})",
                    self.budgets.max_decoded_bytes
                ),
            ));
        }
        Ok(())
    }

    /// Record one value evaluation that produced a usable value (or an
    /// ordinary null/absent-path — evidence WAS available).
    pub fn record_available(&self) {
        self.evaluations_attempted.fetch_add(1, Ordering::Relaxed);
        self.evaluations_available.fetch_add(1, Ordering::Relaxed);
    }

    /// Record one value evaluation that could not be decided (D12): the
    /// row leaves the data stream, the outcome turns incomplete.
    pub fn record_unavailable(&self, reason: UnavailableReason) {
        self.evaluations_attempted.fetch_add(1, Ordering::Relaxed);
        let idx = REASONS
            .iter()
            .position(|r| *r == reason)
            .expect("reason registered");
        self.unavailable[idx].fetch_add(1, Ordering::Relaxed);
    }

    /// Snapshot the evaluation accounting for the terminal outcome.
    #[must_use]
    pub fn value_evaluations(&self) -> ValueEvaluations {
        let mut by_reason = std::collections::BTreeMap::new();
        let mut total_unavailable = 0;
        for (idx, reason) in REASONS.iter().enumerate() {
            let n = self.unavailable[idx].load(Ordering::Relaxed);
            if n > 0 {
                by_reason.insert(reason.as_str().to_string(), n);
                total_unavailable += n;
            }
        }
        ValueEvaluations {
            attempted: self.evaluations_attempted.load(Ordering::Relaxed),
            available: self.evaluations_available.load(Ordering::Relaxed),
            unavailable: total_unavailable,
            by_reason,
        }
    }

    #[must_use]
    pub fn rows_streamed(&self) -> u64 {
        self.result_rows.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budgets_trip_and_accumulate_globally() {
        let mut budgets = QueryBudgets::unlimited();
        budgets.max_result_rows = 10;
        let tracker = BudgetTracker::new(budgets, CancellationToken::new());
        // Never per-batch: 4 + 4 pass, the next 4 trips the global budget.
        assert!(tracker.count_result_rows(4).is_ok());
        assert!(tracker.count_result_rows(4).is_ok());
        let err = tracker.count_result_rows(4).unwrap_err();
        assert_eq!(err.code, QueryErrorCode::BudgetExceeded);
        assert_eq!(err.code.as_str(), "E_QUERY_BUDGET_EXCEEDED");
    }

    #[test]
    fn cancellation_is_sticky_and_typed() {
        let cancel = CancellationToken::new();
        let tracker = BudgetTracker::new(QueryBudgets::unlimited(), cancel.clone());
        assert!(tracker.checkpoint().is_ok());
        cancel.cancel();
        assert_eq!(
            tracker.checkpoint().unwrap_err().code,
            QueryErrorCode::Cancelled
        );
    }

    #[test]
    fn evaluation_accounting_reconciles_by_reason() {
        let tracker = BudgetTracker::new(QueryBudgets::unlimited(), CancellationToken::new());
        tracker.record_available();
        tracker.record_available();
        tracker.record_unavailable(UnavailableReason::Redacted);
        tracker.record_unavailable(UnavailableReason::Redacted);
        tracker.record_unavailable(UnavailableReason::NotCaptured);
        let v = tracker.value_evaluations();
        assert_eq!(v.attempted, 5);
        assert_eq!(v.available, 2);
        assert_eq!(v.unavailable, 3);
        assert_eq!(v.by_reason.get("redacted"), Some(&2));
        assert_eq!(v.by_reason.get("not_captured"), Some(&1));
    }
}
