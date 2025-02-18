use indexmap::{IndexMap, IndexSet};
use once_cell::sync::Lazy;
use std::{
    collections::HashMap,
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

#[derive(Hash, Eq, PartialEq)]
pub struct FunctionLog {
    id: FunctionId,
}

impl FunctionLog {
    pub fn new(id: FunctionId) -> Self {
        BAML_TRACER.blocking_lock().inc_function_id(&id);
        Self { id }
    }

    pub fn id(&self) -> FunctionId {
        self.id.clone()
    }
}

impl Drop for FunctionLog {
    fn drop(&mut self) {
        BAML_TRACER.blocking_lock().dec_function_id(&self.id);
    }
}

pub struct Collector {
    id: String,
    function_logs: Arc<Mutex<IndexSet<Arc<FunctionLog>>>>,
}

impl Collector {
    pub fn new(id: String) -> Self {
        Self {
            id,
            function_logs: Arc::new(Mutex::new(IndexSet::new())),
        }
    }

    pub fn id(&self) -> String {
        self.id.clone()
    }

    pub fn events(&self) -> Vec<Arc<TraceEvent>> {
        let mut all_events = Vec::new();

        // Acquire the global tracer lock in a blocking manner.
        let tracer = BAML_TRACER.blocking_lock();

        // For each subscribed span, grab its events under a blocking lock.
        let function_logs_guard = self.function_logs.blocking_lock();
        for span_id in function_logs_guard.iter() {
            if let Some(events) = tracer.get_events(&span_id.id) {
                let guard = events.blocking_lock();
                all_events.extend(guard.clone());
            }
        }

        all_events
    }

    pub fn function_logs(&self) -> Vec<Arc<FunctionLog>> {
        let function_logs_guard = self.function_logs.blocking_lock();
        function_logs_guard
            .iter()
            .map(|id| Arc::new(FunctionLog::new(id.id.clone())))
            .collect()
    }

    pub fn track_function(&mut self, span_id: FunctionId) {
        let function_log = FunctionLog::new(span_id);
        let mut function_logs_guard = self.function_logs.blocking_lock();
        function_logs_guard.insert(Arc::new(function_log));
    }
}

impl Drop for Collector {
    fn drop(&mut self) {
        log::info!("Dropping collector: {}", self.id);
        let function_logs_guard = self.function_logs.blocking_lock();
        for function_log in function_logs_guard.iter() {
            BAML_TRACER
                .blocking_lock()
                .dec_function_id(&function_log.id);
        }
    }
}

/// Our main storage struct. Holds:
/// 1) A map of FunctionId -> list of events (wrapped in an Arc<Mutex<Vec<Arc<TraceEvent>>>>).
/// 2) A map of FunctionId -> reference count (how many "subscribers" are listening).
#[derive(Default)]
pub struct TraceStorage {
    /// For each function (span), we keep an Arc<Mutex<Vec<Arc<TraceEvent>>>>>.
    /// This lets us append events asynchronously from multiple tasks.
    span_map: HashMap<FunctionId, Arc<Mutex<Vec<Arc<TraceEvent>>>>>,
    /// For each function (span), how many "subscribers" currently have it referenced?
    ref_counts: HashMap<FunctionId, Arc<AtomicUsize>>,
}

impl TraceStorage {
    pub fn new() -> Self {
        Self {
            span_map: HashMap::new(),
            ref_counts: HashMap::new(),
        }
    }

    /// Retrieve events for a particular function (span).
    /// Returns None if the function isn't being tracked (or was dropped).
    pub fn get_events(&self, function_id: &FunctionId) -> Option<Arc<Mutex<Vec<Arc<TraceEvent>>>>> {
        self.span_map.get(function_id).cloned()
    }

    /// Increments the reference count for the given function ID.
    /// Also ensures the underlying events map entry exists.
    pub fn inc_function_id(&mut self, function_id: &FunctionId) {
        // Ensure we have an event vector for that function ID
        self.span_map
            .entry(function_id.clone())
            .or_insert_with(|| Arc::new(Mutex::new(Vec::new())));

        // Ensure we have a reference count, then increment it.
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
    /// If the reference count is 0 (no subscribers), we skip storing it.
    ///
    /// Regardless, we also publish the event so that external systems can receive it.
    pub fn put(&mut self, event: Arc<TraceEvent>) {
        let span_id = event.span_id.clone();

        // Check how many references are present for this span
        let count = self
            .ref_counts
            .get(&span_id)
            .map(|arc| arc.load(Ordering::SeqCst))
            .unwrap_or(0);

        // Always attempt to publish
        if let Err(e) = PUBLISHING_CHANNEL.send(PublisherMessage::Trace(event.clone())) {
            log::error!("Failed to send event to publisher: {:?}", e);
        }

        // If nobody's referencing this span, no need to store it locally
        if count == 0 {
            return;
        }

        // If count > 0, store the event locally in the span_map.
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
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut storage = TraceStorage::new();
            let f_id = FunctionId("func_abc".to_string());

            // Initially, no references
            assert_eq!(storage.ref_count_for(&f_id), 0);

            // Increment reference count
            storage.inc_function_id(&f_id);
            assert_eq!(storage.ref_count_for(&f_id), 1);

            // Put an event – it should be stored in memory (since ref > 0)
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

            // Wait for the async insert to complete
            tokio::time::sleep(Duration::from_millis(10)).await;

            let maybe_events = storage.get_events(&f_id);
            assert!(maybe_events.is_some());

            // Store Arc<Mutex<Vec<Arc<TraceEvent>>>> in a variable before locking
            let arc_mutex = maybe_events.unwrap();
            let event_list = arc_mutex.lock().await;

            assert_eq!(event_list.len(), 1);
            match &event_list[0].content {
                TraceData::LogMessage { msg } => assert_eq!(msg, "test_event1"),
                _ => panic!("Expected a LogMessage content."),
            }

            // Decrement reference count => it should remove the events if count hits 0
            storage.dec_function_id(&f_id);
            assert_eq!(storage.ref_count_for(&f_id), 0);

            // That should also remove the events, so get_events should return None
            assert!(storage.get_events(&f_id).is_none());
        });
    }
}
