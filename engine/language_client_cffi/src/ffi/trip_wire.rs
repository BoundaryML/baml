use baml_runtime::TripWire;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use tokio_util::sync::CancellationToken;

/// Track active operations with their cancellation tokens
static OPERATION_TRIGGERS: Lazy<DashMap<u32, CancellationToken>> = Lazy::new(DashMap::new);

pub fn make_trip_wire(id: u32) -> std::sync::Arc<TripWire> {
    let token = CancellationToken::new();
    OPERATION_TRIGGERS.insert(id, token.clone());

    TripWire::new_with_on_drop(
        Some(token),
        Box::new(move || {
            OPERATION_TRIGGERS.remove(&id);
        }),
    )
}

pub fn cancel(id: u32) {
    if let Some((_, token)) = OPERATION_TRIGGERS.remove(&id) {
        token.cancel();
    }
}
