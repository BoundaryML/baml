use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};

use baml_runtime::TripWire;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use pyo3::prelude::*;
use stream_cancel::Trigger;

// Track active operations with their triggers
static OPERATION_TRIGGERS: Lazy<DashMap<u32, Trigger>> = Lazy::new(DashMap::new);
static NEXT_ID: AtomicU32 = AtomicU32::new(1);

#[pyclass(module = "baml_py.baml_py")]
#[derive(Clone)]
pub struct AbortController {
    id: u32,
    aborted: Arc<AtomicBool>,
    timeout: Option<Duration>,
}

#[pymethods]
impl AbortController {
    #[new]
    #[pyo3(signature = (timeout_ms=None))]
    fn new(timeout_ms: Option<u64>) -> Self {
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            aborted: Arc::new(AtomicBool::new(false)),
            timeout: timeout_ms.map(Duration::from_millis),
        }
    }

    fn abort(&self) -> PyResult<()> {
        self.aborted.store(true, Ordering::Relaxed);
        abort(self.id);
        Ok(())
    }

    #[getter]
    pub fn aborted(&self) -> bool {
        self.aborted.load(Ordering::Relaxed)
    }
}

impl AbortController {
    pub fn create_tripwire(&self) -> Arc<baml_runtime::TripWire> {
        if self.aborted.load(Ordering::Relaxed) {
            // Already aborted, return None to signal immediate cancellation
            return TripWire::new(None);
        }

        let (trigger, tripwire) = stream_cancel::Tripwire::new();
        OPERATION_TRIGGERS.insert(self.id, trigger);
        let id = self.id;
        if let Some(timeout) = &self.timeout {
            let timeout = *timeout;
            let aborted = self.aborted.clone();
            let _ = std::thread::spawn(move || {
                std::thread::sleep(timeout);
                abort(id);
                aborted.store(true, Ordering::Relaxed);
            });
        }

        TripWire::new_with_on_drop(
            Some(tripwire),
            Box::new(move || {
                OPERATION_TRIGGERS.remove(&id);
            }),
        )
    }
}

impl Drop for AbortController {
    fn drop(&mut self) {
        OPERATION_TRIGGERS.remove(&self.id);
    }
}

fn abort(id: u32) {
    if let Some((_, trigger)) = OPERATION_TRIGGERS.remove(&id) {
        trigger.cancel();
    }
}
