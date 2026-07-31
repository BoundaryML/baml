//! §5.8 recent-call ring: per partition, the last `R` completed calls in
//! fixed slots. The exact-recency source for the default-mode timeline;
//! eviction is counted, never silent ("showing last 4096 of N").

/// Default ring capacity (design R = 4096).
pub const RECENT_RING_SLOTS: usize = 4096;

/// One completed call (the §5.8 slot; `thread_idx` is the partition-local
/// thread index — `call_id` alone is per-thread, the pair is the key).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecentCall {
    pub thread_idx: u32,
    pub call_id: u64,
    pub node: u32,
    pub parent_call_id: u64,
    pub start_ns: u64,
    pub end_ns: u64,
    pub status: u8,
    /// Flight-recorder dump reference (P6); 0 = none.
    pub dump_ref: u32,
}

/// Fixed-capacity overwrite ring. Capacity rounds up to a power of two so
/// wraparound is a mask, not an integer division (this push is on the
/// consumer's per-close path).
pub struct RecentRing {
    slots: Vec<RecentCall>,
    head: usize,
    len: usize,
    mask: usize,
    /// Total completed calls ever pushed (UI renders "last {len} of {total}").
    pub total_pushed: u64,
}

impl RecentRing {
    #[must_use]
    pub fn new(capacity: usize) -> RecentRing {
        let capacity = capacity.next_power_of_two().max(2);
        RecentRing {
            slots: Vec::with_capacity(64.min(capacity)),
            head: 0,
            len: 0,
            mask: capacity - 1,
            total_pushed: 0,
        }
    }

    #[inline]
    #[cfg_attr(not(test), expect(dead_code, reason = "test assertion surface"))]
    fn capacity(&self) -> usize {
        self.mask + 1
    }

    /// Push one completed call; returns whether an old slot was evicted.
    #[inline]
    pub fn push(&mut self, call: RecentCall) -> bool {
        self.total_pushed += 1;
        if self.len <= self.mask {
            if self.slots.len() == self.len {
                self.slots.push(call);
            } else {
                let idx = (self.head + self.len) & self.mask;
                self.slots[idx] = call;
            }
            self.len += 1;
            false
        } else {
            self.slots[self.head] = call;
            self.head = (self.head + 1) & self.mask;
            true
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Oldest → newest iteration.
    pub fn iter(&self) -> impl Iterator<Item = &RecentCall> {
        (0..self.len).map(move |i| &self.slots[(self.head + i) & self.mask])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(id: u64) -> RecentCall {
        RecentCall {
            thread_idx: 0,
            call_id: id,
            node: 1,
            parent_call_id: 0,
            start_ns: id,
            end_ns: id + 1,
            status: 0,
            dump_ref: 0,
        }
    }

    #[test]
    fn ring_overwrites_oldest_and_counts() {
        // Capacity rounds to the next power of two (3 → 4).
        let mut ring = RecentRing::new(3);
        assert_eq!(ring.capacity(), 4);
        assert!(!ring.push(call(1)));
        assert!(!ring.push(call(2)));
        assert!(!ring.push(call(3)));
        assert!(!ring.push(call(4)));
        assert!(ring.push(call(5)), "push past capacity evicts");
        let ids: Vec<u64> = ring.iter().map(|c| c.call_id).collect();
        assert_eq!(ids, vec![2, 3, 4, 5]);
        assert_eq!(ring.total_pushed, 5);
    }
}
