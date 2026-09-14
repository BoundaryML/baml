//! Per-execution fold cache (TASK/baml-query-scope.md §5.4).
//!
//! `ExecutionReader::load()` runs once per execution per session behind
//! a bounded LRU; `threads`, `contexts`, `calls`, `errors`, and `health`
//! for the same execution share the fold. Function tables are cached by
//! CAS cid (one table serves every execution of the same engine).

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use baml_query::error::QueryError;
use bex_prof_store::prof::backend::{ExecutionProfile, FunctionTable};

use crate::universe::{BoundStream, read_error};

/// Approximate folded bytes per span/context/thread entry, for the LRU
/// budget (entries carry small fixed structs; strings are rare).
const APPROX_ENTRY_BYTES: u64 = 256;

pub(crate) struct FoldCache {
    max_bytes: u64,
    state: Mutex<FoldState>,
}

#[derive(Default)]
struct FoldState {
    folds: HashMap<[u8; 32], Arc<ExecutionProfile>>,
    order: Vec<[u8; 32]>,
    bytes: u64,
    tables: HashMap<[u8; 32], Arc<Option<FunctionTable>>>,
}

fn fold_key(summary: &bex_prof_store::prof::backend::ExecutionSummary) -> [u8; 32] {
    // Cache identity per §5.4: (execution, data range, ended?). Streams
    // never reuse a root ThreadRef, and the bound range is frozen, so the
    // encoded id + range is enough within one bound universe.
    use sha2::Digest as _;
    let mut hash = sha2::Sha256::new();
    hash.update(summary.id.encode().as_bytes());
    hash.update(summary.data_first_seq.to_be_bytes());
    hash.update(summary.data_last_seq.to_be_bytes());
    hash.update([u8::from(summary.ended_ns.is_some())]);
    hash.finalize().into()
}

impl FoldCache {
    #[must_use]
    pub(crate) fn new(max_bytes: u64) -> Arc<FoldCache> {
        Arc::new(FoldCache {
            max_bytes: max_bytes.max(1),
            state: Mutex::new(FoldState::default()),
        })
    }

    /// The folded execution, loading it on first use.
    pub(crate) fn fold(
        &self,
        stream: &BoundStream,
        summary: &bex_prof_store::prof::backend::ExecutionSummary,
    ) -> Result<Arc<ExecutionProfile>, QueryError> {
        let key = fold_key(summary);
        {
            let state = self.lock();
            if let Some(hit) = state.folds.get(&key) {
                return Ok(hit.clone());
            }
        }
        let reader = stream.execution_reader(summary)?;
        let profile = Arc::new(reader.load().map_err(|e| read_error(&e))?);
        let bytes = approx_bytes(&profile);
        let mut state = self.lock();
        while state.bytes.saturating_add(bytes) > self.max_bytes && !state.order.is_empty() {
            let evict = state.order.remove(0);
            if let Some(gone) = state.folds.remove(&evict) {
                state.bytes = state.bytes.saturating_sub(approx_bytes(&gone));
            }
        }
        if state.folds.insert(key, profile.clone()).is_none() {
            state.order.push(key);
            state.bytes = state.bytes.saturating_add(bytes);
        }
        Ok(profile)
    }

    /// The engine's durable function table, cached by CAS cid.
    pub(crate) fn function_table(
        &self,
        stream: &BoundStream,
        summary: &bex_prof_store::prof::backend::ExecutionSummary,
    ) -> Result<Arc<Option<FunctionTable>>, QueryError> {
        let Some(cid) = stream
            .reader
            .engines
            .iter()
            .find(|engine| engine.engine_id == summary.engine_id)
            .and_then(|engine| engine.function_table_cid)
        else {
            return Ok(Arc::new(None));
        };
        {
            let state = self.lock();
            if let Some(hit) = state.tables.get(&cid.0) {
                return Ok(hit.clone());
            }
        }
        let reader = stream.execution_reader(summary)?;
        let table = Arc::new(reader.function_table().map_err(|e| read_error(&e))?);
        self.lock().tables.insert(cid.0, table.clone());
        Ok(table)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, FoldState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

fn approx_bytes(profile: &ExecutionProfile) -> u64 {
    let entries = profile.contexts.len()
        + profile.spans.len()
        + profile.threads.len()
        + profile.errors.len()
        + profile.overflow.len();
    (entries as u64).saturating_mul(APPROX_ENTRY_BYTES).max(1)
}
