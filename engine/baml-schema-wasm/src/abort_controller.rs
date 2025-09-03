use std::{cell::RefCell, collections::HashMap};

use baml_runtime::TripWire;
use stream_cancel::Trigger;
use wasm_bindgen::prelude::*;
use web_sys::AbortSignal;

thread_local! {
    static ABORT_CLOSURES: RefCell<HashMap<u32, Closure<dyn Fn()>>> = RefCell::new(HashMap::new());
    static OPERATION_TRIGGERS: RefCell<HashMap<u32, Trigger>> = RefCell::new(HashMap::new());
    static OPERATION_ID_COUNTER: RefCell<u32> = const { RefCell::new(0) };
}

pub fn js_abort_signal_to_tripwire(
    signal: Option<js_sys::Object>,
) -> Result<std::sync::Arc<baml_runtime::TripWire>, JsError> {
    let Some(signal) = signal else {
        return Ok(TripWire::new(None));
    };

    let abort_signal: AbortSignal = signal
        .dyn_into()
        .map_err(|_| JsError::new("Expected AbortSignal"))?;

    let operation_id = OPERATION_ID_COUNTER.with(|counter| {
        let mut c = counter.borrow_mut();
        let id = *c;
        *c += 1;
        id
    });

    let (trigger, tripwire) = stream_cancel::Tripwire::new();

    // Early abort check
    if abort_signal.aborted() {
        trigger.cancel();
        return Ok(TripWire::new(Some(tripwire)));
    }

    // Store the trigger for later cancellation
    OPERATION_TRIGGERS.with(|triggers| {
        triggers.borrow_mut().insert(operation_id, trigger);
    });
    let op_id = operation_id;
    let closure = Closure::wrap(Box::new(move || {
        // Cancel the operation when abort is triggered
        OPERATION_TRIGGERS.with(|triggers| {
            if let Some(trigger) = triggers.borrow_mut().remove(&op_id) {
                trigger.cancel();
            }
        });
        // Self-cleanup after firing
        ABORT_CLOSURES.with(|closures| {
            closures.borrow_mut().remove(&op_id);
        });
    }) as Box<dyn Fn()>);

    // Set up event listener
    abort_signal.set_onabort(Some(closure.as_ref().unchecked_ref()));

    // Store closure to prevent deallocation
    ABORT_CLOSURES.with(|closures| {
        closures.borrow_mut().insert(operation_id, closure);
    });

    let tripwire = TripWire::new_with_on_drop(
        Some(tripwire),
        Box::new(move || cleanup_operation(operation_id)),
    );

    Ok(tripwire)
}

fn cleanup_operation(operation_id: u32) {
    ABORT_CLOSURES.with(|closures| {
        closures.borrow_mut().remove(&operation_id);
    });
    OPERATION_TRIGGERS.with(|triggers| {
        triggers.borrow_mut().remove(&operation_id);
    });
}
