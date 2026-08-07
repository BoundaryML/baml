use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{QueryError, Result, ValueId};

#[derive(Clone, Debug)]
pub struct QueryBudgets {
    pub max_candidate_rows: usize,
    pub max_value_depth: usize,
    pub max_expanded_bytes: usize,
    pub max_blob_bytes: usize,
    pub max_distinct_values: usize,
    pub max_query_duration: Duration,
}

#[derive(Default)]
pub struct QueryMetrics {
    pub batches: AtomicUsize,
    pub input_rows: AtomicUsize,
    pub output_rows: AtomicUsize,
    pub distinct_root_ids: AtomicUsize,
    pub cache_hits: AtomicUsize,
    pub cache_misses: AtomicUsize,
    pub blob_requests: AtomicUsize,
    pub blob_bytes: AtomicUsize,
    query_duration_ns: AtomicU64,
    sqlite_duration_ns: AtomicU64,
    hydration_duration_ns: AtomicU64,
    blob_read_duration_ns: AtomicU64,
    serialization_duration_ns: AtomicU64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct QueryMetricsSnapshot {
    pub batches: usize,
    pub input_rows: usize,
    pub output_rows: usize,
    pub distinct_root_ids: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub blob_requests: usize,
    pub blob_bytes: usize,
    pub query_duration: Duration,
    pub sqlite_duration: Duration,
    pub hydration_duration: Duration,
    pub blob_read_duration: Duration,
    pub serialization_duration: Duration,
}

impl QueryMetrics {
    pub fn snapshot(&self) -> QueryMetricsSnapshot {
        QueryMetricsSnapshot {
            batches: self.batches.load(Ordering::Relaxed),
            input_rows: self.input_rows.load(Ordering::Relaxed),
            output_rows: self.output_rows.load(Ordering::Relaxed),
            distinct_root_ids: self.distinct_root_ids.load(Ordering::Relaxed),
            cache_hits: self.cache_hits.load(Ordering::Relaxed),
            cache_misses: self.cache_misses.load(Ordering::Relaxed),
            blob_requests: self.blob_requests.load(Ordering::Relaxed),
            blob_bytes: self.blob_bytes.load(Ordering::Relaxed),
            query_duration: Duration::from_nanos(self.query_duration_ns.load(Ordering::Relaxed)),
            sqlite_duration: Duration::from_nanos(self.sqlite_duration_ns.load(Ordering::Relaxed)),
            hydration_duration: Duration::from_nanos(
                self.hydration_duration_ns.load(Ordering::Relaxed),
            ),
            blob_read_duration: Duration::from_nanos(
                self.blob_read_duration_ns.load(Ordering::Relaxed),
            ),
            serialization_duration: Duration::from_nanos(
                self.serialization_duration_ns.load(Ordering::Relaxed),
            ),
        }
    }

    pub(crate) fn record_query_duration(&self, duration: Duration) {
        self.query_duration_ns
            .fetch_add(duration_nanos(duration), Ordering::Relaxed);
    }

    pub(crate) fn record_sqlite_duration(&self, duration: Duration) {
        self.sqlite_duration_ns
            .fetch_add(duration_nanos(duration), Ordering::Relaxed);
    }

    pub(crate) fn record_hydration_duration(&self, duration: Duration) {
        self.hydration_duration_ns
            .fetch_add(duration_nanos(duration), Ordering::Relaxed);
    }

    pub(crate) fn record_blob_read_duration(&self, duration: Duration) {
        self.blob_read_duration_ns
            .fetch_add(duration_nanos(duration), Ordering::Relaxed);
    }

    pub(crate) fn record_serialization_duration(&self, duration: Duration) {
        self.serialization_duration_ns
            .fetch_add(duration_nanos(duration), Ordering::Relaxed);
    }
}

fn duration_nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

impl Default for QueryBudgets {
    fn default() -> Self {
        Self {
            max_candidate_rows: 1_000_000,
            max_value_depth: 64,
            max_expanded_bytes: 64 * 1024 * 1024,
            max_blob_bytes: 256 * 1024 * 1024,
            max_distinct_values: 100_000,
            max_query_duration: Duration::from_secs(60),
        }
    }
}

#[derive(Clone)]
pub struct QueryContext {
    pub query_id: Uuid,
    pub project_id: Arc<str>,
    pub cancellation: CancellationToken,
    pub deadline: Instant,
    pub budgets: QueryBudgets,
    pub metrics: Arc<QueryMetrics>,
    hydrated_values: Arc<parking_lot::Mutex<std::collections::HashMap<ValueId, serde_json::Value>>>,
}

impl QueryContext {
    pub fn new(project_id: impl Into<Arc<str>>) -> Self {
        let budgets = QueryBudgets::default();
        Self {
            query_id: Uuid::new_v4(),
            project_id: project_id.into(),
            cancellation: CancellationToken::new(),
            deadline: Instant::now() + budgets.max_query_duration,
            budgets,
            metrics: Arc::new(QueryMetrics::default()),
            hydrated_values: Arc::new(parking_lot::Mutex::new(std::collections::HashMap::new())),
        }
    }

    pub fn metrics_snapshot(&self) -> QueryMetricsSnapshot {
        self.metrics.snapshot()
    }

    pub(crate) fn fresh(&self) -> Self {
        let mut context = Self::new(self.project_id.clone()).with_budgets(self.budgets.clone());
        context.cancellation = self.cancellation.clone();
        context
    }

    #[must_use]
    pub fn with_budgets(mut self, budgets: QueryBudgets) -> Self {
        self.deadline = Instant::now() + budgets.max_query_duration;
        self.budgets = budgets;
        self
    }

    pub fn check_cancelled(&self) -> Result<()> {
        if self.cancellation.is_cancelled() || Instant::now() >= self.deadline {
            return Err(QueryError::Cancelled {
                query_id: self.query_id,
            });
        }
        Ok(())
    }

    pub(crate) fn cached_value(&self, id: &ValueId) -> Option<serde_json::Value> {
        self.hydrated_values.lock().get(id).cloned()
    }

    pub(crate) fn cache_value(&self, id: ValueId, value: serde_json::Value) {
        self.hydrated_values.lock().insert(id, value);
    }
}
