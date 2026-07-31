//! Logical-thread stacks, open-call index entries, and the exact recent ring.

use std::collections::VecDeque;

use super::{nodes::NodeId, spawn::SpawnEdgeId};
use crate::prof::record::FunctionEndStatus;

pub const RECENT_CALL_CAPACITY: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CallKey {
    pub thread_id: u64,
    pub call_id: u64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ActiveCall {
    pub key: CallKey,
    pub node: NodeId,
    pub start_ns: u64,
    pub parent_call_id: u64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Suspend {
    pub seq: u64,
}

#[derive(Debug)]
pub(crate) struct ThreadState {
    pub partition: u32,
    pub root_node: NodeId,
    pub stack: Vec<ActiveCall>,
    /// Most recently resolved `(parent, function, node)` identity. Repeated
    /// calls from the same context are the common case, so this avoids a
    /// hash-table probe without weakening the canonical `NodeStore` identity.
    pub intern_cache: Option<(NodeId, u32, NodeId)>,
    pub last_charge_ns: u64,
    pub watermark_ns: u64,
    pub suspended: Option<Suspend>,
    pub resumed_recent: VecDeque<u64>,
    pub spawn_ctx_node: NodeId,
    pub entry_edge: Option<SpawnEdgeId>,
    pub started_ns: u64,
    pub name: Option<String>,
    pub is_spawned: bool,
}

impl ThreadState {
    #[inline]
    pub(crate) fn target_node(&self) -> NodeId {
        self.stack
            .last()
            .map_or(self.root_node, |active| active.node)
    }
}

/// Exact data kept for the last completed calls. Open calls remain exact in
/// `ThreadState::stack` and therefore do not consume these slots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentCall {
    pub thread_id: u64,
    pub call_id: u64,
    pub node_id: NodeId,
    pub parent_call_id: u64,
    pub start_ns: u64,
    pub end_ns: u64,
    pub status: FunctionEndStatus,
    /// Index into a partition-local exact dump table; zero means no dump.
    pub dump_ref: u32,
}

#[derive(Debug, Default)]
pub(crate) struct RecentCalls {
    calls: Vec<RecentCall>,
    /// Next slot to overwrite once `calls` reaches exact capacity.
    next: usize,
    pub evicted_calls: u64,
}

impl RecentCalls {
    #[inline]
    pub(crate) fn push(&mut self, call: RecentCall) {
        if self.calls.len() < RECENT_CALL_CAPACITY {
            self.calls.push(call);
            return;
        }
        self.calls[self.next] = call;
        self.next = (self.next + 1) & (RECENT_CALL_CAPACITY - 1);
        self.evicted_calls = self.evicted_calls.saturating_add(1);
    }

    pub(crate) fn snapshot(&self) -> Vec<RecentCall> {
        if self.calls.len() < RECENT_CALL_CAPACITY || self.next == 0 {
            return self.calls.clone();
        }
        self.calls[self.next..]
            .iter()
            .chain(&self.calls[..self.next])
            .cloned()
            .collect()
    }

    pub(crate) fn find(&self, key: CallKey) -> Option<&RecentCall> {
        if self.calls.len() < RECENT_CALL_CAPACITY {
            return self
                .calls
                .iter()
                .rev()
                .find(|call| call.thread_id == key.thread_id && call.call_id == key.call_id);
        }
        (0..RECENT_CALL_CAPACITY).find_map(|offset| {
            let index = self
                .next
                .wrapping_add(RECENT_CALL_CAPACITY - 1)
                .wrapping_sub(offset)
                & (RECENT_CALL_CAPACITY - 1);
            let call = &self.calls[index];
            (call.thread_id == key.thread_id && call.call_id == key.call_id).then_some(call)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{CallKey, RECENT_CALL_CAPACITY, RecentCall, RecentCalls};
    use crate::prof::record::FunctionEndStatus;

    #[test]
    fn recent_call_slot_is_the_committed_56_bytes() {
        assert_eq!(std::mem::size_of::<RecentCall>(), 56);
    }

    #[test]
    fn recent_ring_preserves_newest_exact_calls_in_completion_order() {
        let mut recent = RecentCalls::default();
        for call_id in 1..=RECENT_CALL_CAPACITY as u64 + 2 {
            recent.push(RecentCall {
                thread_id: 7,
                call_id,
                node_id: 1,
                parent_call_id: 0,
                start_ns: call_id,
                end_ns: call_id + 1,
                status: FunctionEndStatus::Ok,
                dump_ref: 0,
            });
        }

        let snapshot = recent.snapshot();
        assert_eq!(snapshot.len(), RECENT_CALL_CAPACITY);
        assert_eq!(snapshot.first().map(|call| call.call_id), Some(3));
        assert_eq!(
            snapshot.last().map(|call| call.call_id),
            Some(RECENT_CALL_CAPACITY as u64 + 2)
        );
        assert!(
            recent
                .find(CallKey {
                    thread_id: 7,
                    call_id: 2
                })
                .is_none()
        );
        assert_eq!(
            recent
                .find(CallKey {
                    thread_id: 7,
                    call_id: RECENT_CALL_CAPACITY as u64 + 1,
                })
                .map(|call| call.call_id),
            Some(RECENT_CALL_CAPACITY as u64 + 1)
        );
    }
}
