//! The engine's telemetry definition resolver. The recorder polls it on the
//! processor thread, which must keep draining transport no matter what the
//! heap is doing: a VM holding a permit can be waiting for transport space
//! while a collection waits for that VM. So the resolver never waits for heap
//! access. It takes a permit only when one is free right now, copies a
//! bounded batch of definitions into owned values, and releases the permit
//! before handing anything over; otherwise it answers `Busy` and keeps its
//! work. Definitions of declarations collected meanwhile were extracted by
//! the collector and need no permit at all.

use std::sync::{Arc, Mutex};

use bex_heap::{BexHeap, HeapPermit, HeapPermitManager, InactiveHeapPermit};
use btel_types::{TypeDefinitionSource, TypeResolution};

pub(crate) struct HeapTypeDefinitions {
    heap: Arc<BexHeap>,
    permits: Arc<HeapPermitManager>,
    /// Registered once, kept inactive between polls. It holds no roots.
    permit: Mutex<Option<InactiveHeapPermit<()>>>,
}

impl HeapTypeDefinitions {
    pub(crate) fn new(heap: Arc<BexHeap>, permits: Arc<HeapPermitManager>) -> Self {
        Self {
            heap,
            permits,
            permit: Mutex::new(None),
        }
    }
}

impl TypeDefinitionSource for HeapTypeDefinitions {
    fn try_take(&self, max: usize) -> TypeResolution {
        let mut definitions = self.heap.take_settled_type_definitions();
        let ready = |definitions, more| TypeResolution::Ready { definitions, more };
        if !self.heap.has_type_definition_work() {
            return ready(definitions, false);
        }
        let busy = |definitions: Vec<_>| {
            if definitions.is_empty() {
                TypeResolution::Busy
            } else {
                ready(definitions, true)
            }
        };
        let Ok(mut slot) = self.permit.try_lock() else {
            return busy(definitions);
        };
        let inactive = match slot.take() {
            Some(permit) => permit,
            None => match self.permits.try_new_permit(()) {
                Some(permit) => permit,
                None => return busy(definitions),
            },
        };
        match inactive.try_acquire() {
            Ok(active) => {
                definitions.extend(self.heap.resolve_type_definitions(active.proof(), max));
                // Release before anything leaves: the copies are owned.
                *slot = Some(active.release());
                let more = self.heap.has_type_definition_work();
                ready(definitions, more)
            }
            Err(inactive) => {
                *slot = Some(inactive);
                busy(definitions)
            }
        }
    }
}
