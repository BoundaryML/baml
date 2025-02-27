//! A full implementation of a manually reference-counted trace storage system,
//! including a global tracer, FunctionLog, Collector, and all related data types.
//!
//! This version ensures we don't allocate multiple copies of the same FunctionLogInner
//! for a single FunctionId, even if multiple Collectors or FunctionLogs want it.
//! It uses manual reference counting (`inc_ref` / `dec_ref`) to free memory for
//! a FunctionId as soon as there are no more "owners."
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use baml_types::tracing::events::{FunctionId, TraceEvent};

/// Global (singleton) trace storage.
pub static BAML_TRACER: Lazy<Mutex<TraceStorage>> =
    Lazy::new(|| Mutex::new(TraceStorage::default()));

/// Our main storage struct. Holds:
/// 1) A map of FunctionId -> list of events (Vec<Arc<TraceEvent>>).
/// 2) A map of FunctionId -> reference count (how many "owners" are tracking it).
/// 3) A cache of FunctionId -> Arc<Mutex<FunctionLogInner>> to avoid rebuilding
///    the same FunctionLogInner multiple times.
#[derive(Default)]
pub struct TraceStorage {
    /// For each function (span), we keep a vector of TraceEvents.
    /// This data is only kept while ref_count > 0.
    span_map: HashMap<FunctionId, Vec<Arc<TraceEvent>>>,
    /// Manual reference count for each function ID. If it hits 0, we remove that ID's data.
    ref_counts: HashMap<FunctionId, usize>,
}

impl fmt::Debug for TraceStorage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TraceStorage {{ ref_counts: {:#?}, function_span_count: {:#?} }}",
            self.ref_counts,
            self.function_span_count()
        )
    }
}

impl TraceStorage {
    /// Increase the reference count for the given FunctionId.
    /// If there's no entry yet, create one (with an empty Vec of events).
    pub fn inc_ref(&mut self, function_id: &FunctionId) {
        // log::trace!("Incrementing ref count for FunctionID {:?}", function_id);
        let count = self.ref_counts.entry(function_id.clone()).or_insert(0);
        *count += 1;

        // Ensure span_map has an entry for the ID; create if not present.
        self.span_map
            .entry(function_id.clone())
            .or_insert_with(Vec::new);
    }

    /// Decrease the reference count for the given FunctionId,
    /// and if it hits zero, remove from memory (both events and cached FunctionLogInner).
    pub fn dec_ref(&mut self, function_id: &FunctionId) {
        // log::info!("Decrementing ref count for FunctionID {:?}", function_id);
        match self.ref_counts.get_mut(function_id) {
            Some(rc) => {
                if *rc == 0 {
                    panic!(
                        "Attempted to decrement ref below 0 for FunctionID {:?}",
                        function_id
                    );
                }
                *rc -= 1;
                // If refcount hits 0, remove from both maps
                if *rc == 0 {
                    self.ref_counts.remove(function_id);
                    self.span_map.remove(function_id);

                    crate::tracingv2::collectors::finish_function_id(function_id);
                }
            }
            None => {
                panic!(
                    "Attempted to decrement ref for FunctionID {:?} (not found)",
                    function_id
                );
            }
        }
        // // log::info!("Decremented ref count for FunctionID {:?}", function_id);
        // log::info!(
        //     "Ref counts: {:?}, span_map_function_count: {:?}",
        //     self.ref_counts,
        //     self.span_map.len()
        // );
    }

    /// Append a new event for the given function ID, but only if ref_count > 0.
    pub fn put(&mut self, event: Arc<TraceEvent>) {
        log::trace!(
            "#####################   Putting event: ############\n {:?}\n\n",
            event
        );
        let Some(&count) = self.ref_counts.get(&event.span_id) else {
            // If no references exist, skip or handle otherwise
            // log::trace!("No references for FunctionID {:?} -- dropping events", event.span_id);
            return;
        };
        if count > 0 {
            if let Some(events_vec) = self.span_map.get_mut(&event.span_id) {
                events_vec.push(event);
            }
        }
    }

    /// Retrieve events for a particular function (span).
    /// Returns None if the function isn't being tracked (or was removed).
    pub fn get_events(&self, function_id: &FunctionId) -> Option<&Vec<Arc<TraceEvent>>> {
        self.span_map.get(function_id)
    }

    /// Returns how many references a given function currently has.
    pub fn ref_count_for(&self, function_id: &FunctionId) -> usize {
        self.ref_counts.get(function_id).copied().unwrap_or(0)
    }

    pub fn function_span_count(&self) -> usize {
        self.span_map.len()
    }

    /// For debugging – return a copy of all events in memory.
    pub fn events(&self) -> HashMap<FunctionId, Vec<Arc<TraceEvent>>> {
        self.span_map.clone()
    }

    pub fn clear(&mut self) {
        self.span_map.clear();
        self.ref_counts.clear();
    }
}

pub trait FunctionTrackerTrait: Send + Sync {
    /// Track a function (this object will hold a reference to it)
    fn track_function(&self, fid: FunctionId);
    /// Untrack a function (this object will no longer hold a reference to it)
    fn untrack_function(&self, fid: &FunctionId);
}
