use once_cell::sync::Lazy;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

// Bring in our definitions for our event types.
// (In your real code these come from the baml_types crate)
use baml_types::tracing::events::{FunctionId, TraceEvent};

use crate::tracingv2::publisher::publisher::PublisherMessage;

use super::super::publisher::PUBLISHING_CHANNEL;

pub static BAML_TRACER: Lazy<Mutex<TraceStorage>> =
    Lazy::new(|| Mutex::new(TraceStorage::default()));

#[derive(Default)]
pub struct TraceStorage {
    // Lookup of span id to trace events
    span_map: HashMap<FunctionId, Arc<Mutex<Vec<Arc<TraceEvent>>>>>,
}

// TODO: dont spawn new threads for each event. Use a channel.
impl TraceStorage {
    pub fn new() -> Self {
        Self {
            span_map: HashMap::new(),
        }
    }

    /// Retrieve events for a particular span id.
    pub fn get(&self, span_id: FunctionId) -> Option<Arc<Mutex<Vec<Arc<TraceEvent>>>>> {
        self.span_map.get(&span_id).cloned()
    }

    /// Insert a new event – note that we update the local store asynchronously
    /// and then publish the event immediately.
    pub fn put(&mut self, event: Arc<TraceEvent>) {
        let span_id = event.span_id.clone();

        // Ensure our local map has an entry for the span.
        self.span_map
            .entry(span_id.clone())
            .or_insert_with(|| Arc::new(Mutex::new(Vec::new())));

        // Asynchronously push the event into the span's event list.
        // (We use `tokio::spawn` for macOS and normal native runtimes;
        // on wasm, use spawn_local.)
        #[cfg(not(target_arch = "wasm32"))]
        {
            let event_clone = event.clone();
            let span_map = self.span_map.clone();
            tokio::spawn(async move {
                if let Some(mutex) = span_map.get(&span_id) {
                    let mut events = mutex.lock().await;
                    events.push(event_clone);
                }
            });
        }

        #[cfg(target_arch = "wasm32")]
        {
            let event_clone = event.clone();
            let span_map = self.span_map.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if let Some(mutex) = span_map.get(&span_id) {
                    let mut events = mutex.lock().await;
                    events.push(event_clone);
                }
            });
        }

        // Publish this event so that the TracePublisher can process it.
        if let Err(e) = PUBLISHING_CHANNEL.send(PublisherMessage::Trace(event.clone())) {
            log::error!("Failed to send event to publisher: {:?}", e);
        }
    }

    pub fn events(&self) -> HashMap<FunctionId, Arc<Mutex<Vec<Arc<TraceEvent>>>>> {
        return self.span_map.clone();
    }
}
