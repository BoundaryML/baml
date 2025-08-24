use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc,
};

use dashmap::DashMap;
use once_cell::sync::Lazy;
use pyo3::prelude::*;
use stream_cancel::{Trigger, Tripwire};

// Track active operations with their triggers
static OPERATION_TRIGGERS: Lazy<DashMap<u32, Trigger>> = Lazy::new(DashMap::new);
static NEXT_ID: AtomicU32 = AtomicU32::new(1);

#[pyclass(module = "baml_py.baml_py")]
#[derive(Clone)]
pub struct AbortController {
    id: u32,
    aborted: Arc<AtomicBool>,
}

#[pymethods]
impl AbortController {
    #[new]
    fn new() -> Self {
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            aborted: Arc::new(AtomicBool::new(false)),
        }
    }

    fn abort(&self) -> PyResult<()> {
        self.aborted.store(true, Ordering::Relaxed);

        // Find and trigger all operations associated with this controller
        if let Some((_, trigger)) = OPERATION_TRIGGERS.remove(&self.id) {
            trigger.cancel();
        }

        Ok(())
    }

    #[getter]
    pub fn aborted(&self) -> bool {
        self.aborted.load(Ordering::Relaxed)
    }
}

impl AbortController {
    pub fn create_tripwire(&self) -> Option<Tripwire> {
        if self.aborted.load(Ordering::Relaxed) {
            // Already aborted, return None to signal immediate cancellation
            return None;
        }

        let (trigger, tripwire) = Tripwire::new();
        OPERATION_TRIGGERS.insert(self.id, trigger);
        Some(tripwire)
    }

    pub fn get_id(&self) -> u32 {
        self.id
    }
}
