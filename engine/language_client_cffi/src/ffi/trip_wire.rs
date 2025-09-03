use baml_runtime::TripWire;
use dashmap::DashMap;
use once_cell::sync::Lazy;

/// Track active operations with their cancellation triggers
static OPERATION_TRIGGERS: Lazy<DashMap<u32, stream_cancel::Trigger>> = Lazy::new(DashMap::new);

pub fn make_trip_wire(id: u32) -> std::sync::Arc<TripWire> {
    // Create a cancellation trigger/tripwire pair
    let (trigger, tripwire) = stream_cancel::Tripwire::new();
    OPERATION_TRIGGERS.insert(id, trigger);

    TripWire::new_with_on_drop(
        Some(tripwire),
        Box::new(move || {
            OPERATION_TRIGGERS.remove(&id);
        }),
    )
}

pub fn cancel(id: u32) {
    if let Some((_, trigger)) = OPERATION_TRIGGERS.remove(&id) {
        trigger.cancel();
    }
}
