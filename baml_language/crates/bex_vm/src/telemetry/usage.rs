//! Model usage rolled up the span tree as spans complete.
//!
//! Each node (a span or a thread) keeps its own usage and the totals its
//! children handed up when they completed. A node's total is both; when the
//! node completes it hands that total to its parent. A child that completes
//! after its parent is not in the parent's total: the parent's total is fixed
//! when the parent completes.
//!
//! A parent on the same thread is a node of the same `TelemetryState`, so
//! handing up is a plain merge. A future's parent is the span that spawned
//! it, on another thread, and an owned span can complete on another thread
//! than the one that started it: those children hand up through a
//! [`SharedUsage`], which the parent's thread created when it spawned them
//! and closes when the parent completes. A node whose subtree reported no
//! usage costs its completion two emptiness checks and nothing else.

use std::sync::Mutex;

use btel_records::ModelUsage;

use super::merge_usage;

/// What one open node has been charged: its own calls, and the totals of the
/// children that completed under it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NodeUsage {
    pub(crate) own: Vec<ModelUsage>,
    pub(crate) below: Vec<ModelUsage>,
}

impl NodeUsage {
    /// `own` and `below`, summed per model.
    pub(crate) fn total(&self) -> Vec<ModelUsage> {
        let mut total = self.own.clone();
        merge_usage(&mut total, self.below.clone());
        total
    }

    pub(crate) fn merge(&mut self, other: Self) {
        merge_usage(&mut self.own, other.own);
        merge_usage(&mut self.below, other.below);
    }
}

/// The totals children on other threads hand to one node, until the node
/// completes and closes it. A child that completes after that hands nothing.
#[derive(Debug)]
pub struct SharedUsage(Mutex<Option<Vec<ModelUsage>>>);

impl Default for SharedUsage {
    fn default() -> Self {
        Self(Mutex::new(Some(Vec::new())))
    }
}

impl SharedUsage {
    /// A child's total, counted unless its parent already completed.
    pub(crate) fn add(&self, total: &[ModelUsage]) {
        if total.is_empty() {
            return;
        }
        if let Some(below) = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_mut()
        {
            merge_usage(below, total.to_vec());
        }
    }

    /// The parent completes: what its children handed up so far, and nothing
    /// from now on.
    pub(crate) fn close(&self) -> Vec<ModelUsage> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .unwrap_or_default()
    }
}
