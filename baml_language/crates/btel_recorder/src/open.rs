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

/// A link of a chain of ancestors: a node, its depth, and its parent's link.
/// Made for a node only once something is placed under it.
#[derive(Debug)]
pub(crate) struct Lineage {
    id: u64,
    depth: u32,
    parent: Option<Arc<Lineage>>,
}

/// Where a span sits: its depth and its parent's link (none for a root).
#[derive(Clone, Debug)]
pub(crate) struct Spot {
    depth: u32,
    parent: Option<Arc<Lineage>>,
}

/// Where a node sits, or `None` where that cannot be known.
pub(crate) type Place = Option<Spot>;

impl Spot {
    /// A root thread: depth 0, no ancestor.
    pub(crate) fn root() -> Self {
        Self {
            depth: 0,
            parent: None,
        }
    }

    /// The link of node `id`, which sits here, for what is placed under it.
    fn link(&self, id: u64) -> Arc<Lineage> {
        Arc::new(Lineage {
            id,
            depth: self.depth,
            parent: self.parent.clone(),
        })
    }

    /// The lineage a span's end record carries: its depth, and its nearest
    /// ancestors, nearest first, at most `LINEAGE_ANCESTORS`.
    pub(crate) fn wire(&self) -> proto::Lineage {
        self.wire_into(Vec::new())
    }

    /// [`Self::wire`] into `ancestors`, emptied first: a writer reuses one
    /// allocation for every span.
    pub(crate) fn wire_into(&self, mut ancestors: Vec<u64>) -> proto::Lineage {
        ancestors.clear();
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
/// time, not by recursion. A link still shared, the usual case, is released
/// with one decrement.
impl Drop for Lineage {
    fn drop(&mut self) {
        let mut next = self.parent.take();
        while let Some(link) = next {
            if Arc::strong_count(&link) > 1 {
                break;
            }
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
    /// The node's own link, once something was placed under it.
    link: Option<Arc<Lineage>>,
}

/// The open spans of one recording.
#[derive(Default)]
pub(crate) struct OpenSpans {
    /// Every open node that can be a parent, threads and calls, with its place and, for a
    /// call, what its start carried.
    nodes: HashMap<u64, CallStart>,
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
    /// The place of a node under `parent`: one deeper, under its link, which
    /// is made the first time.
    fn under(&mut self, parent: u64) -> Place {
        let node = self.nodes.get_mut(&parent)?;
        let spot = node.place.as_ref()?;
        let link = node.link.get_or_insert_with(|| spot.link(parent)).clone();
        Some(Spot {
            depth: spot.depth.saturating_add(1),
            parent: Some(link),
        })
    }

    /// The spawner names future `child` while `parent` is open.
    pub(crate) fn spawned(&mut self, child: u64, parent: u64) {
        let place = self.under(parent);
        self.spawned.insert(child, place);
    }

    /// Thread `id` starts: a root, or a future its spawner named, or one the
    /// host started under `parent`.
    pub(crate) fn thread_started(&mut self, id: u64, parent: Option<u64>) {
        let place = match parent {
            None => Some(Spot::root()),
            Some(parent) => match self.spawned.remove(&id) {
                Some(place) => place,
                None => self.under(parent),
            },
        };
        self.nodes.insert(
            id,
            CallStart {
                place,
                ..CallStart::default()
            },
        );
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
        let place = self.nodes.remove(&id).and_then(|node| node.place);
        (place, self.running.remove(&id))
    }

    pub(crate) fn call_started(
        &mut self,
        id: u64,
        parent: u64,
        inputs: Option<proto::CasId>,
        type_args: Option<proto::CasId>,
    ) {
        let place = self.under(parent);
        self.nodes.insert(
            id,
            CallStart {
                place,
                inputs,
                type_args,
                link: None,
            },
        );
    }

    /// Call `id` completes: its start, or for a call promoted at its exit,
    /// which has none, its place under `parent`.
    pub(crate) fn call_completed(&mut self, id: u64, parent: u64) -> CallStart {
        match self.nodes.remove(&id) {
            Some(start) => start,
            None => CallStart {
                place: self.under(parent),
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
        let place = self.under(parent);
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
        let mut link: Option<Arc<Lineage>> = None;
        for id in 0..200_000 {
            link = Some(Arc::new(Lineage {
                id,
                depth: u32::try_from(id).unwrap(),
                parent: link,
            }));
        }
        drop(link);
    }
}
