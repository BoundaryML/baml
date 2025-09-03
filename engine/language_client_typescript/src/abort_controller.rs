use std::sync::Arc;

use baml_runtime::TripWire;
use dashmap::DashMap;
use napi::{Env, JsFunction, JsObject, JsUnknown};
use once_cell::sync::Lazy;
use stream_cancel::Trigger;

// Track active operations with their cancellation triggers
static OPERATION_TRIGGERS: Lazy<DashMap<u32, Trigger>> = Lazy::new(DashMap::new);

// Counter for unique operation IDs
static OPERATION_ID_COUNTER: Lazy<std::sync::atomic::AtomicU32> =
    Lazy::new(|| std::sync::atomic::AtomicU32::new(0));

/// Convert a JavaScript AbortSignal to a Rust cancellation mechanism
/// Returns (operation_id, tripwire) where operation_id is used to track the operation
pub fn js_abort_signal_to_rust_tripwire(
    env: Env,
    signal: Option<JsObject>,
) -> napi::Result<Arc<baml_runtime::TripWire>> {
    let Some(signal) = signal else {
        return Ok(TripWire::new(None));
    };

    // Generate a unique operation ID
    let operation_id = OPERATION_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

    // Create the trigger and tripwire
    let (trigger, tripwire) = stream_cancel::Tripwire::new();
    let tripwire = TripWire::new_with_on_drop(
        Some(tripwire),
        Box::new(move || {
            OPERATION_TRIGGERS.remove(&operation_id);
        }),
    );

    // Check if already aborted
    let aborted: bool = signal.get_named_property("aborted")?;
    if aborted {
        trigger.cancel();
        return Ok(tripwire);
    } else {
        OPERATION_TRIGGERS.insert(operation_id, trigger);
    }

    // Store the trigger for potential cancellation

    // Listen to 'abort' event
    let callback = env.create_function_from_closure("abort_handler", move |_| {
        // Cancel the operation when abort is triggered
        if let Some((_, trigger)) = OPERATION_TRIGGERS.remove(&operation_id) {
            trigger.cancel();
        }
        Ok(())
    })?;

    // signal.addEventListener('abort', callback)
    let add_event_listener: JsFunction = signal.get_named_property("addEventListener")?;
    add_event_listener.call(
        Some(&signal),
        &[
            env.create_string("abort")?.into_unknown(),
            callback.into_unknown(),
        ],
    )?;

    Ok(tripwire)
}
