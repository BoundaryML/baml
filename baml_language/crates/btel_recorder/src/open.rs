//! What the recorder keeps of a span between its start and its end, so that
//! the end record carries the whole span (format minor 16), and, for a span
//! still open when its file seals, the slim start record that file carries
//! (minor 17). A span that starts and ends in one file has none.
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

use btel_core::recording::LINEAGE_ANCESTORS;
use rustc_hash::FxHashMap as HashMap;

use crate::proto;

/// A link of a chain of ancestors: a node and its parent's link. Made for a
/// node only once something is placed under it.
#[derive(Debug)]
pub(crate) struct Lineage {
    id: u64,
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
    /// Events past the cap, counted and not kept.
    pub(crate) dropped_events: u64,
    /// A sealed file carries its start record.
    announced: bool,
    /// What its start record would take in a file, until one carries it.
    start_bytes: usize,
}

/// The slim start record of a span, written if the span is still open when
/// its file seals: in a section of `thread`, under the context it started in.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Start {
    pub(crate) thread: u64,
    pub(crate) context: Option<proto::thread_section::Context>,
    pub(crate) record: StartRecord,
    /// What it takes in a file, about: what the open file's size counts
    /// for it until it is written or its span ends.
    pub(crate) bytes: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum StartRecord {
    Thread,
    Call {
        parent: u64,
        call_path: u32,
        entered_at: u64,
    },
    Request {
        parent: u64,
        call_path: u32,
        started_at: u64,
        method: String,
        url: String,
    },
}

/// A call's start: what its announcement carried that its completion does not.
#[derive(Default)]
pub(crate) struct CallStart {
    pub(crate) place: Place,
    pub(crate) inputs: Option<proto::CasId>,
    pub(crate) type_args: Option<proto::CasId>,
    /// The node's own link, once something was placed under it.
    link: Option<Arc<Lineage>>,
    /// Its start record, until a sealed file carries it.
    start: Option<Box<Start>>,
    /// The context it started under, which its end record is written
    /// under; `None` for a call promoted at its exit, which has no start.
    #[allow(
        clippy::option_option,
        reason = "outer None: no start seen; inner: its context"
    )]
    pub(crate) context: Option<Option<proto::thread_section::Context>>,
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
    /// What the start records not yet written would take in the open file.
    start_bytes: usize,
    /// Events that came with no request to ride, since the last seal.
    late_events: u64,
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
    /// host started under `parent`; `context` is its section's.
    pub(crate) fn thread_started(
        &mut self,
        id: u64,
        parent: Option<u64>,
        context: Option<proto::thread_section::Context>,
        bytes: usize,
    ) {
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
                start: Some(Box::new(Start {
                    thread: id,
                    context,
                    record: StartRecord::Thread,
                    bytes,
                })),
                context: Some(context),
                ..CallStart::default()
            },
        );
        self.start_bytes += bytes;
    }

    /// A node ends: its start record is not needed unless a file carries it.
    fn ended(&mut self, node: &CallStart) {
        if let Some(start) = &node.start {
            self.start_bytes -= start.bytes;
        }
    }

    pub(crate) fn thread_running(&mut self, id: u64, at: u64) {
        self.running.entry(id).or_insert(at);
    }

    /// Thread `id` completes: its place, and when it began running. Nothing
    /// of it stays here.
    pub(crate) fn thread_completed(&mut self, id: u64) -> ThreadEnd {
        let (place, context) = self.nodes.remove(&id).map_or((None, None), |node| {
            self.ended(&node);
            (node.place, node.context)
        });
        // A future cancelled before it started never took its spawner's place.
        self.spawned.remove(&id);
        ThreadEnd {
            place,
            running_at: self.running.remove(&id),
            context,
        }
    }

    /// Call `id` starts on `thread` under `parent`, with its start record's
    /// fields (`start`) and what its completion will carry.
    pub(crate) fn call_started(
        &mut self,
        id: u64,
        parent: u64,
        inputs: Option<proto::CasId>,
        type_args: Option<proto::CasId>,
        start: Start,
    ) {
        let place = self.under(parent);
        self.start_bytes += start.bytes;
        let context = Some(start.context);
        self.nodes.insert(
            id,
            CallStart {
                place,
                inputs,
                type_args,
                link: None,
                start: Some(Box::new(start)),
                context,
            },
        );
    }

    /// Call `id` completes: its start, or for a call promoted at its exit,
    /// which has none, its place under `parent`.
    pub(crate) fn call_completed(&mut self, id: u64, parent: u64) -> CallStart {
        match self.nodes.remove(&id) {
            Some(start) => {
                self.ended(&start);
                start
            }
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
        // A start record of about a function's size, with the text.
        let start_bytes = CALL_START_BYTES + method.len() + url.len();
        self.start_bytes += start_bytes;
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
                dropped_events: 0,
                announced: false,
                start_bytes,
            },
        );
    }

    /// An event of an open request, or of one that completed in the open
    /// file: a parked request that the event says the program is done with
    /// comes back to be written. One that comes after its request was
    /// written has no span to ride and is dropped. A request keeps its first
    /// `cap` events and counts the rest.
    pub(crate) fn request_event(
        &mut self,
        id: u64,
        event: proto::NetworkEvent,
        cap: usize,
    ) -> Option<Parked> {
        if let Some(request) = self.requests.get_mut(&id) {
            if request.events.len() < cap {
                request.events.push(event);
            } else {
                request.dropped_events = request.dropped_events.saturating_add(1);
            }
            return None;
        }
        let Some(parked) = self.parked.get_mut(&id) else {
            // Its request's end record is written, or it never started here.
            self.late_events = self.late_events.saturating_add(1);
            return None;
        };
        let done = finishes(&event);
        let completion = &mut parked.completion;
        if completion.events.len() < cap {
            completion.events.push(event);
        } else {
            completion.dropped_events = completion.dropped_events.saturating_add(1);
        }
        done.then(|| self.parked.remove(&id)).flatten()
    }

    /// The start records of the spans still open that no sealed file
    /// carries yet, in the order they started; each is carried from now on.
    pub(crate) fn take_open_starts(&mut self) -> Vec<(u64, Start)> {
        let mut starts: Vec<(u64, Start)> = self
            .nodes
            .iter_mut()
            .filter_map(|(&id, node)| node.start.take().map(|start| (id, *start)))
            .collect();
        for (&id, request) in &mut self.requests {
            if !request.announced {
                request.announced = true;
                starts.push((
                    id,
                    Start {
                        thread: request.thread,
                        context: request.context,
                        record: StartRecord::Request {
                            parent: request.parent,
                            call_path: request.call_path,
                            started_at: request.started_at,
                            method: request.method.clone(),
                            url: request.url.clone(),
                        },
                        bytes: request.start_bytes,
                    },
                ));
            }
        }
        // Telemetry IDs grow as spans start.
        starts.sort_unstable_by_key(|(id, _)| *id);
        self.start_bytes = 0;
        starts
    }

    /// The start record of span `id` waits for a later file: what it
    /// references is not defined yet.
    pub(crate) fn rearm(&mut self, id: u64, start: Start) {
        if let Some(node) = self.nodes.get_mut(&id) {
            self.start_bytes += start.bytes;
            node.start = Some(Box::new(start));
        } else if let Some(request) = self.requests.get_mut(&id) {
            self.start_bytes += request.start_bytes;
            request.announced = false;
        }
    }

    /// An open request made by thread `thread` (whose completion names it).
    pub(crate) fn has_request_on(&self, thread: u64) -> bool {
        self.requests
            .values()
            .any(|request| request.thread == thread)
    }

    pub(crate) fn request_completed(&mut self, id: u64) -> Option<Request> {
        let request = self.requests.remove(&id)?;
        if !request.announced {
            self.start_bytes -= request.start_bytes;
        }
        Some(request)
    }

    /// The events that came with no request to ride since the last call.
    pub(crate) fn take_late_events(&mut self) -> u64 {
        std::mem::take(&mut self.late_events)
    }

    /// What the start records not yet written would take in the open file.
    pub(crate) fn start_bytes(&self) -> usize {
        self.start_bytes
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

/// About what a function's slim start record takes in a file: the event's
/// keys and lengths, its IDs, call path and time.
pub(crate) const CALL_START_BYTES: usize = 40;

/// What a thread's completion carries from its start.
pub(crate) struct ThreadEnd {
    pub(crate) place: Place,
    pub(crate) running_at: Option<u64>,
    /// The context it started under, when this recording saw it start.
    #[allow(
        clippy::option_option,
        reason = "outer None: no start seen; inner: its context"
    )]
    pub(crate) context: Option<Option<proto::thread_section::Context>>,
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

    fn start() -> Start {
        Start {
            thread: 1,
            context: None,
            record: StartRecord::Thread,
            bytes: 10,
        }
    }

    fn request(open: &mut OpenSpans, id: u64) {
        open.request_started(id, 1, None, 1, 0, 5, "GET".into(), "u".into(), None);
    }

    fn event(name: &str) -> proto::NetworkEvent {
        proto::NetworkEvent {
            name: name.into(),
            ..proto::NetworkEvent::default()
        }
    }

    #[test]
    fn only_spans_open_at_seal_carry_a_start_and_only_once() {
        let mut open = OpenSpans::default();
        open.thread_started(1, None, None, 7);
        open.call_started(2, 1, None, None, start());
        open.call_started(3, 2, None, None, start());
        open.call_completed(3, 2);
        request(&mut open, 4);
        // The thread's 7, the open call's 10 and the request's.
        assert_eq!(open.start_bytes(), 7 + 10 + CALL_START_BYTES + 4);
        let ids: Vec<u64> = open.take_open_starts().iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, vec![1, 2, 4]);
        assert_eq!(open.start_bytes(), 0);
        // A later file does not carry them again; a span started since does.
        open.call_started(5, 2, None, None, start());
        let ids: Vec<u64> = open.take_open_starts().iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, vec![5]);
    }

    #[test]
    fn a_request_keeps_its_first_events_and_counts_the_rest() {
        let mut open = OpenSpans::default();
        open.thread_started(1, None, None, 7);
        request(&mut open, 4);
        for _ in 0..5 {
            assert!(open.request_event(4, event("data"), 3).is_none());
        }
        let request = open.request_completed(4).unwrap();
        assert_eq!((request.events.len(), request.dropped_events), (3, 2));
        // Parked, past the cap: the event that finishes it still writes it.
        open.park(Parked {
            context: None,
            completion: proto::NetworkCompletion {
                span_id: 4,
                events: request.events,
                dropped_events: request.dropped_events,
                ..proto::NetworkCompletion::default()
            },
        });
        let parked = open.request_event(4, event("await"), 3).unwrap();
        assert_eq!(
            (
                parked.completion.events.len(),
                parked.completion.dropped_events
            ),
            (3, 3)
        );
    }

    #[test]
    fn a_finished_thread_leaves_nothing_behind() {
        let mut open = OpenSpans::default();
        open.thread_started(1, None, None, 7);
        open.call_started(2, 1, None, None, start());
        // Future 10 is spawned and cancelled before it ever starts.
        open.spawned(10, 2);
        open.thread_running(10, 7);
        open.thread_completed(10);
        assert!(open.spawned.is_empty() && open.running.is_empty());
    }

    #[test]
    fn a_span_is_one_deeper_than_its_parent_with_its_parent_first() {
        let mut open = OpenSpans::default();
        open.thread_started(1, None, None, 7);
        open.call_started(2, 1, None, None, start());
        open.call_started(3, 2, None, None, start());
        let start = open.call_completed(3, 2);
        let lineage = start.place.unwrap().wire();
        assert_eq!(lineage.depth, 2);
        assert_eq!(lineage.truncated_ancestor_span_ids, vec![2, 1]);
        let thread = open.thread_completed(1).place;
        assert_eq!(thread.unwrap().wire().depth, 0);
    }

    #[test]
    fn ancestors_stop_at_the_cap_and_depth_does_not() {
        let mut open = OpenSpans::default();
        open.thread_started(1, None, None, 7);
        let deepest = 1 + LINEAGE_ANCESTORS as u64 + 10;
        for id in 2..=deepest {
            open.call_started(id, id - 1, None, None, start());
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
        open.thread_started(1, None, None, 7);
        open.call_started(2, 1, None, None, start());
        open.spawned(10, 2);
        open.call_completed(2, 1);
        open.thread_started(10, Some(2), None, 7);
        open.call_started(11, 10, None, None, start());
        let lineage = open.call_completed(11, 10).place.unwrap().wire();
        assert_eq!(lineage.depth, 3);
        assert_eq!(lineage.truncated_ancestor_span_ids, vec![10, 2, 1]);
    }

    #[test]
    fn a_late_call_takes_its_place_under_its_parent() {
        let mut open = OpenSpans::default();
        open.thread_started(1, None, None, 7);
        let lineage = open.call_completed(5, 1).place.unwrap().wire();
        assert_eq!(
            (lineage.depth, lineage.truncated_ancestor_span_ids),
            (1, vec![1])
        );
    }

    #[test]
    fn a_span_whose_parent_is_unknown_has_no_place() {
        let mut open = OpenSpans::default();
        open.thread_started(7, Some(99), None, 7);
        open.call_started(8, 7, None, None, start());
        assert!(open.call_completed(8, 7).place.is_none());
    }

    #[test]
    fn a_deep_chain_drops_without_recursing() {
        let mut link: Option<Arc<Lineage>> = None;
        for id in 0..200_000 {
            link = Some(Arc::new(Lineage { id, parent: link }));
        }
        drop(link);
    }
}
