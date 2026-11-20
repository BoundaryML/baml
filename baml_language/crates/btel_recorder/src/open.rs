//! What the recorder keeps of a span between its start and its end, so that
//! the end record carries the whole span (format minor 16).
//!
//! A span's records arrive in the order its thread wrote them, and a spawned
//! future's after its spawner's `ThreadSpawned` (the spawner leaves its
//! execution scope to spawn, which publishes what it wrote). So when a span
//! starts, its parent is open here, and the span's place in the tree is its
//! parent's plus one. A place is a shared link to the parent's: a span costs
//! one small allocation, never a copy of its ancestors. A span whose parent
//! is not open here (an invocation the host started under a span that has
//! completed) has no place: its lineage is unknown, and so is that of every
//! span under it.

use std::sync::Arc;

use rustc_hash::FxHashMap as HashMap;

use btel_core::recording::LINEAGE_ANCESTORS;

use crate::proto;

/// A node's depth, and its parent's place: the chain of its ancestors.
#[derive(Debug)]
pub(crate) struct Lineage {
    id: u64,
    depth: u32,
    parent: Option<Arc<Lineage>>,
}

/// Where a node sits, or `None` where that cannot be known.
pub(crate) type Place = Option<Arc<Lineage>>;

impl Lineage {
    /// A root thread: depth 0, no ancestor.
    pub(crate) fn root(id: u64) -> Arc<Self> {
        Arc::new(Self {
            id,
            depth: 0,
            parent: None,
        })
    }

    /// Node `id` under `parent`.
    pub(crate) fn child(parent: &Arc<Self>, id: u64) -> Arc<Self> {
        Arc::new(Self {
            id,
            depth: parent.depth.saturating_add(1),
            parent: Some(Arc::clone(parent)),
        })
    }

    /// The lineage a span's end record carries: its depth, and its nearest
    /// ancestors, nearest first, at most `LINEAGE_ANCESTORS`.
    pub(crate) fn wire(&self) -> proto::Lineage {
        let mut ancestors = Vec::new();
        let mut next = self.parent.as_deref();
        while let Some(ancestor) = next {
            if ancestors.len() == LINEAGE_ANCESTORS {
                break;
            }
            ancestors.push(ancestor.id);
            next = ancestor.parent.as_deref();
        }
        proto::Lineage {
            depth: self.depth,
            truncated_ancestor_span_ids: ancestors,
        }
    }
}

/// A chain as deep as the program's recursion goes is dropped one link at a
/// time, not by recursion.
impl Drop for Lineage {
    fn drop(&mut self) {
        let mut next = self.parent.take();
        while let Some(link) = next {
            match Arc::try_unwrap(link) {
                Ok(mut lineage) => next = lineage.parent.take(),
                Err(_) => break,
            }
        }
    }
}

/// A request's start, and its events so far.
pub(crate) struct Request {
    pub(crate) thread: u64,
    /// The context its start was observed under, which its end record is
    /// written under, in a section of its own thread.
    pub(crate) context: Option<proto::thread_section::Context>,
    pub(crate) parent: u64,
    pub(crate) call_path: u32,
    pub(crate) started_at: u64,
    pub(crate) method: String,
    pub(crate) url: String,
    #[allow(clippy::struct_field_names, reason = "the request a request span sent")]
    pub(crate) request: Option<proto::CasId>,
    pub(crate) place: Place,
    pub(crate) events: Vec<proto::NetworkEvent>,
}

/// A call's start: what its announcement carried that its completion does not.
#[derive(Default)]
pub(crate) struct CallStart {
    pub(crate) place: Place,
    pub(crate) inputs: Option<proto::CasId>,
    pub(crate) type_args: Option<proto::CasId>,
}

/// The open spans of one recording.
#[derive(Default)]
pub(crate) struct OpenSpans {
    /// The place of every open node that can be a parent: threads and calls.
    nodes: HashMap<u64, Place>,
    calls: HashMap<u64, CallStart>,
    requests: HashMap<u64, Request>,
    /// When each open spawned thread began running, once it did.
    running: HashMap<u64, u64>,
    /// Requests that completed in the open file, written when it seals or
    /// when the program is done with the response: the program usually
    /// finishes reading a body (`await`) after its transfer completed.
    parked: std::collections::BTreeMap<u64, Parked>,
    /// The place of each spawned future whose own records have not started
    /// it yet, from its spawner's `ThreadSpawned`.
    spawned: HashMap<u64, Place>,
}

impl OpenSpans {
    fn under(&self, parent: u64, id: u64) -> Place {
        self.nodes
            .get(&parent)
            .and_then(Option::as_ref)
            .map(|parent| Lineage::child(parent, id))
    }

    /// The spawner names future `child` while `parent` is open.
    pub(crate) fn spawned(&mut self, child: u64, parent: u64) {
        let place = self.under(parent, child);
        self.spawned.insert(child, place);
    }

    /// Thread `id` starts: a root, or a future its spawner named, or one the
    /// host started under `parent`.
    pub(crate) fn thread_started(&mut self, id: u64, parent: Option<u64>) {
        let place = match parent {
            None => Some(Lineage::root(id)),
            Some(parent) => match self.spawned.remove(&id) {
                Some(place) => place,
                None => self.under(parent, id),
            },
        };
        self.nodes.insert(id, place);
    }

    /// Thread `id` started and has not completed.
    pub(crate) fn thread_known(&self, id: u64) -> bool {
        self.nodes.contains_key(&id)
    }

    pub(crate) fn thread_running(&mut self, id: u64, at: u64) {
        self.running.entry(id).or_insert(at);
    }

    /// Thread `id` completes: its place, and when it began running.
    pub(crate) fn thread_completed(&mut self, id: u64) -> (Place, Option<u64>) {
        let place = self.nodes.remove(&id).flatten();
        (place, self.running.remove(&id))
    }

    pub(crate) fn call_started(
        &mut self,
        id: u64,
        parent: u64,
        inputs: Option<proto::CasId>,
        type_args: Option<proto::CasId>,
    ) {
        let place = self.under(parent, id);
        self.nodes.insert(id, place.clone());
        self.calls.insert(
            id,
            CallStart {
                place,
                inputs,
                type_args,
            },
        );
    }

    /// Call `id` completes: its start, or for a call promoted at its exit,
    /// which has none, its place under `parent`.
    pub(crate) fn call_completed(&mut self, id: u64, parent: u64) -> CallStart {
        match self.calls.remove(&id) {
            Some(start) => {
                self.nodes.remove(&id);
                start
            }
            None => CallStart {
                place: self.under(parent, id),
                ..CallStart::default()
            },
        }
    }

    #[allow(clippy::too_many_arguments, reason = "the start fields of one request")]
    pub(crate) fn request_started(
        &mut self,
        id: u64,
        thread: u64,
        context: Option<proto::thread_section::Context>,
        parent: u64,
        call_path: u32,
        started_at: u64,
        method: String,
        url: String,
        request: Option<proto::CasId>,
    ) {
        let place = self.under(parent, id);
        self.requests.insert(
            id,
            Request {
                thread,
                context,
                parent,
                call_path,
                started_at,
                method,
                url,
                request,
                place,
                events: Vec::new(),
            },
        );
    }

    /// An event of an open request, or of one that completed in the open
    /// file: a parked request that the event says the program is done with
    /// comes back to be written. One that comes after its request was
    /// written has no span to ride and is dropped.
    pub(crate) fn request_event(&mut self, id: u64, event: proto::NetworkEvent) -> Option<Parked> {
        if let Some(request) = self.requests.get_mut(&id) {
            request.events.push(event);
            return None;
        }
        let parked = self.parked.get_mut(&id)?;
        let done = finishes(&event);
        parked.completion.events.push(event);
        done.then(|| self.parked.remove(&id)).flatten()
    }

    pub(crate) fn request_completed(&mut self, id: u64) -> Option<Request> {
        self.requests.remove(&id)
    }

    /// A completed request waits for the program to be done with it, until
    /// the open file seals.
    pub(crate) fn park(&mut self, parked: Parked) {
        self.parked.insert(parked.completion.span_id, parked);
    }

    /// The requests that completed in the open file, to write before it seals.
    pub(crate) fn take_parked(&mut self) -> Vec<Parked> {
        std::mem::take(&mut self.parked).into_values().collect()
    }

    pub(crate) fn has_parked(&self) -> bool {
        !self.parked.is_empty()
    }

    /// Bytes of text a request's completion will write beyond its fixed
    /// fields: its method, URL and event names.
    pub(crate) fn request_text(&self, id: u64) -> usize {
        self.requests.get(&id).map_or(0, |request| {
            request.method.len()
                + request.url.len()
                + request
                    .events
                    .iter()
                    .map(|event| event.name.len() + 64)
                    .sum::<usize>()
        })
    }
}

/// A request that completed, waiting to be written whole.
pub(crate) struct Parked {
    pub(crate) context: Option<proto::thread_section::Context>,
    pub(crate) completion: proto::NetworkCompletion,
}

/// After this event the program does nothing more with the response: it
/// finished reading it, closed it early, or it was collected unread.
pub(crate) fn finishes(event: &proto::NetworkEvent) -> bool {
    matches!(event.name.as_str(), "await" | "close" | "drop")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_span_is_one_deeper_than_its_parent_with_its_parent_first() {
        let mut open = OpenSpans::default();
        open.thread_started(1, None);
        open.call_started(2, 1, None, None);
        open.call_started(3, 2, None, None);
        let start = open.call_completed(3, 2);
        let lineage = start.place.unwrap().wire();
        assert_eq!(lineage.depth, 2);
        assert_eq!(lineage.truncated_ancestor_span_ids, vec![2, 1]);
        let (thread, _) = open.thread_completed(1);
        assert_eq!(thread.unwrap().wire().depth, 0);
    }

    #[test]
    fn ancestors_stop_at_the_cap_and_depth_does_not() {
        let mut open = OpenSpans::default();
        open.thread_started(1, None);
        let deepest = 1 + LINEAGE_ANCESTORS as u64 + 10;
        for id in 2..=deepest {
            open.call_started(id, id - 1, None, None);
        }
        let lineage = open
            .call_completed(deepest, deepest - 1)
            .place
            .unwrap()
            .wire();
        assert_eq!(u64::from(lineage.depth), deepest - 1);
        assert_eq!(lineage.truncated_ancestor_span_ids.len(), LINEAGE_ANCESTORS);
        assert_eq!(lineage.truncated_ancestor_span_ids[0], deepest - 1);
        assert_eq!(
            *lineage.truncated_ancestor_span_ids.last().unwrap(),
            deepest - LINEAGE_ANCESTORS as u64
        );
    }

    #[test]
    fn a_future_sits_under_its_spawner_though_the_spawner_completed_first() {
        let mut open = OpenSpans::default();
        open.thread_started(1, None);
        open.call_started(2, 1, None, None);
        open.spawned(10, 2);
        open.call_completed(2, 1);
        open.thread_started(10, Some(2));
        open.call_started(11, 10, None, None);
        let lineage = open.call_completed(11, 10).place.unwrap().wire();
        assert_eq!(lineage.depth, 3);
        assert_eq!(lineage.truncated_ancestor_span_ids, vec![10, 2, 1]);
    }

    #[test]
    fn a_late_call_takes_its_place_under_its_parent() {
        let mut open = OpenSpans::default();
        open.thread_started(1, None);
        let lineage = open.call_completed(5, 1).place.unwrap().wire();
        assert_eq!(
            (lineage.depth, lineage.truncated_ancestor_span_ids),
            (1, vec![1])
        );
    }

    #[test]
    fn a_span_whose_parent_is_unknown_has_no_place() {
        let mut open = OpenSpans::default();
        open.thread_started(7, Some(99));
        open.call_started(8, 7, None, None);
        assert!(open.call_completed(8, 7).place.is_none());
    }

    #[test]
    fn a_deep_chain_drops_without_recursing() {
        let mut place = Lineage::root(0);
        for id in 1..200_000 {
            place = Lineage::child(&place, id);
        }
        drop(place);
    }
}
