use once_cell::sync::Lazy;
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tokio::sync::Mutex;
use uuid::Uuid;

use baml_types::tracing::events::{FunctionId, TraceEvent};

use crate::tracingv2::publisher::publisher::PublisherMessage;

use super::super::publisher::PUBLISHING_CHANNEL;

/// Global (singleton) trace storage.
pub static BAML_TRACER: Lazy<Mutex<TraceStorage>> =
    Lazy::new(|| Mutex::new(TraceStorage::default()));

/// A unique identifier that represents a collector's identity.
pub type CollectorId = Uuid;

/// Contains details about an individual collector:
/// - Which function IDs (spans) it has subscribed to
/// - Possibly other metadata later (filters, etc.)
#[derive(Debug)]
pub struct Collector {
    pub collector_id: CollectorId,
    pub subscribed_spans: HashSet<FunctionId>,
}

impl Collector {
    /// Initializes a new collector object; you'll still need
    /// to register it with the storage so the reference counts
    /// can be tracked there.
    pub fn new() -> Self {
        Self {
            collector_id: Uuid::new_v4(),
            subscribed_spans: HashSet::new(),
        }
    }

    /// Provides read-access to the set of spans (function IDs) to which this collector is subscribed.
    pub fn subscribed_spans(&self) -> &HashSet<FunctionId> {
        &self.subscribed_spans
    }
}

/// Our main storage struct. Holds:
/// 1) A map of FunctionId -> list of events (wrapped in an Arc<Mutex<Vec<Arc<TraceEvent>>>>).
/// 2) A map of FunctionId -> reference count (how many collectors are listening).
/// 3) A map of CollectorId -> Collector details (which spans it's subscribed to).
#[derive(Default)]
pub struct TraceStorage {
    /// For each function (span), we keep an Arc<Mutex<Vec<Arc<TraceEvent>>>>>.
    /// This lets us append events asynchronously, from multiple tasks.
    span_map: HashMap<FunctionId, Arc<Mutex<Vec<Arc<TraceEvent>>>>>,
    /// For each function (span), how many collectors currently have it subscribed?
    ref_counts: HashMap<FunctionId, Arc<AtomicUsize>>,
    /// All known collectors – so we can drop them cleanly.
    collectors: HashMap<CollectorId, Arc<Collector>>,
}

impl TraceStorage {
    pub fn new() -> Self {
        Self {
            span_map: HashMap::new(),
            ref_counts: HashMap::new(),
            collectors: HashMap::new(),
        }
    }

    /// Retrieve events for a particular function (span).
    /// Returns None if the function isn't being tracked (or was dropped).
    pub fn get_events(&self, function_id: &FunctionId) -> Option<Arc<Mutex<Vec<Arc<TraceEvent>>>>> {
        self.span_map.get(function_id).cloned()
    }

    /// Creates and registers a new collector in this storage.  
    /// Once registered, the caller can subscribe the collector to function IDs (spans).
    /// Alternatively, you can do this all in a single step with `subscribe_collector_to_span`.
    pub fn register_collector(&mut self, mut collector: Arc<Collector>) -> CollectorId {
        let cid = collector.collector_id;
        // By default, the collector has no subscribed spans – we just store it.
        collector.subscribed_spans = HashSet::new();
        self.collectors.insert(cid, collector);
        cid
    }

    /// Subscribe the given collector to the specified function ID.  
    /// This effectively says "the collector wants to see events for this function ID"  
    /// Increments the reference count for that function ID, ensuring it's retained in memory.
    pub fn subscribe_collector_to_span(
        &mut self,
        collector_id: &CollectorId,
        function_id: FunctionId,
    ) {
        if let Some(collector) = self.collectors.get_mut(collector_id) {
            // If the collector is already subscribed, do nothing.
            if !collector.subscribed_spans.contains(&function_id) {
                collector.subscribed_spans.insert(function_id.clone());
                self.inc_function_id(&function_id);
            }
        } else {
            log::warn!("Collector not found: cannot subscribe to span.");
        }
    }

    /// Unsubscribe the given collector from the specified function ID.
    /// This decrements the reference count for the function ID and may remove events if the count hits zero.
    pub fn unsubscribe_collector_from_span(
        &mut self,
        collector_id: &CollectorId,
        function_id: &FunctionId,
    ) {
        if let Some(collector) = self.collectors.get_mut(collector_id) {
            if collector.subscribed_spans.remove(function_id) {
                self.dec_function_id(function_id);
            }
        }
    }

    /// Discard the entire collector (e.g. if a Python side object's finalizer got called).
    /// This unsubscribes the collector from all spans it had subscribed to.
    pub fn discard_collector(&mut self, collector_id: &CollectorId) {
        if let Some(collector) = self.collectors.remove(collector_id) {
            for function_id in collector.subscribed_spans {
                self.dec_function_id(&function_id);
            }
        }
    }

    /// Increments the reference count for the given function ID.
    /// Also ensures the underlying events map entry exists.
    pub fn inc_function_id(&mut self, function_id: &FunctionId) {
        // Ensure we have an event vector for that function ID
        self.span_map
            .entry(function_id.clone())
            .or_insert_with(|| Arc::new(Mutex::new(Vec::new())));

        // Ensure we have a reference count. Then increment it.
        let arc_counter = self
            .ref_counts
            .entry(function_id.clone())
            .or_insert_with(|| Arc::new(AtomicUsize::new(0)));
        arc_counter.fetch_add(1, Ordering::SeqCst);
    }

    /// Decrements the reference count. If it hits zero, we remove the span's events from memory.
    pub fn dec_function_id(&mut self, function_id: &FunctionId) {
        if let Some(arc_counter) = self.ref_counts.get(function_id) {
            let new_val = arc_counter.fetch_sub(1, Ordering::SeqCst);
            if new_val == 1 {
                // That means we just went from 1 to 0 – remove from memory.
                self.ref_counts.remove(function_id);
                self.span_map.remove(function_id);
            }
        }
    }

    /// Insert / record a new event for the given function (span).
    /// If the reference count is 0 (no collectors subscribed), we skip storing it.
    pub fn put(&mut self, event: Arc<TraceEvent>) {
        let span_id = event.span_id.clone();

        // If we're not tracking this span (ref_count=0), skip storing the event.
        // Because no one is subscribed, we don't need to store it in memory.
        // We still publish the event to the publisher, though, in case some external
        // system wants to handle it.
        let count = self
            .ref_counts
            .get(&span_id)
            .map(|arc| arc.load(Ordering::SeqCst))
            .unwrap_or(0);

        // We always attempt to publish
        if let Err(e) = PUBLISHING_CHANNEL.send(PublisherMessage::Trace(event.clone())) {
            log::error!("Failed to send event to publisher: {:?}", e);
        }

        if count == 0 {
            // No collectors for this span: skip local storage.
            return;
        }

        // If count > 0, we store the event locally in the span_map.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let event_clone = event.clone();
            let span_map = self.span_map.get(&span_id).cloned();
            tokio::spawn(async move {
                if let Some(ev) = span_map {
                    let mut guard = ev.lock().await;
                    guard.push(event_clone);
                }
            });
        }

        #[cfg(target_arch = "wasm32")]
        {
            let event_clone = event.clone();
            let span_map = self.span_map.get(&span_id).cloned();
            wasm_bindgen_futures::spawn_local(async move {
                if let Some(ev) = span_map {
                    let mut guard = ev.lock().await;
                    guard.push(event_clone);
                }
            });
        }
    }

    /// For testing or debugging – return the entire map of events.
    /// (Use with caution in a real system, as it might be huge.)
    pub fn events(&self) -> HashMap<FunctionId, Arc<Mutex<Vec<Arc<TraceEvent>>>>> {
        self.span_map.clone()
    }

    /// For testing or debugging – return how many references a given function currently has.
    pub fn ref_count_for(&self, function_id: &FunctionId) -> usize {
        self.ref_counts
            .get(function_id)
            .map(|arc| arc.load(Ordering::SeqCst))
            .unwrap_or(0)
    }
}

// ----------------- TESTS -----------------
#[cfg(test)]
mod tests {
    use super::*;
    use baml_types::tracing::events::{ContentId, FunctionId, TraceData, TraceEvent, TraceLevel};
    use core::time::Duration;
    use tokio::runtime::Runtime;

    #[test]
    fn test_reference_count_lifecycle() {
        // Because we're using async calls in `put()`, let's create a Runtime
        // so we can block_on them here in a synchronous test.
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut storage = TraceStorage::new();

            // Create a collector and register it
            let collector = Collector::new();
            let collector_id = storage.register_collector(Arc::new(collector));

            // No references yet
            let f_id = FunctionId("func_abc".to_string());
            assert_eq!(storage.ref_count_for(&f_id), 0);

            // Subscribe the collector – that increments the ref count
            storage.subscribe_collector_to_span(&collector_id, f_id.clone());
            assert_eq!(storage.ref_count_for(&f_id), 1);

            // Put an event – we should store it, because ref_count > 0.
            let event = Arc::new(TraceEvent {
                span_id: f_id.clone(),
                event_id: ContentId("event_abc".to_string()),
                span_chain: vec![],
                timestamp: web_time::SystemTime::now(),
                callsite: "test_event".into(),
                verbosity: TraceLevel::Info,
                content: TraceData::LogMessage {
                    msg: "test_event1".into(),
                },
                tags: Default::default(),
            });
            storage.put(event.clone());

            // The put is async, so let's wait a little for the spawn to push to the vector
            tokio::time::sleep(Duration::from_millis(10)).await;

            let events_opt = storage.get_events(&f_id);
            assert!(events_opt.is_some());
            let events_vec = events_opt.unwrap().lock().await.clone();
            assert_eq!(events_vec.len(), 1);
            match &events_vec[0].content {
                TraceData::LogMessage { msg } => assert_eq!(msg, "test_event1"),
                _ => panic!("Expected LogMessage event"),
            }

            // Now discard the whole collector => refcount goes to 0 => events are removed
            storage.discard_collector(&collector_id);
            assert_eq!(storage.ref_count_for(&f_id), 0);

            // We should have removed the events from memory
            let events_opt2 = storage.get_events(&f_id);
            assert!(events_opt2.is_none());
        });
    }

    #[test]
    fn test_multiple_spans_and_collectors() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut storage = TraceStorage::new();
            let collector1 = Collector::new();
            let coll1_id = storage.register_collector(Arc::new(collector1));

            let collector2 = Collector::new();
            let coll2_id = storage.register_collector(Arc::new(collector2));

            let f1 = FunctionId("func_1".to_string());
            let f2 = FunctionId("func_2".to_string());

            // Subcribe coll1 to f1
            storage.subscribe_collector_to_span(&coll1_id, f1.clone());
            assert_eq!(storage.ref_count_for(&f1), 1);

            // Subscribe coll2 to f1 and f2
            storage.subscribe_collector_to_span(&coll2_id, f1.clone());
            storage.subscribe_collector_to_span(&coll2_id, f2.clone());
            assert_eq!(storage.ref_count_for(&f1), 2);
            assert_eq!(storage.ref_count_for(&f2), 1);

            // Put events in f1 and f2
            let event_f1 = Arc::new(TraceEvent {
                span_id: f1.clone(),
                event_id: ContentId("event_abc".to_string()),
                span_chain: vec![],
                timestamp: web_time::SystemTime::now(),
                callsite: "event_f1".into(),
                verbosity: TraceLevel::Info,
                content: TraceData::LogMessage {
                    msg: "event_f1".into(),
                },
                tags: Default::default(),
            });
            let event_f2 = Arc::new(TraceEvent {
                span_id: f2.clone(),
                event_id: ContentId("event_abc".to_string()),
                span_chain: vec![],
                timestamp: web_time::SystemTime::now(),
                callsite: "event_f2".into(),
                verbosity: TraceLevel::Info,
                content: TraceData::LogMessage {
                    msg: "event_f2".into(),
                },
                tags: Default::default(),
            });
            storage.put(event_f1.clone());
            storage.put(event_f2.clone());

            tokio::time::sleep(Duration::from_millis(10)).await;

            // Each subscribed function ID should have 1 event
            assert_eq!(storage.get_events(&f1).unwrap().lock().await.len(), 1);
            assert_eq!(storage.get_events(&f2).unwrap().lock().await.len(), 1);

            // Now discard coll1 => it was subscribed only to f1
            storage.discard_collector(&coll1_id);
            assert_eq!(storage.ref_count_for(&f1), 1); // from coll2
            assert_eq!(storage.ref_count_for(&f2), 1);

            // If we discard coll2 as well -> ref counts for both f1, f2 become 0 => remove from memory
            storage.discard_collector(&coll2_id);
            assert_eq!(storage.ref_count_for(&f1), 0);
            assert_eq!(storage.ref_count_for(&f2), 0);

            assert!(storage.get_events(&f1).is_none());
            assert!(storage.get_events(&f2).is_none());
        });
    }
}
