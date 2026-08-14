use std::collections::{HashMap, VecDeque};

use crate::{
    ids::BoundaryId,
    value::{ValueCodec, ValueRef},
};

pub const DEFAULT_NATIVE_LIVE_VALUE_CACHE_BYTES: usize = 64 * 1024 * 1024;
pub const DEFAULT_WASM_LIVE_VALUE_CACHE_BYTES: usize = 16 * 1024 * 1024;

const DEFAULT_EVICTION_TOMBSTONES: usize = 4096;
const LRU_COMPACTION_FACTOR: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LiveValueKey {
    pub boundary_id: BoundaryId,
    pub value_ref_id: String,
}

impl LiveValueKey {
    #[must_use]
    pub fn new(boundary_id: BoundaryId, value_ref_id: impl Into<String>) -> Self {
        Self {
            boundary_id,
            value_ref_id: value_ref_id.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveValueBody {
    pub codec: ValueCodec,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveValueEviction {
    pub body_size_bytes: usize,
    pub diagnostic: String,
    sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveValueLookup {
    Available(LiveValueBody),
    Evicted(LiveValueEviction),
    Missing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveValueInsertResult {
    pub retained: bool,
    pub evicted_entries: usize,
    pub evicted_bytes: usize,
    pub diagnostic: Option<String>,
}

#[derive(Debug)]
pub struct LiveValueCache {
    max_bytes: usize,
    max_eviction_tombstones: usize,
    current_bytes: usize,
    sequence: u64,
    entries: HashMap<LiveValueKey, LiveValueEntry>,
    lru: VecDeque<(u64, LiveValueKey)>,
    evicted: HashMap<LiveValueKey, LiveValueEviction>,
    evicted_order: VecDeque<(u64, LiveValueKey)>,
}

#[derive(Clone, Debug)]
struct LiveValueEntry {
    body: LiveValueBody,
    size_bytes: usize,
    sequence: u64,
}

impl LiveValueCache {
    #[must_use]
    pub fn with_max_bytes(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            max_eviction_tombstones: DEFAULT_EVICTION_TOMBSTONES,
            current_bytes: 0,
            sequence: 0,
            entries: HashMap::new(),
            lru: VecDeque::new(),
            evicted: HashMap::new(),
            evicted_order: VecDeque::new(),
        }
    }

    #[must_use]
    pub fn current_bytes(&self) -> usize {
        self.current_bytes
    }

    pub fn insert(
        &mut self,
        boundary_id: BoundaryId,
        value_ref: &ValueRef,
        body: LiveValueBody,
    ) -> LiveValueInsertResult {
        let key = LiveValueKey::new(boundary_id, value_ref.id.clone());
        self.evicted.remove(&key);
        self.compact_evicted_order_if_needed();
        if let Some(previous) = self.entries.remove(&key) {
            self.current_bytes = self.current_bytes.saturating_sub(previous.size_bytes);
        }

        let size_bytes = body.body.len();
        if size_bytes > self.max_bytes {
            let diagnostic = format!(
                "live value body is {size_bytes} bytes, exceeding the live cache budget of {} bytes",
                self.max_bytes
            );
            self.remember_eviction(key, size_bytes, diagnostic.clone());
            return LiveValueInsertResult {
                retained: false,
                evicted_entries: 0,
                evicted_bytes: 0,
                diagnostic: Some(diagnostic),
            };
        }

        let sequence = self.next_sequence();
        self.entries.insert(
            key.clone(),
            LiveValueEntry {
                body,
                size_bytes,
                sequence,
            },
        );
        self.current_bytes = self.current_bytes.saturating_add(size_bytes);
        self.lru.push_back((sequence, key));
        self.compact_lru_if_needed();
        let (evicted_entries, evicted_bytes) = self.evict_until_within_budget();
        LiveValueInsertResult {
            retained: true,
            evicted_entries,
            evicted_bytes,
            diagnostic: None,
        }
    }

    pub fn get(&mut self, boundary_id: BoundaryId, value_ref_id: &str) -> LiveValueLookup {
        let key = LiveValueKey::new(boundary_id, value_ref_id);
        let sequence = self.next_sequence();
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.sequence = sequence;
            self.lru.push_back((sequence, key));
            let body = entry.body.clone();
            self.compact_lru_if_needed();
            return LiveValueLookup::Available(body);
        }
        self.evicted
            .get(&key)
            .cloned()
            .map_or(LiveValueLookup::Missing, LiveValueLookup::Evicted)
    }

    fn evict_until_within_budget(&mut self) -> (usize, usize) {
        let mut evicted_entries = 0_usize;
        let mut evicted_bytes = 0_usize;
        while self.current_bytes > self.max_bytes {
            let Some((sequence, key)) = self.lru.pop_front() else {
                break;
            };
            let Some(entry) = self.entries.get(&key) else {
                continue;
            };
            if entry.sequence != sequence {
                continue;
            }
            let Some(entry) = self.entries.remove(&key) else {
                continue;
            };
            self.current_bytes = self.current_bytes.saturating_sub(entry.size_bytes);
            evicted_entries = evicted_entries.saturating_add(1);
            evicted_bytes = evicted_bytes.saturating_add(entry.size_bytes);
            self.remember_eviction(
                key,
                entry.size_bytes,
                "live value body was evicted from the bounded live cache".to_string(),
            );
        }
        self.compact_lru_if_needed();
        (evicted_entries, evicted_bytes)
    }

    fn remember_eviction(&mut self, key: LiveValueKey, body_size_bytes: usize, diagnostic: String) {
        let sequence = self.next_sequence();
        self.evicted.insert(
            key.clone(),
            LiveValueEviction {
                body_size_bytes,
                diagnostic,
                sequence,
            },
        );
        self.evicted_order.push_back((sequence, key));
        while self.evicted.len() > self.max_eviction_tombstones {
            let Some((sequence, key)) = self.evicted_order.pop_front() else {
                break;
            };
            if self
                .evicted
                .get(&key)
                .is_some_and(|eviction| eviction.sequence == sequence)
            {
                self.evicted.remove(&key);
            }
        }
        self.compact_evicted_order_if_needed();
    }

    fn compact_lru_if_needed(&mut self) {
        let live_len = self.entries.len().max(1);
        if self.lru.len() <= live_len.saturating_mul(LRU_COMPACTION_FACTOR) {
            return;
        }

        let mut entries = self
            .entries
            .iter()
            .map(|(key, entry)| (entry.sequence, key.clone()))
            .collect::<Vec<_>>();
        entries.sort_by_key(|(sequence, _)| *sequence);
        self.lru = entries.into_iter().collect();
    }

    fn compact_evicted_order_if_needed(&mut self) {
        let evicted_len = self.evicted.len().max(1);
        if self.evicted_order.len() <= evicted_len.saturating_mul(LRU_COMPACTION_FACTOR) {
            return;
        }

        let mut entries = self
            .evicted
            .iter()
            .map(|(key, eviction)| (eviction.sequence, key.clone()))
            .collect::<Vec<_>>();
        entries.sort_by_key(|(sequence, _)| *sequence);
        self.evicted_order = entries.into_iter().collect();
    }

    fn next_sequence(&mut self) -> u64 {
        self.sequence = self.sequence.saturating_add(1);
        self.sequence
    }
}

#[cfg(test)]
mod tests {
    use super::{LRU_COMPACTION_FACTOR, LiveValueBody, LiveValueCache, LiveValueLookup};
    use crate::{
        ids::BoundaryId,
        value::{ValueCodec, ValueRef},
    };

    fn value_ref(id: &str, size: usize) -> ValueRef {
        ValueRef::available(id, ValueCodec::BamlOutboundValue, size, size)
    }

    #[test]
    fn live_value_cache_evicts_least_recently_used_entries_by_byte_budget() {
        let boundary_id = BoundaryId::from_bytes([1; 16]);
        let mut cache = LiveValueCache::with_max_bytes(6);
        cache.insert(
            boundary_id,
            &value_ref("value_1", 3),
            LiveValueBody {
                codec: ValueCodec::BamlOutboundValue,
                body: vec![1, 2, 3],
            },
        );
        cache.insert(
            boundary_id,
            &value_ref("value_2", 2),
            LiveValueBody {
                codec: ValueCodec::BamlOutboundValue,
                body: vec![4, 5],
            },
        );
        assert!(matches!(
            cache.get(boundary_id, "value_1"),
            LiveValueLookup::Available(_)
        ));

        let result = cache.insert(
            boundary_id,
            &value_ref("value_3", 3),
            LiveValueBody {
                codec: ValueCodec::BamlOutboundValue,
                body: vec![6, 7, 8],
            },
        );

        assert!(result.retained);
        assert_eq!(result.evicted_entries, 1);
        assert!(matches!(
            cache.get(boundary_id, "value_2"),
            LiveValueLookup::Evicted(_)
        ));
        assert!(matches!(
            cache.get(boundary_id, "value_1"),
            LiveValueLookup::Available(_)
        ));
        assert!(matches!(
            cache.get(boundary_id, "value_3"),
            LiveValueLookup::Available(_)
        ));
    }

    #[test]
    fn live_value_cache_records_oversized_values_as_evicted() {
        let boundary_id = BoundaryId::from_bytes([2; 16]);
        let mut cache = LiveValueCache::with_max_bytes(2);

        let result = cache.insert(
            boundary_id,
            &value_ref("value_big", 3),
            LiveValueBody {
                codec: ValueCodec::BamlOutboundValue,
                body: vec![1, 2, 3],
            },
        );

        assert!(!result.retained);
        assert_eq!(cache.current_bytes(), 0);
        assert!(matches!(
            cache.get(boundary_id, "value_big"),
            LiveValueLookup::Evicted(_)
        ));
    }

    #[test]
    fn live_value_cache_compacts_lru_tombstones_after_repeated_reads() {
        let boundary_id = BoundaryId::from_bytes([3; 16]);
        let mut cache = LiveValueCache::with_max_bytes(4);
        cache.insert(
            boundary_id,
            &value_ref("value_1", 1),
            LiveValueBody {
                codec: ValueCodec::BamlOutboundValue,
                body: vec![1],
            },
        );

        for _ in 0..64 {
            assert!(matches!(
                cache.get(boundary_id, "value_1"),
                LiveValueLookup::Available(_)
            ));
        }

        let max_lru_len = cache.entries.len().max(1) * LRU_COMPACTION_FACTOR;
        assert!(
            cache.lru.len() <= max_lru_len,
            "lru should stay compacted; len={} max={max_lru_len}",
            cache.lru.len()
        );
    }

    #[test]
    fn live_value_cache_compacts_eviction_tombstones_after_reinsert() {
        let boundary_id = BoundaryId::from_bytes([4; 16]);
        let mut cache = LiveValueCache::with_max_bytes(1);

        for _ in 0..64 {
            cache.insert(
                boundary_id,
                &value_ref("value_1", 2),
                LiveValueBody {
                    codec: ValueCodec::BamlOutboundValue,
                    body: vec![1, 2],
                },
            );
            assert!(matches!(
                cache.get(boundary_id, "value_1"),
                LiveValueLookup::Evicted(_)
            ));

            cache.insert(
                boundary_id,
                &value_ref("value_1", 1),
                LiveValueBody {
                    codec: ValueCodec::BamlOutboundValue,
                    body: vec![1],
                },
            );
        }

        assert!(cache.evicted.is_empty());
        let max_evicted_order_len = cache.evicted.len().max(1) * LRU_COMPACTION_FACTOR;
        assert!(
            cache.evicted_order.len() <= max_evicted_order_len,
            "evicted order should stay compacted; len={} max={max_evicted_order_len}",
            cache.evicted_order.len()
        );
    }
}
