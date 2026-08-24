//! The backend-neutral `ValueResolver` contract plus the query-scoped
//! hydration context (bounded, deduplicated, budget-counted, batched —
//! TASK/baml-query-scope.md §5.5).
//!
//! A provider's resident rows carry opaque handle bytes for each virtual
//! value column. The resolver — bound to the query's snapshot by trusted
//! code — interprets those handles. SQL never sees handle contents, and
//! handle bytes are never an equality surface.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use crate::{budget::BudgetTracker, outcome::UnavailableReason, value::model::Value};

/// The outcome of interpreting one handle.
#[derive(Debug, Clone)]
pub enum Resolved {
    Value(Arc<Value>),
    /// Typed unavailability: the row's evaluation cannot be decided; the
    /// outcome accounts for it.
    Unavailable(UnavailableReason),
}

/// Decode ceilings a resolver must honor per value read.
#[derive(Debug, Clone, Copy)]
pub struct DecodeCaps {
    pub max_bytes: u64,
    pub max_depth: u32,
}

/// Backend-neutral value resolution within one bound snapshot. The
/// batched entry point exists so a provider can amortize storage reads
/// across one Arrow batch (§5.5); the result vector matches `handles`
/// index-for-index.
pub trait ValueResolver: Send + Sync {
    /// Interpret a batch of provider-private handles. `results.len() ==
    /// handles.len()`, position-matched.
    fn resolve_many(&self, handles: &[&[u8]], caps: DecodeCaps) -> Vec<Resolved>;

    /// Resolve a canonical value by public CID (`baml_value_cid`
    /// comparisons). Backends without CID-addressable storage return
    /// `Unavailable(Unsupported)`.
    fn resolve_cid(&self, cid: &[u8; 32], caps: DecodeCaps) -> Resolved {
        let _ = (cid, caps);
        Resolved::Unavailable(UnavailableReason::Unsupported)
    }

    /// The canonical root CID for a handle, when the backend can prove it
    /// WITHOUT decoding (equality optimization: a deterministic canonical
    /// codec makes equal CIDs equivalent to encoded-body identity).
    /// `None` = unknown; the caller hydrates and compares semantically.
    fn canonical_cid(&self, handle: &[u8]) -> Option<[u8; 32]> {
        let _ = handle;
        None
    }
}

/// Query-scoped hydration: deduplicates handle resolutions in a bounded
/// cache and charges every miss against the query-global budget. One per
/// query; shared by every value UDF invocation.
pub struct HydrationContext {
    resolver: Arc<dyn ValueResolver>,
    tracker: Arc<BudgetTracker>,
    cache: Mutex<HandleCache>,
}

struct HandleCache {
    map: HashMap<Vec<u8>, Resolved>,
    order: VecDeque<Vec<u8>>,
    capacity: usize,
}

impl std::fmt::Debug for HydrationContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HydrationContext").finish_non_exhaustive()
    }
}

impl HydrationContext {
    /// `cache_entries` bounds the dedup cache (entries, not bytes — the
    /// decoded-byte budget bounds total memory pressure).
    #[must_use]
    pub fn new(
        resolver: Arc<dyn ValueResolver>,
        tracker: Arc<BudgetTracker>,
        cache_entries: usize,
    ) -> Arc<HydrationContext> {
        Arc::new(HydrationContext {
            resolver,
            tracker,
            cache: Mutex::new(HandleCache {
                map: HashMap::new(),
                order: VecDeque::new(),
                capacity: cache_entries.max(1),
            }),
        })
    }

    #[must_use]
    pub fn tracker(&self) -> &Arc<BudgetTracker> {
        &self.tracker
    }

    /// Resolve a batch of handles (one Arrow array's worth) with dedup +
    /// budget accounting: distinct uncached handles go to the resolver in
    /// ONE `resolve_many` call. Budget/cancel exhaustion surfaces as
    /// `Unavailable(QueryBudgetExhausted)` so the evaluation stays typed;
    /// the stream-level checkpoint turns the same condition into the
    /// terminal error. The result is position-matched; `None` in equals
    /// `None` out.
    pub fn resolve_batch(&self, handles: &[Option<&[u8]>]) -> Vec<Option<Resolved>> {
        // Answers are collected in a per-batch map and returned from it —
        // never read back through the LRU, whose eviction under a batch
        // with more distinct handles than the cache capacity would
        // otherwise discard fresh resolutions mid-answer. The cache is an
        // opportunistic cross-batch dedup, not the source of truth.
        let mut local: HashMap<&[u8], Resolved> = HashMap::new();
        let mut misses: Vec<&[u8]> = Vec::new();
        {
            let cache = self.lock_cache();
            for handle in handles.iter().flatten() {
                if local.contains_key(*handle) {
                    continue;
                }
                if let Some(hit) = cache.map.get(*handle) {
                    local.insert(*handle, hit.clone());
                } else {
                    local.insert(
                        *handle,
                        Resolved::Unavailable(UnavailableReason::QueryBudgetExhausted),
                    );
                    misses.push(*handle);
                }
            }
        }
        if !misses.is_empty() {
            let mut budgeted: Vec<&[u8]> = Vec::with_capacity(misses.len());
            for handle in misses {
                if self.tracker.checkpoint().is_ok() && self.tracker.count_hydration().is_ok() {
                    budgeted.push(handle);
                }
                // Exhausted handles keep the typed placeholder above.
            }
            let resolved = if budgeted.is_empty() {
                Vec::new()
            } else {
                self.resolver.resolve_many(&budgeted, self.caps())
            };
            let mut cache = self.lock_cache();
            for (handle, resolved) in budgeted.iter().zip(resolved) {
                if let Resolved::Value(value) = &resolved {
                    // Approximate decoded size; exact byte accounting comes
                    // from the resolver honoring caps.
                    let _ = self.tracker.count_decoded_bytes(approx_bytes(value));
                }
                local.insert(*handle, resolved.clone());
                cache_put(&mut cache, handle.to_vec(), resolved);
            }
        }
        handles
            .iter()
            .map(|handle| {
                handle.map(|handle| {
                    local
                        .get(handle)
                        .cloned()
                        .expect("every non-null handle was answered above")
                })
            })
            .collect()
    }

    /// Resolve one handle (comparison right-hand sides and unit tests);
    /// batch callers use [`HydrationContext::resolve_batch`].
    pub fn resolve(&self, handle: &[u8]) -> Resolved {
        self.resolve_batch(&[Some(handle)])
            .pop()
            .flatten()
            .expect("one handle in, one resolution out")
    }

    /// Resolve by public CID (comparison right-hand sides).
    pub fn resolve_cid(&self, cid: &[u8; 32]) -> Resolved {
        if self.tracker.checkpoint().is_err() || self.tracker.count_hydration().is_err() {
            return Resolved::Unavailable(UnavailableReason::QueryBudgetExhausted);
        }
        self.resolver.resolve_cid(cid, self.caps())
    }

    /// Equality shortcut: both sides canonically identified ⇒ equal CIDs
    /// are encoded-body identity. `None` = prove it by hydration.
    #[must_use]
    pub fn cid_shortcut(&self, handle: &[u8], cid: &[u8; 32]) -> Option<bool> {
        self.resolver
            .canonical_cid(handle)
            .map(|handle_cid| &handle_cid == cid)
    }

    fn caps(&self) -> DecodeCaps {
        let budgets = self.tracker.budgets();
        DecodeCaps {
            max_bytes: budgets.max_value_bytes,
            max_depth: budgets.max_decode_depth,
        }
    }

    fn lock_cache(&self) -> std::sync::MutexGuard<'_, HandleCache> {
        self.cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

fn cache_put(cache: &mut HandleCache, handle: Vec<u8>, resolved: Resolved) {
    if cache.map.len() >= cache.capacity
        && let Some(evict) = cache.order.pop_front()
    {
        cache.map.remove(&evict);
    }
    if cache.map.insert(handle.clone(), resolved).is_none() {
        cache.order.push_back(handle);
    }
}

/// Cheap decoded-size estimate for budget accounting.
fn approx_bytes(value: &Value) -> u64 {
    use crate::value::model::MediaContent;
    match value {
        Value::Null | Value::Bool(_) | Value::Int(_) | Value::Float(_) => 8,
        Value::BigInt(s) | Value::String(s) => s.len() as u64,
        Value::Bytes(b) => b.len() as u64,
        Value::List(items) => 8 + items.iter().map(approx_bytes).sum::<u64>(),
        Value::Map(entries) => {
            8 + entries
                .iter()
                .map(|(k, v)| k.len() as u64 + approx_bytes(v))
                .sum::<u64>()
        }
        Value::Class { name, fields } => {
            name.len() as u64
                + fields
                    .iter()
                    .map(|(k, _, v)| k.len() as u64 + v.as_ref().map_or(0, approx_bytes))
                    .sum::<u64>()
        }
        Value::Enum { name, variant } => (name.len() + variant.len()) as u64,
        Value::Media { content, .. } => match content {
            MediaContent::Bytes(b) => b.len() as u64,
            MediaContent::Url(u) => u.len() as u64,
        },
        Value::Omitted { reason } => reason.len() as u64,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::budget::{CancellationToken, QueryBudgets};

    struct CountingResolver {
        calls: AtomicU64,
        handles: AtomicU64,
    }

    impl ValueResolver for CountingResolver {
        fn resolve_many(&self, handles: &[&[u8]], _caps: DecodeCaps) -> Vec<Resolved> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.handles
                .fetch_add(handles.len() as u64, Ordering::Relaxed);
            handles
                .iter()
                .map(|handle| {
                    if *handle == b"gone" {
                        Resolved::Unavailable(UnavailableReason::Lost)
                    } else {
                        Resolved::Value(Arc::new(Value::Int(i64::try_from(handle.len()).unwrap())))
                    }
                })
                .collect()
        }
    }

    fn counting() -> Arc<CountingResolver> {
        Arc::new(CountingResolver {
            calls: AtomicU64::new(0),
            handles: AtomicU64::new(0),
        })
    }

    #[test]
    fn batch_resolution_deduplicates_and_calls_resolver_once() {
        let resolver = counting();
        let tracker = BudgetTracker::new(QueryBudgets::unlimited(), CancellationToken::new());
        let ctx = HydrationContext::new(resolver.clone(), tracker, 64);
        let batch: Vec<Option<&[u8]>> =
            vec![Some(b"h1"), None, Some(b"h2"), Some(b"h1"), Some(b"h1")];
        let out = ctx.resolve_batch(&batch);
        assert_eq!(out.len(), 5);
        assert!(out[1].is_none());
        assert!(matches!(out[0], Some(Resolved::Value(_))));
        assert!(matches!(out[3], Some(Resolved::Value(_))));
        assert_eq!(resolver.calls.load(Ordering::Relaxed), 1, "one batch call");
        assert_eq!(resolver.handles.load(Ordering::Relaxed), 2, "deduplicated");
        // A second batch of already-cached handles never reaches the
        // resolver.
        let _ = ctx.resolve_batch(&[Some(b"h1"), Some(b"h2")]);
        assert_eq!(resolver.calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn hydration_deduplicates_and_counts_budget() {
        let resolver = counting();
        let mut budgets = QueryBudgets::unlimited();
        budgets.max_hydrations = 2;
        let tracker = BudgetTracker::new(budgets, CancellationToken::new());
        let ctx = HydrationContext::new(resolver.clone(), tracker, 64);

        // Same handle three times: one resolver handle, one budget unit.
        for _ in 0..3 {
            assert!(matches!(ctx.resolve(b"h1"), Resolved::Value(_)));
        }
        assert_eq!(resolver.handles.load(Ordering::Relaxed), 1);
        // Second distinct handle: second (and last allowed) hydration.
        assert!(matches!(ctx.resolve(b"h2"), Resolved::Value(_)));
        // Third distinct handle: budget-exhausted as a TYPED evaluation.
        assert!(matches!(
            ctx.resolve(b"h3"),
            Resolved::Unavailable(UnavailableReason::QueryBudgetExhausted)
        ));
    }

    #[test]
    fn unavailability_is_typed_and_cached() {
        let resolver = counting();
        let tracker = BudgetTracker::new(QueryBudgets::unlimited(), CancellationToken::new());
        let ctx = HydrationContext::new(resolver.clone(), tracker, 64);
        for _ in 0..2 {
            assert!(matches!(
                ctx.resolve(b"gone"),
                Resolved::Unavailable(UnavailableReason::Lost)
            ));
        }
        assert_eq!(
            resolver.handles.load(Ordering::Relaxed),
            1,
            "misses cached too"
        );
    }
}
