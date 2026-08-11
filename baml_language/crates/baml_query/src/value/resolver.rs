//! The backend-neutral ValueResolver contract plus the query-scoped
//! hydration context (bounded, deduplicated, budget-counted).
//!
//! A provider's resident rows carry opaque handle bytes for each virtual
//! value column. The resolver — bound to the query's snapshot by trusted
//! code — interprets those handles. SQL never sees handle contents, and
//! handle bytes are never an equality surface.

use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use bex_events::store::canon::CanonValue;

use crate::budget::BudgetTracker;
use crate::outcome::UnavailableReason;

/// The outcome of interpreting one handle.
#[derive(Debug, Clone)]
pub enum Resolved {
    Value(Arc<CanonValue>),
    /// Typed unavailability (D12): the row's evaluation cannot be
    /// decided; the outcome accounts for it.
    Unavailable(UnavailableReason),
}

/// Decode ceilings a resolver must honor per value read.
#[derive(Debug, Clone, Copy)]
pub struct DecodeCaps {
    pub max_bytes: u64,
    pub max_depth: u32,
}

/// Backend-neutral value resolution within one bound snapshot.
pub trait ValueResolver: Send + Sync {
    /// Interpret one provider-private handle.
    fn resolve(&self, handle: &[u8], caps: DecodeCaps) -> Resolved;

    /// Resolve a canonical value by public CID (`baml_value_cid`
    /// comparisons). Backends without CID-addressable storage return
    /// `Unavailable(Unsupported)`.
    fn resolve_cid(&self, cid: &[u8; 32], caps: DecodeCaps) -> Resolved;

    /// The canonical root CID for a handle, when the backend can prove it
    /// WITHOUT decoding (D7 equality optimization: a deterministic
    /// canonical codec makes equal CIDs equivalent to semantic equality).
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

    /// Resolve one handle with dedup + budget accounting. Budget/cancel
    /// exhaustion surfaces as `Unavailable(QueryBudgetExhausted)` so the
    /// evaluation stays typed; the stream-level checkpoint turns the same
    /// condition into the terminal error.
    pub fn resolve(&self, handle: &[u8]) -> Resolved {
        if let Some(hit) = self.cache_get(handle) {
            return hit;
        }
        if self.tracker.checkpoint().is_err() || self.tracker.count_hydration().is_err() {
            return Resolved::Unavailable(UnavailableReason::QueryBudgetExhausted);
        }
        let caps = self.caps();
        let resolved = self.resolver.resolve(handle, caps);
        if let Resolved::Value(value) = &resolved {
            // Approximate decoded size; exact byte accounting comes from
            // the resolver honoring caps.
            let _ = self.tracker.count_decoded_bytes(approx_bytes(value));
        }
        self.cache_put(handle.to_vec(), resolved.clone());
        resolved
    }

    /// Resolve by public CID (comparison right-hand sides).
    pub fn resolve_cid(&self, cid: &[u8; 32]) -> Resolved {
        if self.tracker.checkpoint().is_err() || self.tracker.count_hydration().is_err() {
            return Resolved::Unavailable(UnavailableReason::QueryBudgetExhausted);
        }
        self.resolver.resolve_cid(cid, self.caps())
    }

    /// D7 equality shortcut: both sides canonically identified ⇒ equal
    /// CIDs are semantic equality. `None` = prove it by hydration.
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

    fn cache_get(&self, handle: &[u8]) -> Option<Resolved> {
        let cache = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache.map.get(handle).cloned()
    }

    fn cache_put(&self, handle: Vec<u8>, resolved: Resolved) {
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if cache.map.len() >= cache.capacity
            && let Some(evict) = cache.order.pop_front()
        {
            cache.map.remove(&evict);
        }
        if cache.map.insert(handle.clone(), resolved).is_none() {
            cache.order.push_back(handle);
        }
    }
}

/// Cheap decoded-size estimate for budget accounting.
fn approx_bytes(value: &CanonValue) -> u64 {
    match value {
        CanonValue::Null | CanonValue::Bool(_) | CanonValue::Int(_) | CanonValue::Float(_) => 8,
        CanonValue::Bigint(s) | CanonValue::String(s) => s.len() as u64,
        CanonValue::Bytes(b) => b.len() as u64,
        CanonValue::List(items) => 8 + items.iter().map(approx_bytes).sum::<u64>(),
        CanonValue::Map(entries) => {
            8 + entries
                .iter()
                .map(|(k, v)| k.len() as u64 + approx_bytes(v))
                .sum::<u64>()
        }
        CanonValue::Class { fields, .. } => {
            8 + fields
                .iter()
                .map(|(k, _, v)| k.len() as u64 + v.as_ref().map_or(0, approx_bytes))
                .sum::<u64>()
        }
        CanonValue::Enum {
            definition_key,
            variant,
        } => (definition_key.len() + variant.len()) as u64,
        CanonValue::Media { content, .. } => content.len() as u64,
        CanonValue::Omitted { message, .. } => message.len() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::{CancellationToken, QueryBudgets};
    use std::sync::atomic::{AtomicU64, Ordering};

    struct CountingResolver {
        calls: AtomicU64,
    }

    impl ValueResolver for CountingResolver {
        fn resolve(&self, handle: &[u8], _caps: DecodeCaps) -> Resolved {
            self.calls.fetch_add(1, Ordering::Relaxed);
            if handle == b"gone" {
                Resolved::Unavailable(UnavailableReason::Lost)
            } else {
                Resolved::Value(Arc::new(CanonValue::Int(handle.len() as i64)))
            }
        }

        fn resolve_cid(&self, _cid: &[u8; 32], _caps: DecodeCaps) -> Resolved {
            Resolved::Unavailable(UnavailableReason::Unsupported)
        }
    }

    #[test]
    fn hydration_deduplicates_and_counts_budget() {
        let resolver = Arc::new(CountingResolver {
            calls: AtomicU64::new(0),
        });
        let mut budgets = QueryBudgets::unlimited();
        budgets.max_hydrations = 2;
        let tracker = BudgetTracker::new(budgets, CancellationToken::new());
        let ctx = HydrationContext::new(resolver.clone(), tracker, 64);

        // Same handle three times: one resolver call, one budget unit.
        for _ in 0..3 {
            assert!(matches!(ctx.resolve(b"h1"), Resolved::Value(_)));
        }
        assert_eq!(resolver.calls.load(Ordering::Relaxed), 1);
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
        let resolver = Arc::new(CountingResolver {
            calls: AtomicU64::new(0),
        });
        let tracker = BudgetTracker::new(QueryBudgets::unlimited(), CancellationToken::new());
        let ctx = HydrationContext::new(resolver.clone(), tracker, 64);
        for _ in 0..2 {
            assert!(matches!(
                ctx.resolve(b"gone"),
                Resolved::Unavailable(UnavailableReason::Lost)
            ));
        }
        assert_eq!(
            resolver.calls.load(Ordering::Relaxed),
            1,
            "misses cached too"
        );
    }
}
