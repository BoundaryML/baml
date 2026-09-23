//! Owned call configuration and one-shot span reservations.
//!
//! Decoded calls contain no GC values. Reservations keep only a weak runtime
//! owner, so retaining an unattached reservation cannot keep its heap alive.

use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, Ordering},
};

use bex_heap::BexHeap;
use bex_vm_types::types::Value;
use btel_types::{InvocationMode, TelemetryId, allocate_telemetry_id};

use crate::{BexVm, errors::VmPanic};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TraceOptions {
    pub mode: Option<InvocationMode>,
    pub inputs: Option<bool>,
    pub output: Option<bool>,
    pub error: Option<bool>,
}

/// IDs are allocated process-wide and never reused, including across runtimes.
/// These opaque values are not serialized or accepted from another process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SpanId(pub TelemetryId);

#[derive(Debug)]
struct Reservation {
    id: TelemetryId,
    owner: Weak<BexHeap>,
    consumed: AtomicBool,
}

#[derive(Clone, Debug, Default)]
pub struct TraceCall {
    pub options: TraceOptions,
    reservation: Option<Arc<Reservation>>,
}

impl TraceCall {
    pub(crate) fn reserve(mut options: TraceOptions, heap: &Arc<BexHeap>) -> Self {
        options.mode = Some(InvocationMode::Span);
        Self {
            options,
            reservation: Some(Arc::new(Reservation {
                id: allocate_telemetry_id(),
                owner: Arc::downgrade(heap),
                consumed: AtomicBool::new(false),
            })),
        }
    }

    pub(crate) fn id(&self) -> Option<TelemetryId> {
        self.reservation.as_ref().map(|reservation| reservation.id)
    }

    /// Consume a reservation only when attaching it to the actual invocation.
    /// Clones share the one-shot state. A wrong runtime does not consume it.
    pub fn attach(
        &self,
        heap: &Arc<BexHeap>,
    ) -> Result<(TraceOptions, Option<TelemetryId>), VmPanic> {
        let Some(reservation) = &self.reservation else {
            return Ok((self.options, None));
        };
        if !Weak::ptr_eq(&reservation.owner, &Arc::downgrade(heap)) {
            return Err(trace_error("span reservation belongs to another runtime"));
        }
        if reservation.consumed.swap(true, Ordering::AcqRel) {
            return Err(trace_error("span reservation has already been attached"));
        }
        Ok((self.options, Some(reservation.id)))
    }
}

fn trace_error(message: &str) -> VmPanic {
    VmPanic::UserPanic {
        message: message.to_owned(),
    }
}

impl BexVm {
    /// Decode options while the argument is rooted; the result owns no GC values.
    pub fn decode_trace_options(&self, value: Value) -> Result<TraceCall, VmPanic> {
        let invalid = || trace_error("$trace expects trace.Options or trace.ReservedSpan");
        let instance = self.as_instance(&value).map_err(|_| invalid())?;
        if instance.class == self.resolve_class("trace.Options") {
            let options = self
                .as_rust_data::<TraceOptions>(&instance.load_field(0))
                .map_err(|_| invalid())?;
            Ok(TraceCall {
                options: *options,
                reservation: None,
            })
        } else if instance.class == self.resolve_class("trace.ReservedSpan") {
            self.as_rust_data::<TraceCall>(&instance.load_field(0))
                .cloned()
                .map_err(|_| invalid())
        } else {
            Err(invalid())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Runtime ownership and host-thread races cannot be exercised from BAML.
    #[test]
    fn wrong_runtime_does_not_consume_reservation() {
        let owner = BexHeap::new(Vec::new());
        let other = BexHeap::new(Vec::new());
        let call = TraceCall::reserve(TraceOptions::default(), &owner);
        let alias = call.clone();
        assert!(call.attach(&other).is_err());
        let (options, id) = call.attach(&owner).unwrap();
        assert_eq!(options.mode, Some(InvocationMode::Span));
        assert_eq!(id, call.id());
        assert!(alias.attach(&owner).is_err());
    }

    #[test]
    fn reservation_does_not_retain_heap() {
        let owner = BexHeap::new(Vec::new());
        let weak = Arc::downgrade(&owner);
        let call = TraceCall::reserve(TraceOptions::default(), &owner);
        drop(owner);
        assert!(weak.upgrade().is_none());
        assert!(call.id().is_some());
    }

    #[test]
    fn concurrent_aliases_attach_exactly_once() {
        let owner = BexHeap::new(Vec::new());
        let call = TraceCall::reserve(TraceOptions::default(), &owner);
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let call = call.clone();
                let owner = Arc::clone(&owner);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    call.attach(&owner).is_ok()
                })
            })
            .collect();
        assert_eq!(
            threads
                .into_iter()
                .map(|thread| usize::from(thread.join().unwrap()))
                .sum::<usize>(),
            1
        );
    }
}
